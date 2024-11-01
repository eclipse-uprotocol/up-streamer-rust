# up-streamer-rust

## up-streamer

Generic, pluggable uStreamer that should be usable in most places we need
to write a uStreamer application to bridge from one transport to another.

Reference its README.md for further details.

## up-linux-streamer

Concrete implementation of a uStreamer as a binary.

Reference its README.md for further details.

## Building

### Only `up-streamer`

If you only want to compile the library itself, you can as normal:

```
cargo build
```

### Also build the reference Zenoh, vsomeip streamer implementations

You'll need to use the feature flags `vsomeip-transport` and `zenoh-transport`. You then also have the option of including your own vsomeip or using one bundled in `up-transport-vsomeip`.

For the bundled option, the following:

```
GENERIC_CPP_STDLIB_PATH=<see-below> ARCH_SPECIFIC_CPP_STDLIB_PATH=<see-below> cargo build --features vsomeip-transport,bundled-vsomeip,zenoh-transport
```

where for reference on my machine:

```
GENERIC_CPP_STDLIB_PATH=/usr/include/c++/13
ARCH_SPECIFIC_CPP_STDLIB_PATH=/usr/include/x86_64-linux-gnu/c++/13
```

These environment varialbes are necessary because of a workaround done in `up-transport-vsomeip` due to not being able to figure out another way to compile vsomeip without them. (If you can figure out how to avoid this, I'm all ears!)

Please reference the documentation for [vsomeip-sys](https://github.com/eclipse-uprotocol/up-transport-vsomeip-rust/tree/main/vsomeip-sys) for more details on:
* the build requirements for vsomeip in the linked documentation in the COVESA repo
* the environment variables which must be set

