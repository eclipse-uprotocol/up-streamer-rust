# up-streamer

Generic, pluggable uStreamer that should be usable in most places we need
to bridge from one transport to another.


## Overview

Implementation of the uProtocol's uStreamer specification in Rust.

### Visual Breakdown

```mermaid
sequenceDiagram 
    participant main thread
    participant TransportForwarder - Foo
    participant TransportForwarder - Bar
    participant UPClientFoo owned thread / task
    participant UPClientBar owned thread / task

    main thread->>main thread: let utransport_foo: Arc<dyn UTransport> = Arc::new(UPClientFoo::new())
    main thread->>main thread: let local_authority = ...
    main thread->>main thread: let local_endpoint = Endpoint::new(local_authority.clone(), utransport_foo.clone())

    main thread->>main thread: let utransport_bar: Arc<dyn UTransport> = Arc::new(UPClientBar::new())
    main thread->>main thread: let remote_authority = ...
    main thread->>main thread: let remote_endpoint = Endpoint::new(remote_authority.clone(), utransport_bar.clone())
  
    main thread->>main thread: let ustreamer = UStreamer::new()

    main thread->>main thread: ustreamer.add_route(local_endpoint, remote_endpoint)
    main thread->>TransportForwarder - Foo: launch TransportForwarder - Foo
    activate TransportForwarder - Foo
    main thread->>UPClientFoo owned thread / task: (within ustreamer.add_route()) <br> local_endpoint.transport.lock().await.register_listener <br> (uauthority_to_uuri(remote_endpoint.authority), forwarding_listener).await
    activate UPClientFoo owned thread / task

    main thread->>main thread: ustreamer.add_route(remote_endpoint, local_endpoint)
    main thread->>TransportForwarder - Bar: launch TransportForwarder - Bar
    activate TransportForwarder - Bar
    main thread->>UPClientBar owned thread / task: (within ustreamer.add_route()) <br> remote_endpoint.transport.lock().await.register_listener <br> (uauthority_to_uuri(local_endpoint.authority), forwarding_listener).await
    activate UPClientBar owned thread / task

    loop Park the main thread, let background tasks run until closing UStreamer app

        par UPClientFoo thread / task calls ForwardingListener.on_receive()

            UPClientFoo owned thread / task->>UPClientFoo owned thread / task: forwarding_listener.on_receive(UMessage)
            UPClientFoo owned thread / task-->>TransportForwarder - Foo: Send UMesssage over channel
            TransportForwarder - Foo->>TransportForwarder - Foo: out_transport.send(UMessage) <br> (out_transport => utransport_bar in this case)

        end

        par UPClientBar thread / task calls ForwardingListener.on_receive()

            UPClientBar owned thread / task->>UPClientBar owned thread / task: forwarding_listener.on_receive(UMessage)
            UPClientBar owned thread / task-->>TransportForwarder - Bar: Send UMesssage over channel
            TransportForwarder - Bar->>TransportForwarder - Bar: out_transport.send(UMessage) <br> (out_transport => utransport_foo in this case)

        end

        deactivate UPClientFoo owned thread / task
        deactivate UPClientBar owned thread / task
        deactivate TransportForwarder - Foo
        deactivate TransportForwarder - Bar

    end
```

### Generating cargo docs locally

Documentation can be generated locally with:

```bash
cargo doc --package up-streamer --open
```

which will open your browser to view the docs.

## Getting Started

### Working with the library

`up-streamer-rust` is generic and pluggable and can serve your needs so long as
* Each transport you want to bridge over has a `up-transport-foo-rust` library
  and UPTransportFoo struct which has `impl`ed `UTransport`

### Usage

After following along with the [cargo docs](#generating-cargo-docs-locally) generated to add all your forwarding rules, you'll then need to keep the instantiated `UStreamer` around and then pause the main thread, so it will not exit, while the routing happens in the background threads spun up.

## Implementation Status

- [x] Routing of Request, Response, and Notification Messages
- [ ] Routing of Publish messages (requires further development of uSubscription interface)
- [x] Mechanism to retrieve messages received on and sent over transports

## Benchmark Guardrails

`up-streamer` uses a pinned Criterion harness (`criterion = 0.5.1`) at:

- `up-streamer/benches/streamer_criterion.rs`

The helper script runs canonical benchmark commands and can invoke the Rust guardrail CLI:

```bash
export CRITERION_ARGS="--sample-size 60 --warm-up-time 3 --measurement-time 12 --noise-threshold 0.02"
if command -v taskset >/dev/null; then export BENCH_PIN_PREFIX="taskset -c 2"; else export BENCH_PIN_PREFIX=""; fi

scripts/bench_streamer_criterion.sh baseline
scripts/bench_streamer_criterion.sh candidate ergonomics_candidate_slice_a
scripts/bench_streamer_criterion.sh guardrail ergonomics_candidate_slice_a "$OPENCODE_CONFIG_DIR/reports/ergonomics-perf/bench-data/criterion-guardrail.json"
scripts/bench_streamer_criterion.sh export
```

Threshold pass/fail evaluation is handled only by the Rust CLI:

```bash
cargo run -p criterion-guardrail -- \
  --criterion-root target/criterion \
  --baseline ergonomics_baseline \
  --candidate ergonomics_candidate_slice_a \
  --throughput-threshold-pct 3 \
  --latency-threshold-pct 5 \
  --alloc-proxy-threshold-pct 5 \
  --report "$OPENCODE_CONFIG_DIR/reports/ergonomics-perf/bench-data/criterion-guardrail.json"
```
