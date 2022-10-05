use std::collections::BTreeMap;
use std::fmt::Display;

use chrono::DateTime;
use chrono::Utc;
use color_eyre::eyre;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(try_from = "ApiApartmentData")]
pub struct ApartmentData {
    pub apartments: Vec<Apartment>,
}

impl TryFrom<ApiApartmentData> for ApartmentData {
    type Error = eyre::Report;

    fn try_from(data: ApiApartmentData) -> Result<Self, Self::Error> {
        let mut apartments = Vec::with_capacity(data.units.len());

        for apt in data.units {
            apartments.push(Apartment {
                inner: apt.clone(),
                history: vec![ApartmentSnapshot {
                    inner: serde_json::to_value(&apt)?,
                    observed: Utc::now(),
                }],
                listed: Utc::now(),
                unlisted: None,
            })
        }

        Ok(Self { apartments })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ApiApartmentData {
    units: Vec<ApiApartment>,
    promotions: Vec<Promotion>,
    pricing_overview: Vec<PricingOverview>,
    #[serde(flatten)]
    extra: Value,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Apartment {
    pub inner: ApiApartment,
    pub history: Vec<ApartmentSnapshot>,
    pub listed: DateTime<Utc>,
    pub unlisted: Option<DateTime<Utc>>,
}

impl Apartment {
    pub fn id(&self) -> &str {
        &self.inner.unit_id
    }

    pub fn update_inner(&mut self, new_inner: ApiApartment) -> eyre::Result<()> {
        self.inner = new_inner;
        self.history.push(ApartmentSnapshot {
            inner: serde_json::to_value(&self.inner)?,
            observed: Utc::now(),
        });
        Ok(())
    }
}

impl Display for Apartment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(unlisted) = self.unlisted {
            let tracked_duration = unlisted - self.listed;
            write!(
                f,
                "Unlisted after {} days: {}",
                tracked_duration.num_days(),
                self.inner
            )
        } else {
            write!(f, "{}", self.inner)
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ApartmentSnapshot {
    pub inner: Value,
    pub observed: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ApiApartment {
    pub unit_id: String,
    #[serde(rename = "name")]
    pub number: String,
    #[serde(rename = "furnishStatus")]
    furnished: Furnished,
    floor_plan: FloorPlan,
    virtual_tour: Option<VirtualTour>,
    bedroom: usize,
    bathroom: usize,
    square_feet: f64,
    pub available_date: AvaDate,
    #[serde(rename = "unitRentPrice")]
    rent: Rent,
    #[serde(rename = "lowestPricePerMoveInDate")]
    lowest_rent: LowestRent,
    promotions: Vec<ApplicablePromotion>,

    #[serde(flatten)]
    extra: Value,
}

impl ApiApartment {
    pub fn meets_qualifications(&self) -> bool {
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

impl Display for ApiApartment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ApiApartment {
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
        let available_date = &available_date.date().format("%c");
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
    move_in_date: AvaDate,
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
    date: AvaDate,

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
    start_date: AvaDate,
    end_date: Option<AvaDate>,
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

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(transparent)]
pub struct AvaDate(#[serde(with = "crate::ava_date")] DateTime<Utc>);

impl std::ops::Deref for AvaDate {
    type Target = DateTime<Utc>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
