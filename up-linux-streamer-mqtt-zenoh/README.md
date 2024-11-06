# up-linux-streamer

Concrete implementation of a uStreamer as a binary.

## Configuration

Reference the `DEFAULT_CONFIG.json5` configuration file to understand configuration options.

As well, the `ZENOH_CONFIG.json5` file is used to set Zenoh configurations. By default, it is only used to set listening endpoints, but can be used with more configurations according to [Zenoh's page on it](https://zenoh.io/docs/manual/configuration/#configuration-files).

### Bundled vsomeip or bring your own

The default is to build a bundled version of [vsomeip](https://github.com/COVESA/vsomeip) for use by the `up-transport-vsomeip` crate.

The vsomeip library is used to communicate over [SOME/IP](https://some-ip.com/) to mechatronics devices.

If you wish to bring your own vsomeip install, you can use the flag `--no-default-features` flag when building with `cargo build`. For more details on required environment variables when building `up-transport-vsomeip-rust`, reference the README for [vsomeip-sys](https://github.com/eclipse-uprotocol/up-transport-vsomeip-rust/tree/main/vsomeip-sys).

### Running the `up-linux-streamer`

```bash
LD_LIBRARY_PATH=$LD_LIBRARY_PATH:<path/to/vsomeip/lib> cargo run -- --config up-linux-streamer/DEFAULT_CONFIG.json5
```
