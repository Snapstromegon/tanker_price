# Tankerkoenig-Prometheus

This is a prometheus exporter for the german fuel price API [Tankerkönig](https://creativecommons.tankerkoenig.de/).

## Usage

To use this tool, just start the tool via `tanker_price` or run the docker container via `docker run -p 9501:9501 snapstromegon/tankerkoenig-prometheus [options]`.

You can configure the tool via these options:

| Option                 | Shortcut | Environment Variable | Type   | Description                                                 | Default         |
| :--------------------- | :------- | :------------------- | :----- | :---------------------------------------------------------- | :-------------- |
| --update-interval      | -u       | UPDATE_INTERVAL      | u64    | Update interval in seconds                                  | 300 (5 minutes) |
| --tankerkoenig-key     | -k       | TANKERKOENIG_KEY     | String | Tankerkönig API Key                                         | -               |
| --location             | -l       | LOCATION             | String | Text description of the current location                    | -               |
| --radius               | -r       | RADIUS               | f64    | Radius around location in km to include in search (max. 25) | 2               |
| --prometheus_namespace | -n       | PROMETHEUS_NAMESPACE | String | Namespace/prefix for prometheus metrics                     | tanker_price    |
| --listen               | -i       | LISTEN               | String | Interface to listen on                                      | 0.0.0.0:9501    |

## Get an API Key

To get an API key, follow the guide on the [Tankerkönig page](https://creativecommons.tankerkoenig.de/).

## Location resolver

To resolve the location of the `--location` option, the [Open Street Maps Nominatim service](https://nominatim.openstreetmap.org/) is used.
Alternatively you can pass in the string version of a [recoord](https://crates.io/crates/recoord) compatible location.

## Contributions

Feel free to contribute in any shape or form to this project!
