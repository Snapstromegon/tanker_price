#![deny(
    warnings,
    unsafe_code,
    missing_docs,
    clippy::missing_docs_in_private_items
)]

//! Exposes a prometheus exporter for the [Tankerkönig API](https://creativecommons.tankerkoenig.de/)
//! which is also able to resolve locations using the [Nominatim openstreetmap.org API](https://nominatim.openstreetmap.org/ui/search.html).

use std::{net::SocketAddr, time::Duration};

use crate::tankerkoenig::TankerKoenig;
use axum::{
    response::{IntoResponse, Redirect},
    routing::get,
    Router,
};
use clap::Parser;
use prometheus::{register_gauge, register_gauge_vec, Encoder, TextEncoder};
use tokio::time;
mod locator;
mod tankerkoenig;

/// Validate the update timings
fn arg_validate_update_time(time: &str) -> Result<u64, String> {
    match time.parse() {
        Ok(t) if t >= 5 * 60 => Ok(t),
        Ok(t) => Err(format!("Your update cycle {t} is shorter than five minutes. You have to use at least five minutes (300s) to comply with the Tankerkönig API Terms.")),
        Err(_) => Err("Your update time is not a valid (unsigned) integer!".to_string()),
    }
}

/// Make sure that the provided radius conforms to the Tankerkönig API limitations
fn arg_validate_radius(radius: &str) -> Result<f64, String> {
    match radius.parse() {
        Err(_) => Err("The radius is not a valid floating point number!".to_string()),
        Ok(r) if r < 0. => Err(format!("The provided radius {r} is less than 0!")),
        Ok(r) if r <= 25. => Ok(r),
        Ok(r) => Err(format!("The provided radius {r} is larger than 25km, which is not allowed by the Tankerkönig API. Please choose a radius <= 25.")),
    }
}

/// Tankerkönig interface which is also able to resolve Open Street Map locations
/// and exports the data as prometheus metrics
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None, )]
struct Args {
    /// Location to search prices for
    #[clap(short, long, env)]
    location: locator::Location,

    /// Radius around location to search
    #[clap(short, long, env, default_value_t = 2., parse(try_from_str=arg_validate_radius))]
    radius: f64,

    /// API Key for the Tankerkönig API
    #[clap(short = 'k', long, env)]
    tankerkoenig_key: String,

    /// Update Interval in Seconds
    #[clap(short, long, env, default_value_t = 300, parse(try_from_str=arg_validate_update_time))]
    update_interval: u64,

    /// Namespace for all prometheus metrics
    #[clap(short = 'n', long, env, default_value = "tanker_price")]
    prometheus_namespace: String,

    /// Socket address to bind to for the prometheus endpoint
    #[clap(long, env, default_value = "0.0.0.0:3000")]
    listen: SocketAddr,
}

/// Expose the prometheus metrics
async fn metrics() -> impl IntoResponse {
    let encoder = TextEncoder::new();
    let mut buffer = vec![];
    encoder
        .encode(&prometheus::gather(), &mut buffer)
        .expect("Failed to encode metrics");

    let response = String::from_utf8(buffer.clone()).expect("Failed to convert bytes to string");
    buffer.clear();

    response
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let fuel_prices = register_gauge_vec!(
        format!("{}_fuel_price", args.prometheus_namespace),
        "Price of each fuel type",
        &["name", "brand", "id", "fuel_type"]
    )
    .unwrap();
    let is_open = register_gauge_vec!(
        format!("{}_is_open", args.prometheus_namespace),
        "Is gas station currently open?",
        &["name", "brand", "id"]
    )
    .unwrap();
    let distance = register_gauge_vec!(
        format!("{}_distance_km", args.prometheus_namespace),
        "Distance from reference point",
        &["name", "brand", "id"]
    )
    .unwrap();
    let loc_long = register_gauge_vec!(
        format!("{}_location_long", args.prometheus_namespace),
        "Longitude of station",
        &["name", "brand", "id"]
    )
    .unwrap();
    let loc_lat = register_gauge_vec!(
        format!("{}_location_lat", args.prometheus_namespace),
        "Latitude of station",
        &["name", "brand", "id"]
    )
    .unwrap();
    let last_update = register_gauge!(
        format!("{}_update", args.prometheus_namespace),
        "Last update in seconds"
    )
    .unwrap();

    let coordinates = args.location.resolve_to_coordinates().await.unwrap();

    let tk = TankerKoenig {
        api_key: args.tankerkoenig_key,
        radius: args.radius,
        location: coordinates.clone(),
    };
    println!("Searching for location {:?}", coordinates);

    let updater = tokio::spawn(async move {
        let mut interval = time::interval(Duration::from_secs(args.update_interval));
        loop {
            interval.tick().await;
            println!("Fetching prices...");
            let stations = tk.load_prices().await.unwrap();
            last_update.set(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs_f64(),
            );

            for station in &stations {
                is_open
                    .with_label_values(&[&station.name, &station.brand, &station.id])
                    .set(if station.is_open { 1. } else { 0. });
                distance
                    .with_label_values(&[&station.name, &station.brand, &station.id])
                    .set(station.dist);
                loc_lat
                    .with_label_values(&[&station.name, &station.brand, &station.id])
                    .set(station.location.lat);
                loc_long
                    .with_label_values(&[&station.name, &station.brand, &station.id])
                    .set(station.location.long);
                for price in &station.prices {
                    fuel_prices
                        .with_label_values(&[
                            &station.name,
                            &station.brand,
                            &station.id,
                            &price.fuel_type.to_string(),
                        ])
                        .set(price.price);
                }
            }
            println!("Update Done!");
        }
    });

    let app = Router::new()
        .route("/metrics", get(metrics))
        .route("/", get(|| async { Redirect::permanent("/metrics") }));

    let server = axum::Server::bind(&args.listen).serve(app.into_make_service());

    let (server_res, updater_res) = tokio::join!(server, updater);
    server_res.unwrap();
    updater_res.unwrap();
}
