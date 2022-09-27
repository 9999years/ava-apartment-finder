#![allow(dead_code)]

use std::collections::BTreeMap;

use color_eyre::eyre;
use color_eyre::eyre::eyre;
use quick_js::Context;
use serde::Deserialize;
use serde_json::Value;
use soup::prelude::*;

const AVA_URL: &str =
    "https://new.avaloncommunities.com/washington/seattle-apartments/ava-capitol-hill/";

const JS_PREFIX: &str = "window = {}; \
                         window.Fusion = {}; \
                         Fusion = window.Fusion; ";
const JS_SUFFIX: &str = "JSON.stringify(Fusion.globalContent)";

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let response = reqwest::get(AVA_URL).await?;
    let body = response.text().await?;

    let soup = Soup::new(&body);

    let script_tag = soup
        .tag("script")
        .attr("id", "fusion-metadata")
        .find()
        .ok_or_else(|| eyre!("Could not find `<script id=\"fusion-metadata\">` tag"))?
        .text();

    let script = format!("{JS_PREFIX}{script_tag}{JS_SUFFIX}");

    let js_context = Context::new().unwrap();

    let value = js_context.eval_as::<String>(&script)?;

    let apartment_data: ApartmentData = serde_json::from_str(&value)?;

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

        println!("Apartment {number} ({bedroom} bed {bathroom} bath, ${price}): {square_feet} sqft, available {available_date}");
    }

    Ok(())
}

#[derive(Clone, Debug, Deserialize)]
struct ApartmentData {
    units: Vec<Apartment>,
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
    available_date: String,
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
    move_in_date: String,
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
    date: String,

    // Shoulda been a usize
    term_length: String,

    #[serde(flatten)]
    price: Price,
}
