//! Allows parsing and resolving locations with the openstreetmap API

use regex::Regex;
use serde::Deserialize;
use std::error::Error;
use std::fmt;
use std::num::ParseFloatError;
use std::str::FromStr;

/// Convert a sexagesimal coordinate to a decimal one.
fn sexagesimal_to_decimal(degree: f64, minutes: Option<f64>, seconds: Option<f64>) -> f64 {
    degree + minutes.unwrap_or(0.) / 60. + seconds.unwrap_or(0.) / 60. / 60.
}

/// Location of Open Street Maps
#[derive(Deserialize)]
struct OSMLocation {
    /// Latitude
    lat: String,
    /// Longitude
    lon: String,
}

/// Possible compass directions
#[derive(PartialEq, Eq)]
enum CompassDirection {
    /// North
    North,
    /// East
    East,
    /// South
    South,
    /// West
    West,
}

impl From<&str> for CompassDirection {
    fn from(dir: &str) -> Self {
        match &dir.to_uppercase()[..] {
            "N" | "NORTH" => CompassDirection::North,
            "E" | "EAST" => CompassDirection::East,
            "S" | "SOUTH" => CompassDirection::South,
            "W" | "WEST" => CompassDirection::West,
            _ => unreachable!(),
        }
    }
}

/// Possible errors when creating a location
#[derive(Debug)]
pub enum LocationError {
    /// The location was malformed and can't be used
    Malformed,
    /// There was an error parsing a float
    ParseFloatError(ParseFloatError),
    /// There was a problem connecting to the API
    ReqwestError(reqwest::Error),
    /// The location can't be resolved by the Openstreetmaps API
    Unresolveable,
}

impl Error for LocationError {}

impl fmt::Display for LocationError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl From<ParseFloatError> for LocationError {
    fn from(err: ParseFloatError) -> Self {
        Self::ParseFloatError(err)
    }
}

impl From<reqwest::Error> for LocationError {
    fn from(err: reqwest::Error) -> Self {
        Self::ReqwestError(err)
    }
}

/// A coordinate location (as returned by the resolve_to_coordinates location)
///
/// You probably don't want to create this, but use the Location struct instead
#[derive(Debug, Clone)]
pub struct CoordinateLocation {
    /// Longitude
    pub long: f64,
    /// Latitude
    pub lat: f64,
}

impl fmt::Display for CoordinateLocation {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{},{}", self.lat, self.long)
    }
}

/// A location which can describe a place either as a coordinate or an abstract location like an address
#[derive(Debug, Clone)]
pub enum Location {
    /// A coordinates location
    Coordinates(CoordinateLocation),
    /// A named coordinates (anything that is not a coordinate location already)
    /// Can be resolved to a coordinate location with resolve_to_coordinates()
    Named(String),
}

impl Location {
    /// Resolve a location to coordinates.
    ///
    /// If the location is already coordinates, it returns a clone,
    /// otherwise it uses the [Nominatim API](https://nominatim.openstreetmap.org/) to resolve the location
    /// to coordinates.
    pub async fn resolve_to_coordinates(&self) -> Result<CoordinateLocation, LocationError> {
        match self {
            Location::Coordinates(coordinates) => Ok(coordinates.clone()),
            Location::Named(name) => {
                let locations = reqwest::Client::new()
                    .get("https://nominatim.openstreetmap.org/search")
                    .header(reqwest::header::USER_AGENT, "tanker_price")
                    .query(&[("format", "json"), ("q", name)])
                    .send()
                    .await?
                    .json::<Vec<OSMLocation>>()
                    .await?;
                if let Some(location) = locations.get(0) {
                    Ok(CoordinateLocation {
                        long: location.lon.parse()?,
                        lat: location.lat.parse()?,
                    })
                } else {
                    Err(LocationError::Unresolveable)
                }
            }
        }
    }
}

impl fmt::Display for Location {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Named(name) => write!(f, "{name}"),
            Self::Coordinates(location) => write!(f, "{},{}", location.lat, location.long),
        }
    }
}

impl FromStr for Location {
    type Err = LocationError;
    fn from_str(raw_loc: &str) -> Result<Self, Self::Err> {
        let loc = raw_loc.to_uppercase();
        let loc = loc.trim();

        let decimal_coords_re =
            Regex::new(r"^(?P<lat>[+-]?\d+(\.\d+)?)\s*[,\./]\s*(?P<long>[+-]?\d+(\.\d+)?)$")
                .unwrap();
        let re_captures = decimal_coords_re.captures(loc);

        if let Some(captures) = re_captures {
            match (captures.name("long"), captures.name("lat")) {
                (None, _) => return Err(LocationError::Malformed),
                (_, None) => return Err(LocationError::Malformed),
                (Some(long), Some(lat)) => {
                    return Ok(Self::Coordinates(CoordinateLocation {
                        long: long.as_str().parse()?,
                        lat: lat.as_str().parse()?,
                    }))
                }
            }
        }
        let long_lat_re = Regex::new("^(?P<lat_deg>\\d+(\\.\\d+)?)°((?P<lat_min>\\d+(\\.\\d+)?)')?((?P<lat_sec>\\d+(\\.\\d+)?)\"?)(?P<n_s>[NS])\\s*(?P<long_deg>\\d+(\\.\\d+)?)°((?P<long_min>\\d+(\\.\\d+)?)')?((?P<long_sec>\\d+(\\.\\d+)?)\")?(?P<e_w>[EW])$").unwrap();
        let re_captures = long_lat_re.captures(loc);
        if let Some(captures) = re_captures {
            match (
                captures.name("lat_deg"),
                captures.name("lat_min"),
                captures.name("lat_sec"),
                captures.name("long_deg"),
                captures.name("long_min"),
                captures.name("long_sec"),
            ) {
                (None, _, _, _, _, _) => return Err(LocationError::Malformed),
                (_, _, _, None, _, _) => return Err(LocationError::Malformed),
                (Some(lat_deg), lat_min, lat_sec, Some(long_deg), long_min, long_sec) => {
                    return Ok(Self::Coordinates(CoordinateLocation {
                        lat: if CompassDirection::from(captures.name("n_s").unwrap().as_str())
                            == CompassDirection::North
                        {
                            1.
                        } else {
                            -1.
                        } * sexagesimal_to_decimal(
                            lat_deg.as_str().parse()?,
                            match lat_min {
                                None => None,
                                Some(lat_min) => Some(lat_min.as_str().parse()?),
                            },
                            match lat_sec {
                                None => None,
                                Some(lat_min) => Some(lat_min.as_str().parse()?),
                            },
                        ),
                        long: if CompassDirection::from(captures.name("e_w").unwrap().as_str())
                            == CompassDirection::East
                        {
                            1.
                        } else {
                            -1.
                        } * sexagesimal_to_decimal(
                            long_deg.as_str().parse()?,
                            match long_min {
                                None => None,
                                Some(long_min) => Some(long_min.as_str().parse()?),
                            },
                            match long_sec {
                                None => None,
                                Some(long_min) => Some(long_min.as_str().parse()?),
                            },
                        ),
                    }))
                }
            }
        }

        Ok(Self::Named(raw_loc.to_owned()))
    }
}
