# up-streamer-rust

Generic, pluggable uStreamer that should be usable in most places we need
to bridge from one transport to another.

## Overview

Implementation of the uProtocol's uStreamer specification in Rust.

### Visual Breakdown

```mermaid
sequenceDiagram 
    participant main thread
    participant UPClientFoo thread / task
    participant UPClientBar thread / task

    main thread->>main thread: let utransport_foo: Arc<Mutex<Box<dyn UTransport>>> = Arc::new(Mutex::new(Box::new(UPClientFoo::new())))
    main thread->>main thread: let local_authority = ...
    main thread->>main thread: let local_route = Route::new(local_authority.clone(), utransport_foo.clone())

    main thread->>main thread: let utransport_bar: Arc<Mutex<Box<dyn UTransport>>> = Arc::new(Mutex::new(Box::new(UPClientBar::new())))
    main thread->>main thread: let remote_authority = ...
    main thread->>main thread: let remote_route = Route::new(remote_authority.clone(), utransport_bar.clone())
  
    main thread->>main thread: let ustreamer = UStreamer::new()
    main thread->>main thread: ustreamer.add_forwarding_rule(local_route, remote_route)
    main thread->>UPClientFoo thread / task: (within ustreamer.add_forwarding_rule()) <br> local_route.transport.lock().await.register_listener <br> (uauthority_to_uuri(remote_route.authority), forwarding_listener).await
    activate UPClientFoo thread / task

    main thread->>main thread: ustreamer.add_forwarding_rule(remote_route, local_route)
    main thread->>UPClientBar thread / task: (within ustreamer.add_forwarding_rule()) <br> remote_route.transport.lock().await.register_listener <br> (uauthority_to_uuri(local_route.authority), forwarding_listener).await
    activate UPClientBar thread / task

    par UPClientFoo thread / task pings forwarding listener

        UPClientFoo thread / task->>UPClientFoo thread / task: forwarding_listener.on_receive(UMessage)
        UPClientFoo thread / task->>UPClientFoo thread / task: out_transport.send(UMessage) <br> (out_transport => utransport_bar in this case)

    end

    par UPClientBar thread / task pings forwarding listener

        UPClientBar thread / task->>UPClientBar thread / task: forwarding_listener.on_receive(UMessage)
        UPClientBar thread / task->>UPClientBar thread / task: out_transport.send(UMessage) <br> (out_transport => utransport_foo in this case)

    end

    deactivate UPClientFoo thread / task
    deactivate UPClientBar thread / task
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