#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::Path;

use chrono::DateTime;
use chrono::Utc;
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

const DATA_PATH: &str = "ava_db.json";

const AVA_URL: &str =
    "https://new.avaloncommunities.com/washington/seattle-apartments/ava-capitol-hill/";

const JS_PREFIX: &str = "window = {}; \
                         window.Fusion = {}; \
                         Fusion = window.Fusion; ";
const JS_SUFFIX: &str = "JSON.stringify(Fusion.globalContent)";

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let data_path = Path::new(&DATA_PATH);
    if data_path.exists() {
        tracing::debug!(path = ?data_path, "DB path exists, reading");
        let app: App = serde_json::from_str(
            &std::fs::read_to_string(&data_path)
                .wrap_err_with(|| format!("Failed to read `{data_path:?}`"))?,
        )
        .wrap_err_with(|| format!("Failed to load Apartment data from `{data_path:?}`"))?;
    } else {
    }

    let apartment_data = get_apartments().await?;

    for Apartment {
        unit_id,
        number,
        furnished,
        floor_plan,
        virtual_tour,
        bedroom,
        bathroom,
        square_feet,
        available_date,
        rent,
        lowest_rent,
        extra,
    } in apartment_data.units
    {
        if let Furnished::Furnished = furnished {
            tracing::debug!(number = number, "Skipping apartment; furnished");
            continue;
        }

        if bedroom != 2 {
            tracing::debug!(
                number = number,
                bedrooms = bedroom,
                bathrooms = bathroom,
                "Skipping apartment; too few bedrooms"
            );
            continue;
        }

        let price = lowest_rent.price.price;

        let available_date = available_date.date();

        println!("Apartment {number} ({bedroom} bed {bathroom} bath, ${price}): {square_feet} sqft, available {available_date}");
    }

    Ok(())
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
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct App {
    known_apartments: BTreeMap<String, Apartment>,
}

impl App {
    /// One 'tick' of the app. Get new apartment data and report changes.
    async fn tick(&mut self) -> eyre::Result<()> {
        Ok(())
    }

    /// Fetch new apartment data, update `known_apartments` to include it, and return the
    /// changes with the previous `known_apartments`.
    async fn compute_diff(&mut self) -> eyre::Result<()> {
        let new_data = get_apartments().await?;
        let mut diff = ApartmentsDiff::default();

        for unit in new_data.units {
            if let Some(known_unit) = self.known_apartments.get(&unit.unit_id) {
                if &unit != known_unit {
                    // TODO: print diff here!!
                    tracing::info!(
                        old_data = serde_json::to_string_pretty(known_unit)
                            .map_err(|err| tracing::error!("{err}"))
                            .unwrap(),
                        new_data = serde_json::to_string_pretty(&unit)
                            .map_err(|err| tracing::error!("{err}"))
                            .unwrap(),
                        "Data changed for apartment {}",
                        unit.number
                    );
                }
                // diff.added.push(unit);
            }
        }

        Ok(())
    }
}

impl From<ApartmentData> for App {
    fn from(data: ApartmentData) -> Self {
        let mut known_apartments = BTreeMap::new();

        for unit in data.units {
            known_apartments.insert(unit.unit_id.clone(), unit);
        }

        Self { known_apartments }
    }
}
