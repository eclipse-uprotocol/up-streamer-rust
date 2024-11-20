# up-streamer-rust: what is in this repository

## up-streamer

Generic, pluggable uStreamer that should be usable in most places we need
to write a uStreamer application to bridge from one transport to another.

Reference its README.md for further details.

## example-streamer-implementations

Two concrete implementations of a uStreamer as a binary. These can be used out of the box either to try running different UStreamer setups or to directly use them in a project!

Reference the README.md there for more details.

## example-streamer-uses

A number of UEntity examples for SOME/IP, Zenoh and MQTT5. These can be used together with the example streamer implementations to run basic setups of either a publisher and a subscriber, or a service and a client.

Reference the README.md there for more details.

## Building

### Only `up-streamer`

If you only want to compile the library itself, you can as normal:

```
cargo build
```

### Also build the reference Zenoh, vsomeip streamer implementations

You'll need to use the feature flags `vsomeip-transport`, `zenoh-transport` or `mqtt-transport` depending on which implementation you want to build. You then also have the option of including your own vsomeip or using one bundled in `up-transport-vsomeip`.

For the bundled option, set the following environment variables (for example in your .cargo/config.toml file):

```toml
[env]
GENERIC_CPP_STDLIB_PATH=<path to your c++ stdlib (for example /usr/include/c++/13)>
ARCH_SPECIFIC_CPP_STDLIB_PATH=<path to your c++ stdlib (for example /usr/include/x86_64-linux-gnu/c++/13)>
```

```bash
cargo build --features vsomeip-transport,bundled-vsomeip,zenoh-transport
```

The environment variables are necessary because of a workaround done in `up-transport-vsomeip` due to not being able to figure out another way to compile vsomeip without them. (If you can figure out how to avoid this, I'm all ears!)

Please reference the documentation for [vsomeip-sys](https://github.com/eclipse-uprotocol/up-transport-vsomeip-rust/tree/main/vsomeip-sys) for more details on:
* the build requirements for vsomeip in the linked documentation in the COVESA repo
* the environment variables which must be set

