use std::collections::BTreeMap;
use std::fmt::Display;

use chrono::DateTime;
use chrono::Utc;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;

use super::ava_date;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApartmentData {
    pub units: Vec<Apartment>,
    promotions: Vec<Promotion>,
    pricing_overview: Vec<PricingOverview>,
    #[serde(flatten)]
    extra: Value,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Apartment {
    pub unit_id: String,
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
