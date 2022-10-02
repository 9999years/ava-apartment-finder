#![allow(dead_code)]

use std::collections::BTreeMap;
use std::fmt::Display;
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;
use std::time::Duration;

use clap::Parser;
use color_eyre::eyre;
use color_eyre::eyre::eyre;
use color_eyre::eyre::Context;
use quick_js::Context as QuickJs;
use serde::Deserialize;
use serde::Serialize;
use soup::prelude::*;

mod api;
mod ava_date;
mod diff;
mod tracing_format;
mod wrap;

const DATA_PATH: &str = "ava_db.json";

const AVA_URL: &str =
    "https://new.avaloncommunities.com/washington/seattle-apartments/ava-capitol-hill/";

const JS_PREFIX: &str = "window = {}; \
                         window.Fusion = {}; \
                         Fusion = window.Fusion; ";
const JS_SUFFIX: &str = "JSON.stringify(Fusion.globalContent)";

const SECONDS_PER_MINUTE: u64 = 50;

#[derive(Parser)]
struct Args {
    #[clap(long, default_value = "info")]
    tracing_filter: String,
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let args = Args::parse();
    install_tracing(&args.tracing_filter);
    let data_path = Path::new(&DATA_PATH);
    let mut app: App = if data_path.exists() {
        tracing::info!(path = ?data_path, "DB path exists, reading");
        serde_json::from_str(
            &std::fs::read_to_string(&data_path)
                .wrap_err_with(|| format!("Failed to read `{data_path:?}`"))?,
        )
        .wrap_err_with(|| format!("Failed to load Apartment data from `{data_path:?}`"))?
    } else {
        tracing::info!(path = ?data_path, "No DB, initializing");
        App::default()
    };

    tracing::info!("Tracking {} apartments", app.known_apartments.len());

    loop {
        match app.tick().await {
            Ok(()) => {}
            Err(err) => {
                tracing::error!("{err:?}");
            }
        }
        // Wait 5 minutes before checking again.
        tokio::time::sleep(Duration::from_secs(5 * SECONDS_PER_MINUTE)).await;
    }
}

/// Initialize the logging framework.
fn install_tracing(filter_directives: &str) {
    use tracing_subscriber::fmt;
    use tracing_subscriber::prelude::*;
    use tracing_subscriber::EnvFilter;

    let fmt_layer = fmt::layer().event_format(tracing_format::EventFormatter::default());
    let filter_layer = EnvFilter::try_new(filter_directives)
        .or_else(|_| EnvFilter::try_from_default_env())
        .or_else(|_| EnvFilter::try_new("info"))
        .unwrap();

    tracing_subscriber::registry()
        .with(fmt_layer)
        .with(filter_layer)
        .init();
}

async fn get_apartments() -> eyre::Result<api::ApartmentData> {
    let response = reqwest::get(AVA_URL).await?;

    tracing::trace!(?response, "Got response");

    let body = response.text().await?;

    tracing::trace!(html = body, "Got HTML");

    let soup = Soup::new(&body);

    let script_tag = soup
        .tag("script")
        .attr("id", "fusion-metadata")
        .find()
        .ok_or_else(|| eyre!("Could not find `<script id=\"fusion-metadata\">` tag"))?
        .text();

    let script = format!("{JS_PREFIX}{script_tag}{JS_SUFFIX}");

    tracing::trace!(script, "Extracted JavaScript");

    let quick_js = QuickJs::new().unwrap();

    let value = quick_js.eval_as::<String>(&script)?;

    tracing::trace!(value, "Evaluated JavaScript");

    Ok(serde_json::from_str(&value)?)
}

// --

#[derive(Clone, Debug, Default)]
struct ApartmentsDiff {
    added: Vec<api::Apartment>,
    removed: Vec<api::Apartment>,
    changed: Vec<ChangedApartment>,
}

impl ApartmentsDiff {
    fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.changed.is_empty()
    }
}

#[derive(Clone, Debug)]
struct ChangedApartment {
    old: api::Apartment,
    new: api::Apartment,
}

impl Display for ChangedApartment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self { old, new } = self;
        write!(
            f,
            "{}",
            diff::diff_header(
                &format!("{old:#?}"),
                &format!("{new:#?}"),
                &old.to_string(),
                &new.to_string(),
            )
            .unwrap_or_else(|err| format!("{err:?}"))
        )
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct App {
    known_apartments: BTreeMap<String, api::Apartment>,
}

impl App {
    /// One 'tick' of the app. Get new apartment data and report changes.
    async fn tick(&mut self) -> eyre::Result<()> {
        let diff = self.compute_diff().await?;

        if diff.is_empty() {
            tracing::debug!(total_available = self.known_apartments.len(), "No news :(");
        } else {
            tracing::info!(
                total_available = self.known_apartments.len(),
                added = diff.added.len(),
                removed = diff.removed.len(),
                changed = diff.changed.len(),
                "Data has changed!"
            );

            if !diff.added.is_empty() {
                tracing::info!(
                    "Newly listed apartments:\n{}",
                    to_bullet_list(diff.added.iter())
                );
            }

            if !diff.removed.is_empty() {
                tracing::info!(
                    "Unlisted apartments:\n{}",
                    to_bullet_list(diff.removed.iter())
                );
            }

            if !diff.changed.is_empty() {
                tracing::info!(
                    "Changed apartments:\n{}",
                    to_bullet_list(diff.changed.iter().map(|c| c.new.clone()))
                );
            }
        }

        let data_file =
            File::create(&DATA_PATH).wrap_err_with(|| format!("Failed to open {DATA_PATH:?}"))?;
        serde_json::to_writer_pretty(BufWriter::new(data_file), self)
            .wrap_err("Failed to write DB")?;

        Ok(())
    }

    /// Fetch new apartment data, update `known_apartments` to include it, and return the
    /// changes with the previous `known_apartments`.
    async fn compute_diff(&mut self) -> eyre::Result<ApartmentsDiff> {
        let new_data = get_apartments().await?;
        let mut diff = ApartmentsDiff::default();
        // A clone of `known_apartments`. We remove each apartment in the _new_
        // data from this map to compute the set of apartments present in the previous
        // data and not present now; that is, the set of apartments that have been
        // _unlisted_.
        let mut removed: BTreeMap<_, _> = std::mem::take(&mut self.known_apartments);

        for unit in new_data.units {
            // Did we have any data for this apartment already?
            // Remember we have the old apartments (minus the ones we've already seen
            // in the new data) in `removed`.
            match removed.get(&unit.unit_id) {
                Some(known_unit) => {
                    // We already have data for an apartment with the same `unit_id`.
                    if &unit != known_unit {
                        // It's different data! Show what changed.
                        let changed = ChangedApartment {
                            old: known_unit.clone(),
                            new: unit.clone(),
                        };
                        // Mark this apartment as changed.
                        diff.changed.push(changed);
                    }
                    // No new data.
                }
                None => {
                    // A new apartment!!!
                    diff.added.push(unit.clone());
                }
            }

            // This unit is still listed, so it wasn't removed.
            removed.remove(&unit.unit_id);
            // Update our data.
            self.known_apartments.insert(unit.unit_id.clone(), unit);
        }

        diff.removed
            .extend(removed.into_iter().map(|(_, unit)| unit));

        Ok(diff)
    }
}

fn to_bullet_list(iter: impl Iterator<Item = impl Display>) -> String {
    itertools::join(iter.map(|unit| format!("• {unit}")), "\n")
}
