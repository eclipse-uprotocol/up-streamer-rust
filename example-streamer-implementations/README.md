# up-linux-streamer

These are reference implementations of a uStreamer.
We implement the Streamers so that they forward between two components, either between Zenoh and MQTT or between Zenoh and SOME/IP.
A component running with Zenoh could be a component running in an ECU, one running with MQTT could be a component running in some sort of cloud and one running SOME/IP could be a mechatronics component.
The Streamer by itself does not do much, but its designed to be run together with two of the transport examples in the example-streamer-uses folder.

## Supported Setups

Here are the setups that can be built with these streamers and different entities with the Zenoh, SOME/IP and MQTT transports.
Depending on which setup you want to try, run the streamer with:

```bash
cargo run --bin <zenoh_mqtt/zenoh_someip> --features=<check cargo.toml or logs to see which feature flags you need> -- --config="DEFAULT_CONFIG.json5"
```

### Client-Service Setups

In a setup with one client and one service, the service runs in the background while the client periodically makes requests to it.
Once the server receives a request it will respond with a reply.
The request message contains information on a "sink" (the URI of the service entity which it tries to reach) and a source (the URI of the client so that the service knows where to send the response to).

For a single setup you can choose either:
- a Zenoh Client and an MQTT Service (A cars' software requesting information from the cloud)
- an MQTT Client and a Zenoh Service (A backend service trying to pull telemetry data from a running car)
- a Zenoh Client and a SOME/IP Service (The infotainment system requesting some mechatronics sensor data)
- or a SOME/IP Client and a Zenoh Service (A mechatronics component asking the infotainment system for input)

### Publish-Subscribe Setups

This setup is more straight forward and consists of one publisher broadcasting messages to a topic and a subscriber who listens to the topic.
Messages that the publisher sends contain a "topic" that the message will be available on. There is no response expected so publish messages do not contain the publishers URI.
The subscriber listens to one or multiple topics via a filter.

For this setup you can also choose:
- a Zenoh Publisher and an MQTT Subscriber (A backend service getting live data from a car)
- an MQTT Publisher and a Zenoh Subscriber (A car getting over the air traffic information)
- a Zenoh Publisher and a SOME/IP Subscriber (An autoedge app pushing some configurations to a mechatronics component)
- or a SOME/IP Publisher and a Zenoh Subscriber (An infotainment app getting live data from a sensor)

### Notification setups

There are currently no example entities for notification type messages. These do not exist for SOME/IP but do exist for Zenoh and MQTT. It should be relatively straight forward to implement the yourself if your system needs them!

## Understanding the Configuration Files

Reference the `DEFAULT_CONFIG.json5` configuration file to understand the basic configuration options for the Streamer.

The `ZENOH_CONFIG.json5` file is used to set Zenoh configurations. By default, it is only used to set listening endpoints, but can be used with more configurations according to [Zenoh's page on it](https://zenoh.io/docs/manual/configuration/#configuration-files).

The 'static_subscriptions.json' is only needed when you set up a publish-subscribe system and can be ignored for a client-service system.
Make sure that the UURI of each pub-sub entity is present at least as a key in this json file!

The 'vsomeip-config/point_to_point.json' is a configuration file only needed for SOME/IP implementations. The list of "services" must include the UEntity IDs of all entities running on the host-protocol (in the reference implementations that means all components running with the Zenoh transport)! The term service in this context comes from SOME/IP and should not be confused with UService entity.

The MQTT Transport has its configuration hardcoded for now. For the settings seach the binaries for "mqtt_config" to set the MQTT brokers address and port as well as ssl-options and others.

## Running the Streamer in a system consisting of an ECU that supports Zenoh and a cloud component that runs MQTT

### Running the `zenoh_mqtt` binary as the Streamer instance

To run one of the basic examples and see two entities with different transports communicate, you'll need to first run the `zenoh_mqtt` bin to bridge between the two transports in a terminal. This implementation should work out of the box with the given examples.

First run an MQTT broker. You can use your own or use the Mosquitto instance provided in this repo. For that execute the following:

```bash
cd ../utils/mosquitto
docker compose up
```

Run the linux streamer with the default configuration file from here (the example-streamer-implementation folder):

```bash
cargo run -bin zenoh_mqtt --features="zenoh-transport mqtt-transport" -- --config="DEFAULT_CONFIG.json5"
```

This starts the streamer which should now be idle. As soon as a client tries to connect with the streamer, the connection will be logged.
The streamer is set to have Zenoh as its "host protocol" or "host transport". This means that the streamer lives in the same component as the Zenoh transport, and shares its authority.
In this setup "authority_B" is the authority of the Zenoh component (in this example the ECU), "authority_A" is the authority of the MQTT component (i.e. the cloud).

### Running the Entities

Execute the following command from the project root directory to start two of the example UEntities:

```bash
cargo run --bin <transport_entity> --features=<check cargo.toml or logs to see which feature flags you need>
```

Depending on the setup you want to test, chose any of these combinations for your two UEntities:

| Entity 1        | Entity 2         |
| --------------- | -------------    |
| mqtt_client     | zenoh_service    |
| mqtt_service    | zenoh_client     |
| mqtt_publisher  | zenoh_subscriber |
| mqtt_subscriber | zenoh_publisher  |

The service and client will run forever. Every second a new request message is sent from the client via zenoh. That Zenoh message is caught and routed over MQTT to the service. The response to the request makes the same journey in reverse.

## Running the Streamer in a system consisting of an ECU that supports Zenoh and a mechatronics component using SOME/IP

### Running the `zenoh_someip` binary as the Streamer instance

The other available reference implementation streams between the Zenoh and SOME/IP transports.

For this implementation you need to add the path to your vsomeip library to the environment, for example by setting it in your .cargo/config.toml:

```toml
[env]
VSOMEIP_INSTALL_PATH=<path to your vsomeip lib (for example /usr/local/lib)>
```

Run the "zenoh_someip" binary of the streamer with the default configuration file:

```bash
cargo run -bin zenoh_someip --features="zenoh-transport vsomeip-transport bundled-someip" -- --config="DEFAULT_CONFIG.json5"
```

For the feature flags choose either "vsomeip-transport" or "bundled-vsomeip" depending on your vsomeip configuration.
Make sure that you have set the required environment variables for the someip transport!

This starts the streamer which should now be idle. As soon as a client tries to connect with the streamer, the connection will be logged.
The streamer is set to have Zenoh as its "host protocol" or "host transport". This means that the streamer lives in the same component as the Zenoh transport, and shares its authority.
In this setup "authority_B" is the authority of the Zenoh component (in this example the ECU), "authority_A" is the authority of the SOME/IP component (i.e. the mechatronics component).

### Running the Entities

Execute the following command from the project root directory to start one of the Entities:

```bash
cargo run -p example-streamer-uses --bin <transport_entity>
```

Depending on the setup you want to test, chose any of these examples:

| Entity 1        | Entity 2         |
| --------------- | -------------    |
| someip_client     | zenoh_service    |
| someip_service    | zenoh_client     |
| someip_publisher  | zenoh_subscriber |
| someip_subscriber | zenoh_publisher  |

The two entities will run forever and exchange messages between each other.

## Going forward from here

If you have familiarized yourself with the streamer to this point you should be able to continue by yourself.
If the two reference implementations are not enough for your system you can consider the following next steps:

- Create a streamer between SOME/IP and MQTT (mind that SOME/IP cannot act as the host-transport)
- Run a streamer that can forward messages between all three transports
- Implement your own custom or proprietary UTransport and connect it to one of the three officially supported ones
- Try out a system with your own UEntities. Between MQTT and Zenoh its also possible to send notification type messages
