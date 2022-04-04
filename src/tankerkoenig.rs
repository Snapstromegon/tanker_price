use serde::Deserialize;

use crate::locator::{self, CoordinateLocation};
use std::error::Error;
use std::fmt::{self, Display};

#[derive(Debug)]
pub struct TankerStation {
    pub id: String,
    pub name: String,
    pub brand: String,
    pub is_open: bool,
    pub dist: f64,
    pub prices: Vec<TankerPrice>,
    pub location: CoordinateLocation
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
            prices: vec![
                TankerPrice {
                    fuel_type: TankerFuelType::Diesel,
                    price: api_resp.diesel,
                },
                TankerPrice {
                    fuel_type: TankerFuelType::E5,
                    price: api_resp.e5,
                },
                TankerPrice {
                    fuel_type: TankerFuelType::E10,
                    price: api_resp.e10,
                },
            ],
            location: CoordinateLocation { long: api_resp.lng, lat: api_resp.lat }
        }
    }
}

#[derive(Debug)]
pub enum TankerFuelType {
    E10,
    E5,
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

#[derive(Debug)]
pub struct TankerPrice {
    pub fuel_type: TankerFuelType,
    pub price: f64,
}

impl Display for TankerPrice {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}: {:.2}€", self.fuel_type, self.price)
    }
}

#[derive(Debug)]
pub enum TankerError {
    ReqwestError(reqwest::Error),
    APIError(Option<String>),
}

impl Error for TankerError {}

impl fmt::Display for TankerError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl From<reqwest::Error> for TankerError {
    fn from(err: reqwest::Error) -> Self {
        Self::ReqwestError(err)
    }
}

#[derive(Deserialize)]
struct TankerAPIResponse {
    ok: bool,
    stations: Option<Vec<TankerAPIStation>>,
    message: Option<String>,
}

#[derive(Deserialize)]
struct TankerAPIStation {
    id: String,
    name: String,
    brand: String,
    // street: String,
    // place: String,
    lat: f64,
    lng: f64,
    dist: f64,
    diesel: f64,
    e5: f64,
    e10: f64,
    #[serde(rename(deserialize = "isOpen"))]
    is_open: bool,
    // #[serde(rename(deserialize = "houseNumber"))]
    // house_number: String,
    // #[serde(rename(deserialize = "postCode"))]
    // post_code: u32,
}

pub struct TankerKoenig {
    pub api_key: String,
    pub radius: f64,
    pub location: locator::CoordinateLocation,
}

impl TankerKoenig {
    pub async fn load_prices(&self) -> Result<Vec<TankerStation>, TankerError> {
        let result = reqwest::Client::new()
            .get("https://creativecommons.tankerkoenig.de/json/list.php")
            .header(reqwest::header::USER_AGENT, "tanker_price")
            .query(&[("type", "all"), ("apikey", &self.api_key)])
            .query(&[
                ("lat", self.location.lat),
                ("lng", self.location.long),
                ("rad", self.radius),
            ])
            .send()
            .await?
            .json::<TankerAPIResponse>()
            .await?;
        match (result.ok, result.stations) {
            (true, Some(stations)) => Ok(stations
                .into_iter()
                .map(TankerStation::from)
                .collect()),
            _ => Err(TankerError::APIError(result.message)),
        }
    }
}
