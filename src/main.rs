#![deny(
    warnings,
    unsafe_code,
    missing_docs,
    clippy::missing_docs_in_private_items
)]

//! Exposes a prometheus exporter for the [Tankerkönig API](https://creativecommons.tankerkoenig.de/)
//! which is also able to resolve locations using the [Nominatim openstreetmap.org API](https://nominatim.openstreetmap.org/ui/search.html).

use log::{error, info};
use std::{net::SocketAddr, str::FromStr, time::Duration};
use tokio::time;

use crate::tankerkoenig::TankerKoenig;
use axum::{
    response::{IntoResponse, Redirect},
    routing::get,
    Router,
};
use clap::Parser;
use prometheus::{register_gauge, register_gauge_vec, Encoder, TextEncoder};
use recoord::Coordinate;
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
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Location to search prices for
    #[clap(short, long, env)]
    location: String,

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
    #[clap(long, env, default_value = "0.0.0.0:9501")]
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
    env_logger::init();
    let args = Args::parse();

    let coordinates = if let Ok(coordinates) = Coordinate::from_str(&args.location) {
        coordinates
    } else {
        recoord::resolvers::nominatim::resolve(&args.location)
            .await
            .expect("Unable to resolve Location!")
    };

    info!("Searching at location {:?}", coordinates);
    let tk = TankerKoenig {
        api_key: args.tankerkoenig_key,
        radius: args.radius,
        location: coordinates,
    };

    let (updater_shutdown_tx, updater_shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let (server_shutdown_tx, server_shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    let updater = tokio::spawn(async move {
        tokio::select! {
            _ = updater_loop(tk, args.prometheus_namespace, Duration::from_secs(args.update_interval)) => {},
            _ = updater_shutdown_rx => {info!("Shutting Down Updater")}
        }
    });

    let app = Router::new()
        .route("/metrics", get(metrics))
        .route("/", get(|| async { Redirect::permanent("/metrics") }));

    info!("Starting Server...");
    let server = axum::Server::bind(&args.listen)
        .serve(app.into_make_service())
        .with_graceful_shutdown(async {
            server_shutdown_rx.await.ok();
        });

    info!("System Ready to receive requests");

    let (server_res, updater_res, _) = tokio::join!(server, updater, async move {
        info!("Registering CTRL+C handler");
        match tokio::signal::ctrl_c().await {
            Ok(()) => {
                info!("Shutting Down");
                server_shutdown_tx
                    .send(())
                    .expect("Unable to shutdown server");
                updater_shutdown_tx
                    .send(())
                    .expect("Unable to shutdown updater");
            }
            Err(err) => {
                eprintln!("Unable to listen for shutdown signal: {}", err);
                // we also shut down in case of error
            }
        }
        info!("Done")
    });

    info!("Shutdown Complete");

    server_res.unwrap();
    updater_res.unwrap();
    info!("Goodbye");
}

/// Run this as a loop to regularly update the prometheus metrics
async fn updater_loop(tk: TankerKoenig, prometheus_namespace: String, update_interval: Duration) {
    let fuel_prices = register_gauge_vec!(
        format!("{}_fuel_price", prometheus_namespace),
        "Price of each fuel type",
        &["name", "brand", "id", "fuel_type"]
    )
    .unwrap();
    let is_open = register_gauge_vec!(
        format!("{}_is_open", prometheus_namespace),
        "Is gas station currently open?",
        &["name", "brand", "id"]
    )
    .unwrap();
    let distance = register_gauge_vec!(
        format!("{}_distance_km", prometheus_namespace),
        "Distance from reference point",
        &["name", "brand", "id"]
    )
    .unwrap();
    let loc_long = register_gauge_vec!(
        format!("{}_location_long", prometheus_namespace),
        "Longitude of station",
        &["name", "brand", "id"]
    )
    .unwrap();
    let loc_lat = register_gauge_vec!(
        format!("{}_location_lat", prometheus_namespace),
        "Latitude of station",
        &["name", "brand", "id"]
    )
    .unwrap();
    let last_update = register_gauge!(
        format!("{}_update", prometheus_namespace),
        "Last update in seconds"
    )
    .unwrap();

    let mut interval = time::interval(update_interval);
    loop {
        interval.tick().await;
        info!("Fetching prices...");
        let load_result = tk.load_prices().await;
        if let Ok(stations) = load_result {
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
                    .set(station.location.lng);
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
            info!("Update Done!");
        } else {
            error!("Update failed: {:?}", load_result);
        }
    }
}
