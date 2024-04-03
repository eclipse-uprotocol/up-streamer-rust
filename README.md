# up-streamer-rust

Generic, pluggable uStreamer that should be usable in most places we need
to bridge from one transport to another.

## Overview

Implementation of the uProtocol's uStreamer specification in Rust.

### Visual Breakdown

```mermaid
sequenceDiagram
    participant main
    participant UTransportRouterInner (foo)
    participant UTransportRouterInner (bar)

    main->>main: let utransport_builder_foo = UTransportBuilderFoo::new()
    main->>main: let utransport_builder_bar = UTransportBuilderBar::new()

    main->>main: let utransport_router_handle_local = UTransportRouter::start(utransport_builder_foo)
    main->>main: UTransportRouterInner::start()
    main->>UTransportRouterInner (foo): launch()
    activate UTransportRouterInner (foo)
    UTransportRouterInner (foo)->>main: UTransportRouterHandle <br> utransport_router_handle_local

    main->>main: let utransport_router_handle_remote = UTransportRouter::start(utransport_builder_bar)
    main->>main: UTransportRouterInner::start()
    main->>UTransportRouterInner (bar): launch()
    activate UTransportRouterInner (bar)
    UTransportRouterInner (bar)->>main: UTransportRouterHandle <br> utransport_router_handle_remote

    main->>main: local_route = Route::new(local_authority, utransport_router_handle_local)
    main->>main: remote_route = Route::new(remote_authority, utransport_route_handle_remote)

    main->>main: ustreamer = UStreamer::new()

    main->>main: ustreamer.add_forwarding_rule(local_route, remote_route)
    main->>main: utransport_router_handle_local.register(local_authority, remote_authority, remote_sender)
    main-->>UTransportRouterInner (foo): UTransportRouterCommand <br> RegisterUnregisterControl <br> (local_authority, remote_authority, remote_sender)
    UTransportRouterInner (foo)->>UTransportRouterInner (foo): handle_message(UTransportRouterCommand)
    UTransportRouterInner (foo)->>UTransportRouterInner (foo): utransport.register_listener(local_authority, request_response_notification_forwarding_callback)
    UTransportRouterInner (foo)-->>main: Result

    main->>main: ustreamer.add_forwarding_rule(remote_route, local_route)
    main->>main: utransport_router_handle_remote.register(remote_authority, local_authority, local_sender)
    main-->>UTransportRouterInner (bar): UTransportRouterCommand <br> RegisterUnregisterControl <br> (remote_authority, local_authority, local_sender)
    UTransportRouterInner (bar)->>UTransportRouterInner (bar): handle_message(UTransportRouterCommand)
    UTransportRouterInner (bar)->>UTransportRouterInner (bar): utransport.register_listener(local_authority, request_response_notification_forwarding_callback
    UTransportRouterInner (bar)-->>main: Result

    par UTransportRouterInner (foo) has its callback pinged on the registered authority
        UTransportRouterInner (foo)->>UTransportRouterInner (foo): request_response_notification_forwarding_callback()
        UTransportRouterInner (foo)-->>UTransportRouterInner (bar): Arc<UMessage>
        UTransportRouterInner (bar)->>UTransportRouterInner (bar): utransport.send(umessage)
    end

    par UTransportRouterInner (bar) has its callback pinged on the registered authority
        UTransportRouterInner (bar)->>UTransportRouterInner (bar): request_response_notification_forwarding_callback()
        UTransportRouterInner (bar)-->>UTransportRouterInner (foo): Arc<UMessage>
        UTransportRouterInner (foo)->>UTransportRouterInner (foo): utransport.send(umessage)
    end

    deactivate UTransportRouterInner (foo)
    deactivate UTransportRouterInner (bar)
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