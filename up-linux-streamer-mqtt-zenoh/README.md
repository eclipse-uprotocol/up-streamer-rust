# up-linux-streamer

Concrete implementation of a uStreamer as a binary.

## Configuration

Reference the `DEFAULT_CONFIG.json5` configuration file to understand configuration options.

As well, the `ZENOH_CONFIG.json5` file is used to set Zenoh configurations. By default, it is only used to set listening endpoints, but can be used with more configurations according to [Zenoh's page on it](https://zenoh.io/docs/manual/configuration/#configuration-files).

### Bundled vsomeip or bring your own

The default is to build a bundled version of [vsomeip](https://github.com/COVESA/vsomeip) for use by the `up-transport-vsomeip` crate.

The vsomeip library is used to communicate over [SOME/IP](https://some-ip.com/) to mechatronics devices.

If you wish to bring your own vsomeip install, you can use the flag `--no-default-features` flag when building with `cargo build`. For more details on required environment variables when building `up-transport-vsomeip-rust`, reference the README for [vsomeip-sys](https://github.com/eclipse-uprotocol/up-transport-vsomeip-rust/tree/main/vsomeip-sys).

## Running the Streamer in system with an ECU that supports Zenoh and a cloud component that runs MQTT

### Running the `up-linux-streamer-mqtt-zenoh` as the Streamer instance

To run one of the examples below and see client and service communicate, you'll need to run the `up-linux-streamer-mqtt-zenoh` to bridge between the transports in a terminal. This implementation works out of the box with the given examples.

Run the linux streamer with the default configuration file from the project root directory (or fix the path and run from here):

```bash
cargo run -p up-linux-streamer-mqtt-zenoh -- --config up-linux-streamer-mqtt-zenoh/DEFAULT_CONFIG.json5
```

This starts the streamer which should now be idle. As soon as a client tries to connect with the streamer, the connection will be logged.

### Running the MQTT cloud component

Launch the `mqtt_service` example in another terminal. This pretends to be a cloud service which listens for MQTT messages.

```bash
cargo run -p example-streamer-uses --bin mqtt_service
```

### Running the Zenoh ECU component

Launch the `zenoh_client` example in another terminal. This pretends to be an ECU component which sends data through Zenoh

```bash
cargo run -p example-streamer-uses --bin me_client
```

The service and client will run forever. Every second a new request message is sent from the client via zenoh. That zenoh message is caught and routed over MQTT to the service. The response to the request makes the same journey in reverse.