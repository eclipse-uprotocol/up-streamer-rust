# up-linux-streamer

Concrete implementation of a uStreamer as a binary.

## Configuration

Reference the `DEFAULT_CONFIG.json5` configuration file to understand configuration options.

### Bundled vsomeip or bring your own

The default is to build a bundled version of vsomeip for use by the `up-transport-vsomeip` crate.

If you wish to bring your own vsomeip install, you can use the flag `--no-default-features` flag when building with `cargo build`. For more details on required environment variables when building `up-transport-vsomeip-rust`, reference the README for [vsomeip-sys](https://github.com/eclipse-uprotocol/up-transport-vsomeip-rust/tree/main/vsomeip-sys).

### Running the `up-linux-streamer`

```bash
LD_LIBRARY_PATH=$LD_LIBRARY_PATH:<path/to/vsomeip/lib> cargo run -- --config up-linux-streamer/DEFAULT_CONFIG.json5
```
