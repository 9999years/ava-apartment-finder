#![allow(dead_code)]

use std::collections::BTreeMap;

use chrono::DateTime;
use chrono::Utc;
use color_eyre::eyre;
use color_eyre::eyre::eyre;
use quick_js::Context;
use serde::Deserialize;
use serde_json::Value;
use soup::prelude::*;

mod ava_date;

const AVA_URL: &str =
    "https://new.avaloncommunities.com/washington/seattle-apartments/ava-capitol-hill/";

const JS_PREFIX: &str = "window = {}; \
                         window.Fusion = {}; \
                         Fusion = window.Fusion; ";
const JS_SUFFIX: &str = "JSON.stringify(Fusion.globalContent)";

#[tokio::main]
async fn main() -> eyre::Result<()> {
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

    let js_context = Context::new().unwrap();

    let value = js_context.eval_as::<String>(&script)?;

    tracing::trace!(value, "Evaluated JavaScript");

    Ok(serde_json::from_str(&value)?)
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApartmentData {
    units: Vec<Apartment>,
    promotions: Vec<Promotion>,
    pricing_overview: Vec<PricingOverview>,
    #[serde(flatten)]
    extra: Value,
}

#[derive(Clone, Debug, Deserialize)]
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

#[derive(Clone, Debug, Deserialize)]
enum Furnished {
    Unfurnished,
    OnDemand,
    #[serde(rename = "Designated")]
    Furnished,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FloorPlan {
    name: String,
    low_resolution: String,
    high_resolution: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VirtualTour {
    space: String,
    is_actual_unit: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Rent {
    applied_discount: f64,
    prices_per_movein_date: Vec<PricesForMoveInDate>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PricesForMoveInDate {
    #[serde(with = "ava_date")]
    move_in_date: DateTime<Utc>,
    prices_per_terms: BTreeMap<usize, Price>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Price {
    price: f64,
    net_effective_price: f64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LowestRent {
    #[serde(with = "ava_date")]
    date: DateTime<Utc>,

    // Shoulda been a usize
    term_length: String,

    #[serde(flatten)]
    price: Price,
}

#[derive(Clone, Debug, Deserialize)]
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

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApplicablePromotion {
    promotion_id: String,
    #[serde(with = "ava_date")]
    start_date: DateTime<Utc>,
    #[serde(with = "ava_date")]
    end_date: DateTime<Utc>,
    terms: Vec<usize>,
}

#[derive(Clone, Debug, Deserialize)]
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

struct App {
    known_apartments: BTreeMap<String, Apartment>,
}
