# up-linux-streamer-plugin

## Using the plugin

### Build the plugin

```bash
LD_LIBRARY_PATH=<your/path/to/vsomeip/lib> VSOMEIP_LIB_DIR=<your/path/to/vsomeip/lib> cargo build
```

### Copy plugin and configs to standalone folder

```bash
mkdir -p my/new/standalone/zenohd/path
cp target/debug/libzenoh_plugin_up_linux_streamer.so my/new/standalone/zenohd/path/
cp up-linux-streamer-plugin/DEFAULT_CONFIG.json5 my/new/standalone/zenohd/path/
cp -r up-linux-streamer-plugin/vsomeip-configs my/new/standalone/zenohd/path/
```

### Using the plugin

Because up-transport-zenoh-rust uses minimum supported Rust version (**MSRV**) of 1.74.0 and up-rust uses MSRV of 1.72.1, we need to build Zenoh from source in order to get a compatible zenohd (Zenoh Router). The following steps describe how to do so.

### Getting Zenoh

https:

```bash
git clone https://github.com/eclipse-zenoh/zenoh.git
```

ssh:

```bash
git clone git@github.com:eclipse-zenoh/zenoh.git
```

### Check out a compatible release tag

Currently this is:

```bash
git checkout release/0.11.0
```

You can `git checkout -b <my_relevant_tag>` here if you wish to save these changes to some fork for later usage.

### Modify MSRV of Zenoh

In zenoh/rust-toolchain.toml, modify to version `1.74.0`:

```toml
[toolchain]
channel = "1.74.0"
```

In zenoh/Cargo.toml, modify rust-version:

```toml
[workspace.package]
rust-version = "1.74.0"
```

### Make a build of Zenoh and copy to standalon folder

Within the zenoh folder:

```bash
cargo build
```

```bash
cp target/debug/zenohd my/new/standalone/zenohd/path
```

### Running zenohd with up-linux-streamer-plugin

From within the `my/new/standalone/zenohd/path`:

```bash
RUST_LOG=trace LD_LIBRARY_PATH=<your/path/to/vsomeip/lib>  VSOMEIP_LIB_DIR=<your/path/to/vsomeip/lib> ./zenohd --config DEFAULT_CONFIG.json5
```

You can also run without the `RUST_LOG=trace` environment variable prepended and should for production use cases. It can be used for debugging purposes.

### You're done!