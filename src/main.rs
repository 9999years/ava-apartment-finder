#![allow(dead_code)]

use std::collections::BTreeMap;
use std::fmt::Display;
use std::path::Path;
use std::time::Duration;

use chrono::DateTime;
use chrono::Utc;
use clap::Parser;
use color_eyre::eyre;
use color_eyre::eyre::eyre;
use color_eyre::eyre::Context;
use quick_js::Context as QuickJs;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use soup::prelude::*;

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
        tracing::debug!(path = ?data_path, "DB path exists, reading");
        serde_json::from_str(
            &std::fs::read_to_string(&data_path)
                .wrap_err_with(|| format!("Failed to read `{data_path:?}`"))?,
        )
        .wrap_err_with(|| format!("Failed to load Apartment data from `{data_path:?}`"))?
    } else {
        tracing::info!(path = ?data_path, "No DB, initializing");
        App::default()
    };

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

async fn get_apartments() -> eyre::Result<ApartmentData> {
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

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ApartmentData {
    units: Vec<Apartment>,
    promotions: Vec<Promotion>,
    pricing_overview: Vec<PricingOverview>,
    #[serde(flatten)]
    extra: Value,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct Apartment {
    unit_id: String,
    #[serde(rename = "name")]
    number: String,
    #[serde(rename = "furnishStatus")]
    furnished: Furnished,
    floor_plan: FloorPlan,
    virtual_tour: Option<VirtualTour>,
    bedroom: usize,
    bathroom: usize,
    square_feet: f64,
    #[serde(with = "ava_date")]
    available_date: DateTime<Utc>,
    #[serde(rename = "unitRentPrice")]
    rent: Rent,
    #[serde(rename = "lowestPricePerMoveInDate")]
    lowest_rent: LowestRent,

    #[serde(flatten)]
    extra: Value,
}

impl Apartment {
    fn meets_qualifications(&self) -> bool {
        if let Furnished::Furnished = self.furnished {
            tracing::debug!(number = self.number, "Skipping apartment; furnished");
            false
        } else if self.bedroom != 2 {
            tracing::debug!(
                number = self.number,
                bedrooms = self.bedroom,
                bathrooms = self.bathroom,
                rent = self.lowest_rent.price.price,
                "Skipping apartment; too few bedrooms"
            );
            false
        } else {
            true
        }
    }
}

impl Display for Apartment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Apartment {
            number,
            floor_plan,
            virtual_tour,
            bedroom,
            bathroom,
            square_feet,
            available_date,
            furnished,
            lowest_rent,
            ..
        } = self;
        let price = lowest_rent.price.price;
        let available_date = &available_date.date();
        let floor_plan = &floor_plan.name;
        let virtual_tour = match virtual_tour {
            Some(virtual_tour) if virtual_tour.is_actual_unit => ", virtual tour",
            _ => "",
        };
        let furnished = match furnished {
            Furnished::Unfurnished => "",
            Furnished::OnDemand => "",
            Furnished::Furnished => ", furnished",
        };
        write!(
            f,
            "Apartment {number} \
             ({bedroom} bed {bathroom} bath, \
             ${price}, \
             {square_feet}sq/ft, \
             avail. {available_date}, \
             plan {floor_plan}\
             {furnished}\
             {virtual_tour}\
             )"
        )
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
enum Furnished {
    Unfurnished,
    OnDemand,
    #[serde(rename = "Designated")]
    Furnished,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct FloorPlan {
    name: String,
    low_resolution: String,
    high_resolution: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct VirtualTour {
    space: String,
    is_actual_unit: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct Rent {
    applied_discount: f64,
    prices_per_movein_date: Vec<PricesForMoveInDate>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct PricesForMoveInDate {
    #[serde(with = "ava_date")]
    move_in_date: DateTime<Utc>,
    prices_per_terms: BTreeMap<usize, Price>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct Price {
    price: f64,
    net_effective_price: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct LowestRent {
    #[serde(with = "ava_date")]
    date: DateTime<Utc>,

    // Shoulda been a usize
    term_length: String,

    #[serde(flatten)]
    price: Price,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
struct Promotion {
    #[serde(rename = "promotionId")]
    id: String,
    #[serde(rename = "promotionTitle")]
    title: String,
    #[serde(rename = "promotionDescription")]
    description: String,
    #[serde(rename = "promotionDisclaimer")]
    disclaimer: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct ApplicablePromotion {
    promotion_id: String,
    #[serde(with = "ava_date")]
    start_date: DateTime<Utc>,
    #[serde(with = "ava_date")]
    end_date: DateTime<Utc>,
    terms: Vec<usize>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct PricingOverview {
    display_name: String,
    bedroom: usize,
    r#type: String,
    available: bool,
    designated_lowest_price: f64,
    on_demand_lowest_price: Option<f64>,
    total_lowest_price: f64,
    total_highest_price: f64,
}

// --

#[derive(Clone, Debug, Default)]
struct ApartmentsDiff {
    added: Vec<Apartment>,
    removed: Vec<Apartment>,
    changed: Vec<ChangedApartment>,
}

impl ApartmentsDiff {
    fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.changed.is_empty()
    }
}

#[derive(Clone, Debug)]
struct ChangedApartment {
    old: Apartment,
    new: Apartment,
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
    known_apartments: BTreeMap<String, Apartment>,
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
                    to_bullet_list(diff.changed.iter())
                );
            }
        }

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
