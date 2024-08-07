# up-streamer-rust

## up-streamer

Generic, pluggable uStreamer that should be usable in most places we need
to write a uStreamer application to bridge from one transport to another.

Reference its README.md for further details.

## up-linux-streamer

Concrete implementation of a uStreamer as a binary.

Reference its README.md for further details.

## Building

Included is a dependency on [up-transport-vsomeip-rust](https://github.com/eclipse-uprotocol/up-transport-vsomeip-rust) in order to communicate via SOME/IP that imposes some additional build considerations.

Please reference the documentation for [vsomeip-sys](https://github.com/eclipse-uprotocol/up-transport-vsomeip-rust/tree/main/vsomeip-sys) and note
* the build requirements for vsomeip in the linked documentation in the COVESA repo
* the environment variables which must be set

