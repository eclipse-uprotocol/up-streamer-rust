
# Running entities

## Running a UStreamer

To run one of the examples below and see two entities communicate, you'll need to run one of the two example implementations of the UStreamer to bridge between the transports in a terminal. Check out the `example-streamer-implementation` README for details.

## Running two UEntities

To launch any of the entities choose the correct bin and give it the appropriate feature flags:

| --bin      | --features        |
| ---------- | -------------     |
| mqtt_*     | mqtt-transport    |
| zenoh_*    | zenoh-transport   |
| someip*    | vsomeip-transport |

Combined:

```bash
cargo run --bin <transport-entity> --features <*-transport>
```

To check which combinations of entities are possible for a test run check the `example-streamer-implementation` README.
Before running any entity that uses MQTT remember to first start up a MQTT broker, for example the mosquitto broker found in this repo:

```bash
cd ../utils/mosquitto
docker compose up
```

# Working setup: a mechatronics client and a high compute service

This example should help with understanding what the streamer does and how it can be used. The first part of the system that we are building here is a mechatronics component, which could be some embedded chip deep down in the belly of a car, connected to some mechanical parts like a windshield wiper. This component can (or wants to) only use SOME/IP as a transport protocol either because thats what its developers like to use or because of some hardware limitation. The second component is a high compute unit running some small Linux system that serves as the vehicles boardcomputer. The software thats running here uses Zenoh as its preferred transport protocol because a solution architect once decided that it should do that. The UStreamer comes into play as the system that allows these two components to communicate with each other while staying true to their respective protocols.

Because the streamer cant run on the mechatronics chip, it runs on the high compute unit. In our setup we want the mechatronics entity to request some information from the high compute unit, so the entity type on the SOME/IP side should be a client. If the chip indeed controls the windshield wipers it might want to periodically ask the board computer to provide information about if and how much its raining. The high compute unit should respond to the chip with the latest data that it has available, so it must be an entity of the type service.

## Run the Zenoh-SOME/IP Streamer

To launch the streamer, go to the example-streamer-implementations folder and run the binary with the correct feature flags and the provided config file in a new terminal:

```bash
cd ../example-streamer-implementations
cargo run --bin zenoh_someip --features="zenoh-transport vsomeip-transport" -- --config='DEFAULT_CONFIG.json5'
```

This will start the streamer. By itself it should not do much, except for logging the vsomeip version every now and then.

## Run the mechatronics client

To launch the mechatronics entity, run the binary `someip_client` example in a new terminal:

```bash
cargo run --bin someip_client --features someip_transport
```

You should see in the log that the client is trying to send messages of type UMESSAGE_TYPE_REQUEST. These messages should now also show up in the logs of your streamer terminal just after the SOME/IP client sends them. Since there is no other component they are not yet being forwarded anywhere though.

## Run the high compute unit

Lastly to launch the high compute unit that has access to whatever data the mechatronics part is requesting, launch a zenoh service in yet another terminal:

```bash
cargo run --bin zenoh_service --features zenoh_transport
```

Immediately after launching you should see that this entity is logging incoming requests of type UMESSAGE_TYPE_REQUEST and right after is trying to answer them with UMESSAGE_TYPE_RESPONSE messages. These should then show up again in the log of the UStreamer, and also in the log of the mechatronics SOME/IP client.

The service and client will run forever. Every second a new request is made from the someip_client via vsomeip. That vsomeip message is caught and routed over Zenoh to the zenoh_service. The response makes the same journey in reverse.

To better track the messages you can also check that the payload is listed with `@i=n` where n is incremented on each send so that we can trace the message back and forth over the streamer.


Further, you will see printed the deserialized `HelloResponse` ProtoBuf object:

> Here we received response: HelloResponse { message: "The response to the request: someip_client@i=n", special_fields: SpecialFields { unknown_fields: UnknownFields { fields: None }, cached_size: CachedSize { size: 0 } } }

## CLI parameter overrides

All 12 example binaries now support a common local-entity override surface:

- `--uauthority <string>`
- `--uentity <u32 decimal|hex>`
- `--uversion <u8 decimal|hex>`
- `--resource <u16 decimal|hex>`

Additional role-specific flags:

- Client binaries: `--target-authority --target-uentity --target-uversion --target-resource`
- Subscriber binaries: `--source-authority --source-uentity --source-uversion --source-resource`

Transport-specific flags:

- MQTT binaries: `--broker-uri` (default `localhost:1883`)
- Zenoh binaries: `--endpoint` (existing behavior, now composed with URI overrides)
- SOME/IP binaries: `--vsomeip-config` and `--remote-authority`

Running with no extra flags keeps prior behavior (defaults are aligned with previous constants).

### Numeric formats

Numeric URI flags accept decimal and `0x`/`0X` prefixed hex.

Decimal example:

```bash
cargo run -p example-streamer-uses --bin mqtt_publisher --features mqtt-transport -- --uauthority authority-a --uentity 23456 --uversion 1 --resource 32769
```

Hex example:

```bash
cargo run -p example-streamer-uses --bin mqtt_publisher --features mqtt-transport -- --uauthority authority-a --uentity 0x5BA0 --uversion 0x1 --resource 0x8001
```

Invalid formats (for example underscores in numeric values) are rejected with deterministic errors that include the flag name, raw value, and expected range.

### SOME/IP caveat

SOME/IP binaries accept URI overrides, but runtime compatibility can still depend on application/service IDs configured in the selected `--vsomeip-config` file. If `--uentity` is overridden, the binaries emit a startup warning when that override may conflict with vsomeip config expectations.
