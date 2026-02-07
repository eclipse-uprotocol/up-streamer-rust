# up-rust 0.9 transport upgrade validation

This document captures the validation evidence for the migration to `up-rust 0.9` and the aligned transport versions.

## Target matrix

- `up-rust = 0.9.x`
- `up-transport-zenoh = 0.9.0`
- `up-transport-mqtt5 = 0.4.0`
- `up-transport-vsomeip` pinned to git rev `278ab26415559d6cb61f40facd21de822032cc83`
- Workspace MSRV: Rust `1.88`

## Compile and test validation

Executed from workspace root:

```bash
cargo check --workspace --all-targets
cargo test --workspace
```

Result:

- Workspace check passed.
- Workspace tests passed.

## Canonical smoke test A: Zenoh <-> SOME/IP request/response

Environment and setup:

- `source build/envsetup.sh highest`
- `LD_LIBRARY_PATH` includes bundled vsomeip libs from `target/debug/build/vsomeip-sys-*/out/vsomeip/vsomeip-install/lib`
- Streamer executed from `example-streamer-implementations`

Processes:

- Streamer: `cargo run --bin zenoh_someip --features "zenoh-transport vsomeip-transport bundled-vsomeip" -- --config "DEFAULT_CONFIG.json5"`
- Service: `cargo run -p example-streamer-uses --bin zenoh_service --features "zenoh-transport" -- --endpoint "tcp/localhost:7447"`
- Client: `cargo run -p example-streamer-uses --bin someip_client --features "vsomeip-transport bundled-vsomeip"`

Observed results (40s sample):

- `routing_miss_count=0`
- `response_count=38`
- `ok_commstatus_count=38`

Pass criteria satisfied:

- No repeating `Routing info for remote service could not be found`
- SOME/IP client receives `UMESSAGE_TYPE_RESPONSE` with `commstatus: Some(OK)`

## Canonical smoke test B: Zenoh <-> MQTT5 request/response

Environment and setup:

- Started broker with `docker compose -f utils/mosquitto/docker-compose.yaml up -d`
- Streamer executed from `configurable-streamer` with `CONFIG.json5`

Processes:

- Streamer: `cargo run -- --config "CONFIG.json5"`
- Service: `cargo run -p example-streamer-uses --bin zenoh_service --features "zenoh-transport" -- --endpoint "tcp/localhost:7447"`
- Client: `cargo run -p example-streamer-uses --bin mqtt_client --features "mqtt-transport"`

Observed results (35s sample):

- `request_count=33`
- `response_count=33`
- No `error`/`panic`/`failed` markers in streamer, service, or client logs

Pass criteria satisfied:

- Stable continuous request/response flow across streamer
- No transport-level failures in streamer/entity logs

## Temporary pin follow-up

`up-transport-vsomeip` remains pinned to git rev `278ab26415559d6cb61f40facd21de822032cc83` until a crates.io release with `up-rust 0.9` compatibility is available.

Follow-up after release: replace the git pin with the crates.io dependency and refresh `Cargo.lock`.
