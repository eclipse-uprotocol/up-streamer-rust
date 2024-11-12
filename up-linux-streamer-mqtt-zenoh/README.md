# up-linux-streamer

This is a reference implementation of a uStreamer. We implement the Streamer so that it forwards between two components, one running with Zenoh (for example a component running in an ECU) and one running with MQTT (which might be a component running in some sort of cloud).
The Streamer by itself does not do much, but its designed to be run together with two of the transport examples in the example-streamer-uses folder.

## Supported Setups

Here are the setups that can be built with this streamer and different entities with the Zenoh and MQTT transports.

### Client-Service Setups

In a setup with one client and one service, the service runs in the background while the client periodically makes requests to it.
Once the server receives a request it will respond with a reply.
The request message contains information on a "sink" (the URI of the service entity which it tries to reach) and a source (the URI of the client so that the service knows where to send the response to).

For a single setup you can choose either:
- a Zenoh Client and an MQTT Service (Could be some component of a cars' software requesting information from the cloud)
- or an MQTT Client and a Zenoh Service (Might be a backend service trying to pull telemetry data from a running car)

### Publish-Subscribe Setups

This setup is more straight forward and consists of one publisher broadcasting messages to a topic and a subscriber who listens to the topic.
Messages that the publisher sends contain a "topic" that the message will be available on. There is no response expected so publish messages do not contain the publishers URI.
The subscriber listens to one or multiple topics via a filter.

For this setup you can also choose:
- a Zenoh Publisher and an MQTT Subscriber (Maybe a backend service getting live data from a car)
- an MQTT Publisher and a Zenoh Subscriber (Perhaps a car getting over the air traffic information)

## Understanding the Configuration Files

Reference the `DEFAULT_CONFIG.json5` configuration file to understand the basic configuration options for the Streamer.

The `ZENOH_CONFIG.json5` file is used to set Zenoh configurations. By default, it is only used to set listening endpoints, but can be used with more configurations according to [Zenoh's page on it](https://zenoh.io/docs/manual/configuration/#configuration-files).

The static_subscriptions.json is only needed when you set up a publish-subscribe system and can be ignored for a client-service system.

The MQTT Transport has its configuration hardcoded for now. For the settings check main.rs for "mqtt_config".

## Running the Streamer in system with an ECU that supports Zenoh and a cloud component that runs MQTT

### Running the `up-linux-streamer-mqtt-zenoh` as the Streamer instance

To run one of the examples below and see client and service communicate, you'll need to run the `up-linux-streamer-mqtt-zenoh` to bridge between the transports in a terminal. This implementation works out of the box with the given examples.

First run an MQTT broker. You can use your own or use the Mosquitto instance provided in this repo. For that execute the following:

```bash
cd mosquitto
docker compose up
```

Run the linux streamer with the default configuration file from the project root directory (or fix the path and run from here):

```bash
cargo run -p up-linux-streamer-mqtt-zenoh -- --config up-linux-streamer-mqtt-zenoh/DEFAULT_CONFIG.json5
```

This starts the streamer which should now be idle. As soon as a client tries to connect with the streamer, the connection will be logged.

### Running the Entities

Execute the following command from the project root directory to start one of the Entities:

```bash
cargo run -p example-streamer-uses --bin <transport_entity>
```

Depending on the setup you want to test, chose any of these examples:

| Entity 1        | Entity 2         |
| --------------- | -------------    |
| mqtt_client     | zenoh_service    |
| mqtt_service    | zenoh_client     |
| mqtt_publisher  | zenoh_subscriber |
| mqtt_subscriber | zenoh_publisher  |

The service and client will run forever. Every second a new request message is sent from the client via zenoh. That zenoh message is caught and routed over MQTT to the service. The response to the request makes the same journey in reverse.
