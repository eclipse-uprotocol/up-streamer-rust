# up-streamer-rust

Generic, pluggable uStreamer that should be usable in most places we need
to bridge from one transport to another.

## Overview

Implementation of the uProtocol's uStreamer specification in Rust.

### Visual Breakdown

```mermaid
flowchart TB

    subgraph ide0 [Usage Flow]
    subgraph ide5 [UTransportRouter]
    end

    subgraph ide6 [UTransportRouterInner]
    a4[Async Loop Here Awaiting \n Commands & Messages \n in OS thread to send over  \n non-thread-safe UTransport]
    end

    subgraph ide7 [UStreamer]
    a5[add_forwarding_rule]
    a6[delete_forwarding_rule]
    end

    subgraph ide8 [Route]
    end

    main-- start(UTransportBuilder) -->ide5
    ide5-- UTransportRouterHandle -->main
    ide5-- launch() --> ide6

    main-- new() -->ide7
    ide7-- UStreamer -->main

    main-- new(UAuthority, UTransportRouterHandle) --> ide8
    ide8-- Route -->main

    main-- add_forwarding_rule(in: Route, out: Route) -->ide7
    main-- delete_forwarding_rule(in: Route, out: Route) -->ide7

    a5-->a4
    a4-- Result -->a5

    a6-->a4
    a4-- Result -->a6

    end
```

`UTransport` is not thread-safe, so we opt for an approach where a `UTransportRouter` starts a `UTransportRouterInner` which launches an OS thread onto which the `Box<dyn UTransport>` is built and we await further commands / messages in an async loop.

### Generating cargo docs locally

Documentation can be generated locally with:
```bash
cargo doc --package up-streamer --open
```
which will open your browser to view the docs.

## Getting Started

### Working with the library

`up-streamer-rust` is generic and pluggable and can serve your needs so long as
1. Each transport you want to bridge over has a `up-client-foo-rust` library
   and UPClientFoo struct which has `impl`ed `UTransport`
2. `UTransportBuilder` has been `impl`ed on a struct so that a 
   `Box<dyn UTransport>` can be safely created by the `UTransportRouter`
   in the proper thread's context

### Usage

After following along with the [cargo docs](#generating-cargo-docs-locally) generated to add all your forwarding routes, you'll then need to keep the instantiated `UStreamer`, `UTransportRouter`, and `UTransportRouterHandle` around and then pause the main thread, so it will not exit, while the routing happens in the background threads spun up.

## Implementation Status

- [x] Routing of Request, Response, and Notification Messages
- [ ] Routing of Publish messages (requires further development of uSubscription interface)
- [x] Mechanism to retrieve messages received on and sent over transports