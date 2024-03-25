use async_broadcast::broadcast;
use async_broadcast::{Receiver, Sender};
use async_std::{future, task};
use example_up_client_foo::{UPClientFoo, UTransportBuilderFoo};
use std::sync::Arc;
use std::time::Duration;
use up_rust::UMessageType::{UMESSAGE_TYPE_PUBLISH, UMESSAGE_TYPE_REQUEST, UMESSAGE_TYPE_RESPONSE};
use up_rust::{Number, UAttributes, UAuthority, UEntity, UMessage, UStatus, UTransport, UUri};
use up_streamer::{Route, UStreamer, UTransportRouter};

#[async_std::main]
async fn main() {
    // using async_broadcast to simulate communication protocol
    let (tx_1, rx_1) = broadcast(100);
    let (tx_2, rx_2) = broadcast(100);

    // kicking off a UTransportRouter for protocol "foo" and retrieving its UTransportRouterHandle
    let utransport_builder_foo =
        UTransportBuilderFoo::new("utransport_builder_foo", rx_1.clone(), tx_1.clone());
    let utransport_router_handle_foo = Arc::new(
        UTransportRouter::start("foo".to_string(), utransport_builder_foo, 100, 100).unwrap(),
    );

    // kicking off a UTransportRouter for protocol "bar" and retrieving its UTransportRouterHandle
    let utransport_builder_bar =
        UTransportBuilderFoo::new("utransport_builder_bar", rx_2.clone(), tx_2.clone());
    let utransport_router_handle_bar = Arc::new(
        UTransportRouter::start("bar".to_string(), utransport_builder_bar, 100, 100).unwrap(),
    );

    // setting up streamer to bridge between "foo" and "bar"
    let ustreamer = UStreamer::new("foo_bar_streamer");

    // setting up routes between authorities and protocols
    let local_route = Route::new(&local_authority(), &utransport_router_handle_foo);
    let remote_route = Route::new(&remote_authority(), &utransport_router_handle_bar);

    // adding local to remote routing
    let add_forwarding_rule_res = ustreamer
        .add_forwarding_rule(local_route.clone(), remote_route.clone())
        .await;
    assert!(add_forwarding_rule_res.is_ok());

    // adding remote to local routing
    let add_forwarding_rule_res = ustreamer
        .add_forwarding_rule(remote_route.clone(), local_route.clone())
        .await;
    assert!(add_forwarding_rule_res.is_ok());

    // kicking off a "local_foo_client" and "remote_bar_client" in order to keep exercising
    // the streamer periodically
    run_client(
        "local_foo_client".to_string(),
        local_client_uuri(10),
        remote_client_uuri(20),
        local_client_listener,
        tx_1.clone(),
        rx_1.clone(),
        publish_from_local_client_for_remote_client(100),
        request_from_local_client_for_remote_client(100, 200),
        response_from_local_client_for_remote_client(100, 200),
        true,
    )
    .await;
    run_client(
        "remote_bar_client".to_string(),
        remote_client_uuri(200),
        local_client_uuri(100),
        remote_client_listener,
        tx_2.clone(),
        rx_2.clone(),
        publish_from_remote_client_for_local_client(20),
        request_from_remote_client_for_local_client(20, 10),
        response_from_remote_client_for_local_client(20, 10),
        true,
    )
    .await;

    // pause current task and wait forever
    future::pending::<()>().await;
}

pub fn local_authority() -> UAuthority {
    UAuthority {
        name: Some("local_authority".to_string()),
        number: Number::Ip(vec![192, 168, 1, 100]).into(),
        ..Default::default()
    }
}

pub fn remote_authority() -> UAuthority {
    UAuthority {
        name: Some("remote_authority".to_string()),
        number: Number::Ip(vec![192, 168, 1, 200]).into(),
        ..Default::default()
    }
}

pub fn local_client_uuri(id: u32) -> UUri {
    UUri {
        authority: Some(local_authority()).into(),
        entity: Some(UEntity {
            name: format!("local_entity_{id}").to_string(),
            id: Some(id),
            version_major: Some(1),
            ..Default::default()
        })
        .into(),
        ..Default::default()
    }
}

pub fn remote_client_uuri(id: u32) -> UUri {
    UUri {
        authority: Some(remote_authority()).into(),
        entity: Some(UEntity {
            name: format!("remote_entity_{id}").to_string(),
            id: Some(id),
            version_major: Some(1),
            ..Default::default()
        })
        .into(),
        ..Default::default()
    }
}

