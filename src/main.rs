#![allow(dead_code)]

use std::collections::BTreeMap;
use std::fmt::Display;
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;
use std::time::Duration;

use chrono::Utc;
use clap::Parser;
use color_eyre::eyre;
use color_eyre::eyre::eyre;
use color_eyre::eyre::Context;
use serde::Deserialize;
use serde::Serialize;
use soup::prelude::*;

mod api;
mod ava_date;
mod diff;
mod jmap;
mod node;
mod trace;
mod wrap;

const DATA_PATH: &str = "ava_db.json";

const AVA_URL: &str =
    "https://new.avaloncommunities.com/washington/seattle-apartments/ava-capitol-hill/";

const JS_PREFIX: &str = "window = {}; \
                         window.Fusion = {}; \
                         Fusion = window.Fusion; ";
const JS_SUFFIX: &str = "console.log(JSON.stringify(Fusion.globalContent))";

const SECONDS_PER_MINUTE: u64 = 50;

#[derive(Parser)]
struct Args {
    #[clap(long, default_value = "info")]
    tracing_filter: String,
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    color_eyre::install()?;
    let args = Args::parse();
    let log_file = trace::install_tracing(&args.tracing_filter)?;
    tracing::info!("Logging to {log_file}");

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

#[tracing::instrument]
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

    let value = node::js_eval(script)?;

    tracing::trace!(value, "Evaluated JavaScript");

    Ok(serde_json::from_str(&value)?)
}

// --

#[derive(Clone, Debug, Default)]
struct ApartmentsDiff {
    added: Vec<api::ApiApartment>,
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
    old: api::ApiApartment,
    new: api::ApiApartment,
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
    unlisted_apartments: BTreeMap<String, api::Apartment>,
}

impl App {
    /// One 'tick' of the app. Get new apartment data and report changes.
    #[tracing::instrument(skip(self))]
    async fn tick(&mut self) -> eyre::Result<()> {
        let diff = self.compute_diff().await?;

        if diff.is_empty() {
            tracing::debug!(total_available = self.known_apartments.len(), "No news :(");
        } else {
            tracing::debug!(
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

                for unit in diff.added {
                    // if unit.meets_qualifications() {}
                    jmap::Email {
                        to: ("Rebecca Turner", "rbt@fastmail.com").into(),
                        from: ("Ava Apartment Finder", "rbt@fastmail.com").into(),
                        subject: format!(
                            "Apartment {} listed, available {}",
                            unit.number,
                            unit.available_date.format("%b %e %Y"),
                        ),
                        body: format!("{unit}"),
                    }
                    .send()
                    .await?;
                }
            }

            if !diff.removed.is_empty() {
                tracing::info!(
                    "Unlisted apartments:\n{}",
                    to_bullet_list(diff.removed.iter())
                );

                for unit in diff.removed {
                    match unit.unlisted {
                        None => {
                            tracing::warn!(apartment = ?unit, "Weird that apartment in `diff.removed` has no `unlisted` field");
                        }
                        Some(unlisted) => {
                            let tracked_duration = unlisted - unit.listed;
                            jmap::Email {
                                to: ("Rebecca Turner", "rbt@fastmail.com").into(),
                                from: ("Ava Apartment Finder", "rbt@fastmail.com").into(),
                                subject: format!(
                                    "Apartment {} no longer available!",
                                    unit.inner.number
                                ),
                                body: format!(
                                    "{unit}\nTracked since: {}\nTracked for: {} days",
                                    unit.listed,
                                    tracked_duration.num_days()
                                ),
                            }
                            .send()
                            .await?;
                        }
                    }
                }
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
    #[tracing::instrument]
    async fn compute_diff(&mut self) -> eyre::Result<ApartmentsDiff> {
        let new_data = get_apartments().await?;
        let mut diff = ApartmentsDiff::default();
        // A clone of `known_apartments`. We remove each apartment in the _new_
        // data from this map to compute the set of apartments present in the previous
        // data and not present now; that is, the set of apartments that have been
        // _unlisted_.
        let mut removed: BTreeMap<_, _> = std::mem::take(&mut self.known_apartments);

        for mut apt in new_data.apartments {
            // Did we have any data for this apartment already?
            // Remember we have the old apartments (minus the ones we've already seen
            // in the new data) in `removed`.
            match removed.get(apt.id()) {
                Some(known_unit) => {
                    // This apartment wasn't listed now, so copy the listed
                    // time from the old data, as the
                    // `impl TryFrom<api::ApartmentData> for api::ApartmentData`
                    // just... inserts the current time!
                    apt.listed = known_unit.listed;
                    // We already have data for an apartment with the same `unit_id`.
                    if &apt.inner != &known_unit.inner {
                        // It's different data! Show what changed.
                        let changed = ChangedApartment {
                            old: known_unit.inner.clone(),
                            new: apt.inner.clone(),
                        };
                        // Mark this apartment as changed.
                        diff.changed.push(changed);
                    }
                    // No new data.
                }
                None => {
                    // A new apartment!!!
                    diff.added.push(apt.inner.clone());
                }
            }

            // This unit is still listed, so it wasn't removed.
            removed.remove(apt.id());
            // Update our data.
            self.known_apartments.insert(apt.id().to_owned(), apt);
        }

        for (_, mut unit) in removed.iter_mut() {
            unit.unlisted = Some(Utc::now());
        }

        diff.removed
            .extend(removed.iter().map(|(_, unit)| unit.clone()));

        // Note when each apartment was unlisted.
        self.unlisted_apartments.extend(removed.into_iter());

        Ok(diff)
    }
}

fn to_bullet_list(iter: impl Iterator<Item = impl Display>) -> String {
    itertools::join(iter.map(|unit| format!("• {unit}")), "\n")
}
