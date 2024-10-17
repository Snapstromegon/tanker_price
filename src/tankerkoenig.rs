//! Abstraction for the Tankerkönig API
//!
//! This abstracts away the [Tankerkönig API](https://creativecommons.tankerkoenig.de/) and allows
//! loading prices from the API.

use recoord::Coordinate;
use serde::Deserialize;
use std::fmt::{self, Display};

/// A Tankerkönig station with all required information including prices
#[derive(Debug)]
pub struct TankerStation {
    /// ID of the station (as provided by the Tankerkönig API)
    pub id: String,
    /// Name of the station
    pub name: String,
    /// Brand of the station
    pub brand: String,
    /// Is the station currently open?
    pub is_open: bool,
    /// Distance from the search center
    pub dist: f64,
    /// The fuel prices of this station
    pub prices: Vec<TankerPrice>,
    /// The location of this station
    pub location: Coordinate,
}

impl Display for TankerStation {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "{}/{} ({})", self.brand, self.name, self.id)?;
        writeln!(f, "\tlocation: {}", self.location)?;
        writeln!(f, "\topen: {}", self.is_open)?;
        writeln!(f, "\tdist: {}", self.dist)?;
        writeln!(f, "\tPrices:")?;
        for price in &self.prices {
            writeln!(f, "\t\t{price}")?;
        }

        Ok(())
    }
}

impl From<TankerAPIStation> for TankerStation {
    fn from(api_resp: TankerAPIStation) -> Self {
        TankerStation {
            id: api_resp.id,
            name: api_resp.name,
            brand: api_resp.brand,
            is_open: api_resp.is_open,
            dist: api_resp.dist,
            prices: [
                (TankerFuelType::Diesel, api_resp.diesel),
                (TankerFuelType::E5, api_resp.e5),
                (TankerFuelType::E10, api_resp.e10),
            ]
            .into_iter()
            .filter_map(|(fuel_type, price)| price.map(|price| TankerPrice { fuel_type, price }))
            .into_iter()
            .collect(),
            location: Coordinate {
                lng: api_resp.lng,
                lat: api_resp.lat,
            },
        }
    }
}

/// Available fuel types
#[derive(Debug)]
pub enum TankerFuelType {
    /// Fuel with 10% ethanol
    E10,
    /// Fuel with 5% ethanol
    E5,
    /// Diesel fuel
    Diesel,
}

impl Display for TankerFuelType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::E10 => write!(f, "E10"),
            Self::E5 => write!(f, "E5"),
            Self::Diesel => write!(f, "Diesel"),
        }
    }
}

impl From<TankerFuelType> for String {
    fn from(fuel_type: TankerFuelType) -> Self {
        match fuel_type {
            TankerFuelType::Diesel => "Diesel".to_string(),
            TankerFuelType::E10 => "E10".to_string(),
            TankerFuelType::E5 => "E5".to_string(),
        }
    }
}

/// A price entry for a single fuel type
#[derive(Debug)]
pub struct TankerPrice {
    /// The fuel type of this
    pub fuel_type: TankerFuelType,
    /// Price for this fuel
    pub price: f64,
}

impl Display for TankerPrice {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}: {:.2}€", self.fuel_type, self.price)
    }
}

/// Possible errors of the Tankerkönig API
#[derive(Debug, thiserror::Error)]
pub enum TankerError {
    #[error("There was a connection error to the API: {0}")]
    ReqwestError(#[from] reqwest::Error),
    #[error("The API threw an error")]
    APIError(Option<String>),
}

/// Struct for deserializing the Tankerkönig API response
#[derive(Deserialize)]
struct TankerAPIResponse {
    /// Is the response successful?
    ok: bool,
    /// Stations of the response
    stations: Option<Vec<TankerAPIStation>>,
    /// Optional error message
    message: Option<String>,
}

/// A struct for deserialization of the Tankerkönig API response
#[derive(Deserialize)]
struct TankerAPIStation {
    /// ID of the station in the Tankerkönig API
    id: String,
    /// name of the station
    name: String,
    /// Brand of the station
    brand: String,
    // street: String,
    // place: String,
    /// latitude of the station
    lat: f64,
    /// longitude of the station
    lng: f64,
    /// distance from search center
    dist: f64,
    /// price for one liter diesel
    diesel: Option<f64>,
    /// price for one liter e5
    e5: Option<f64>,
    /// price for one liter e10
    e10: Option<f64>,
    /// Is the station currently open?
    #[serde(rename(deserialize = "isOpen"))]
    is_open: bool,
    // #[serde(rename(deserialize = "houseNumber"))]
    // house_number: String,
    // #[serde(rename(deserialize = "postCode"))]
    // post_code: u32,
}

/// An abstraction for the TankerKoenig API.
///
/// It binds your API key, location and search radius.
pub struct TankerKoenig {
    /// API Key from the Tankerkönig API. You can get yours on the [Tankerkönig API page](https://creativecommons.tankerkoenig.de/)
    pub api_key: String,
    /// Radius in km around the location
    pub radius: f64,
    /// Location (center point) of your search area
    pub location: Coordinate,
}

impl TankerKoenig {
    /// Load the prices for the current TankerKoenig instance.
    pub async fn load_prices(&self) -> Result<Vec<TankerStation>, TankerError> {
        let client = reqwest::Client::new();
        let request = client
            .get("https://creativecommons.tankerkoenig.de/json/list.php")
            .header(reqwest::header::USER_AGENT, "tanker_price")
            .query(&[("type", "all"), ("apikey", &self.api_key)])
            .query(&[
                ("lat", self.location.lat),
                ("lng", self.location.lng),
                ("rad", self.radius),
            ])
            .build()?;
        let result = client
            .execute(request)
            .await?
            .json::<TankerAPIResponse>()
            .await?;
        match (result.ok, result.stations) {
            (true, Some(stations)) => Ok(stations.into_iter().map(TankerStation::from).collect()),
            _ => Err(TankerError::APIError(result.message)),
        }
    }
}
