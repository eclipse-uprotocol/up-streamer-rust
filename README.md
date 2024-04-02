# up-streamer-rust

Generic, pluggable uStreamer that should be usable in most places we need
to bridge from one transport to another.

## Overview

Implementation of the uProtocol's uStreamer specification in Rust.

### Visual Breakdown

```mermaid
sequenceDiagram
    participant main
    participant utransport_router_handle_local
    participant utransport_router_handle_remote
    participant launch (foo)
    participant launch (bar)
    participant UTransportBuilderFoo
    participant UTransportBuilderBar
    participant dyn UTransport (foo)
    participant dyn UTransport (bar)

    main->>UTransportBuilderFoo: ::new()
    UTransportBuilderFoo->>main: UTransportBuilderFoo
    main->>UTransportRouter: ::start(UTransportBuilderFoo)
    activate UTransportRouter
    UTransportRouter->>UTransportRouterInner: ::start(UTransportBuilderFoo)
    UTransportRouterInner->>UTransportRouterInner: UTransportBuilderFoo.build()
    UTransportRouterInner->>launch (foo): .launch() on new OS Thread
    activate launch (foo)
    UTransportRouter->>main: UTransportRouterHandle (foo)
    deactivate UTransportRouter

    main->>UTransportBuilderBar: ::new()
    UTransportBuilderBar->>main: UTransportBuilderBar
    main->>UTransportRouter: ::start(UTransportBuilderBar)
    activate UTransportRouter
    UTransportRouter->>UTransportRouterInner: ::start(UTransportBuilderBar)
    UTransportRouterInner->>launch (bar): .launch() on new OS Thread
    activate launch (bar)
    UTransportRouter->>main: UTransportRouterHandle (bar)

    main->>Route: ::new(uauthority_foo, utransport_router_handle_foo)
    Route->>main: local_route
    main->>Route: ::new(uauthority_bar, utransport_router_handle_bar)
    Route->>main: remote_route

    main->>UStreamer: ::new()
    UStreamer->>main: ustreamer
    activate ustreamer

    main->>ustreamer: add_forwarding_rule(local_route, remote_route)
    ustreamer->>utransport_router_handle_local: register(local_route_authority, remote_route_authority, remote_route_channel)
    utransport_router_handle_local-->>launch (foo): UTransportRouterCommand::RegisterUnregisterControl: <br> {local_route_authority, remote_route_authority, remote_route_channel} <br> sent on command_sender
    launch (foo)->>launch (foo): handle_command()
    launch (foo)->>dyn UTransport (foo): register_listener(local_route_authority, request_response_notification_forwarding_callback)
    activate request_response_notification_forwarding_callback (foo)
    launch (foo)-->>utransport_router_handle_local: Result of registration
    utransport_router_handle_local->>ustreamer: Result of registration
    ustreamer->>main: Result of registration

    main->>ustreamer: add_forwarding_rule(remote_route, local_route)
    ustreamer->>utransport_router_handle_remote: register(remote_route_authority, local_route_authority, local_route_channel)
    utransport_router_handle_remote-->>launch (bar): UTransportRouterCommand::RegisterUnregisterControl: <br> {remote_route_authority, local_route_authority, local_route_channel} <br> sent on command_sender
    launch (bar)->>launch (bar): handle_command()
    launch (bar)->>dyn UTransport (bar): register_listener(remote_route_authority, request_response_notification_forwarding_callback)
    activate request_response_notification_forwarding_callback (bar)
    launch (bar)-->>utransport_router_handle_remote: Result of registration
    utransport_router_handle_remote->>ustreamer: Result of registration
    ustreamer->>main: Result of registration

    opt request_response_notification_forwarding_callback (foo) triggered for local_authority
        request_response_notification_forwarding_callback (foo)-->>launch (bar): UMessage from Foo transport headed to launch (bar) async loop
        launch (bar)->>launch (bar): send_over_utransport(UMessage)
        launch (bar)->>dyn UTransport (bar): dyn UTransport (bar).send(UMessage)
    end

    opt request_response_notification_forwarding_callback (bar) triggered for remote_authority
        request_response_notification_forwarding_callback (bar)-->>launch (foo): UMessage from Bar transport headed to launch (foo) async loop
        launch (foo)->>launch (foo): send_over_utransport(UMessage)
        launch (foo)->>dyn UTransport (foo): dyn UTransport (foo).send(UMessage)
    end

    deactivate launch (foo)
    deactivate launch (bar)
    deactivate ustreamer
    deactivate request_response_notification_forwarding_callback (foo)
    deactivate request_response_notification_forwarding_callback (bar)
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