pub fn publish_from_local_client_for_remote_client(local_id: u32) -> UMessage {
    UMessage {
        attributes: Some(UAttributes {
            source: Some(local_client_uuri(local_id)).into(),
            type_: UMESSAGE_TYPE_PUBLISH.into(),
            ..Default::default()
        })
        .into(),
        ..Default::default()
    }
}

pub fn request_from_local_client_for_remote_client(local_id: u32, remote_id: u32) -> UMessage {
    UMessage {
        attributes: Some(UAttributes {
            source: Some(local_client_uuri(local_id)).into(),
            sink: Some(remote_client_uuri(remote_id)).into(),
            type_: UMESSAGE_TYPE_REQUEST.into(),
            ..Default::default()
        })
        .into(),
        ..Default::default()
    }
}

pub fn response_from_local_client_for_remote_client(local_id: u32, remote_id: u32) -> UMessage {
    UMessage {
        attributes: Some(UAttributes {
            source: Some(local_client_uuri(local_id)).into(),
            sink: Some(remote_client_uuri(remote_id)).into(),
            type_: UMESSAGE_TYPE_RESPONSE.into(),
            ..Default::default()
        })
        .into(),
        ..Default::default()
    }
}

pub fn publish_from_remote_client_for_local_client(remote_id: u32) -> UMessage {
    UMessage {
        attributes: Some(UAttributes {
            source: Some(remote_client_uuri(remote_id)).into(),
            type_: UMESSAGE_TYPE_PUBLISH.into(),
            ..Default::default()
        })
        .into(),
        ..Default::default()
    }
}

pub fn request_from_remote_client_for_local_client(remote_id: u32, local_id: u32) -> UMessage {
    UMessage {
        attributes: Some(UAttributes {
            source: Some(remote_client_uuri(remote_id)).into(),
            sink: Some(local_client_uuri(local_id)).into(),
            type_: UMESSAGE_TYPE_REQUEST.into(),
            ..Default::default()
        })
        .into(),
        ..Default::default()
    }
}

pub fn response_from_remote_client_for_local_client(remote_id: u32, local_id: u32) -> UMessage {
    UMessage {
        attributes: Some(UAttributes {
            source: Some(remote_client_uuri(remote_id)).into(),
            sink: Some(local_client_uuri(local_id)).into(),
            type_: UMESSAGE_TYPE_RESPONSE.into(),
            ..Default::default()
        })
        .into(),
        ..Default::default()
    }
}

pub fn local_client_listener(received: Result<UMessage, UStatus>) {
    println!("within local_client_listener! received: {:?}", received);
}

pub fn remote_client_listener(received: Result<UMessage, UStatus>) {
    println!("within remote_client_listener! received: {:?}", received);
}

#[allow(clippy::too_many_arguments)]
pub async fn run_client(
    name: String,
    my_client_uuri: UUri,
    other_client_uuri: UUri,
    listener: fn(Result<UMessage, UStatus>),
    tx: Sender<Result<UMessage, UStatus>>,
    rx: Receiver<Result<UMessage, UStatus>>,
    publish_msg: UMessage,
    request_msg: UMessage,
    response_msg: UMessage,
    send: bool,
) {
    std::thread::spawn(move || {
        task::block_on(async move {
            let client = UPClientFoo::new(&name, rx, tx).await;

            let register_res = client
                .register_listener(my_client_uuri.clone(), Box::new(listener))
                .await;
            let Ok(_registration_string) = register_res else {
                panic!("Unable to register!");
            };

            let register_res = client
                .register_listener(other_client_uuri.clone(), Box::new(listener))
                .await;
            let Ok(_registration_string) = register_res else {
                panic!("Unable to register!");
            };

            loop {
                task::sleep(Duration::from_millis(5000)).await;

                println!("-----------------------------------------------------------------------");

                if !send {
                    continue;
                }

                let send_res = client.send(publish_msg.clone()).await;
                if send_res.is_err() {
                    panic!("Unable to send from client: {}", &name);
                }

                let send_res = client.send(request_msg.clone()).await;
                if send_res.is_err() {
                    panic!("Unable to send from client: {}", &name);
                }

                let send_res = client.send(response_msg.clone()).await;
                if send_res.is_err() {
                    panic!("Unable to send from client: {}", &name);
                }
            }
        });
    });
}
