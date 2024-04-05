/********************************************************************************
 * Copyright (c) 2024 Contributors to the Eclipse Foundation
 *
 * See the NOTICE file(s) distributed with this work for additional
 * information regarding copyright ownership.
 *
 * This program and the accompanying materials are made available under the
 * terms of the Apache License Version 2.0 which is available at
 * https://www.apache.org/licenses/LICENSE-2.0
 *
 * SPDX-License-Identifier: Apache-2.0
 ********************************************************************************/

use async_broadcast::broadcast;
use async_broadcast::{Receiver, Sender};
use async_std::sync::Mutex;
use async_std::task;
use async_trait::async_trait;
use example_up_client_foo::UPClientFoo;
use log::{debug, error};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;
use up_rust::UMessageType::{
    UMESSAGE_TYPE_NOTIFICATION, UMESSAGE_TYPE_PUBLISH, UMESSAGE_TYPE_REQUEST,
    UMESSAGE_TYPE_RESPONSE,
};
use up_rust::{
    Number, UAttributes, UAuthority, UEntity, UListener, UMessage, UStatus, UTransport,
    UUIDBuilder, UUri,
};
use up_streamer::{Route, UStreamer};

static FINISH_CLIENTS: AtomicBool = AtomicBool::new(false);

#[async_std::main]
async fn main() {
    // using async_broadcast to simulate communication protocol
    let (tx_1, rx_1) = broadcast(100);
    let (tx_2, rx_2) = broadcast(100);

    let utransport_foo: Arc<Mutex<Box<dyn UTransport>>> = Arc::new(Mutex::new(Box::new(
        UPClientFoo::new("upclient_foo", rx_1.clone(), tx_1.clone()).await,
    )));
    let utransport_bar: Arc<Mutex<Box<dyn UTransport>>> = Arc::new(Mutex::new(Box::new(
        UPClientFoo::new("upclient_bar", rx_2.clone(), tx_2.clone()).await,
    )));

    // setting up streamer to bridge between "foo" and "bar"
    let ustreamer = UStreamer::new("foo_bar_streamer", 100, 2);

    // setting up routes between authorities and protocols
    let local_route = Route::new("local_route", local_authority(), utransport_foo.clone());
    let remote_route = Route::new("remote_route", remote_authority(), utransport_bar.clone());

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

    let local_client_listener: Arc<dyn UListener> = Arc::new(LocalClientListener);
    let remote_client_listener: Arc<dyn UListener> = Arc::new(RemoteClientListener);

    // kicking off a "local_foo_client" and "remote_bar_client" in order to keep exercising
    // the streamer periodically
    let handle_1 = run_client(
        "local_foo_client".to_string(),
        local_client_uuri(10),
        remote_client_uuri(20),
        local_client_listener,
        tx_1.clone(),
        rx_1.clone(),
        publish_from_local_client_for_remote_client(10),
        notification_from_local_client_for_remote_client(10, 200),
        request_from_local_client_for_remote_client(10, 200),
        response_from_local_client_for_remote_client(10, 200),
        true,
    )
    .await;
    let handle_2 = run_client(
        "remote_bar_client".to_string(),
        remote_client_uuri(200),
        local_client_uuri(100),
        remote_client_listener,
        tx_2.clone(),
        rx_2.clone(),
        publish_from_remote_client_for_local_client(20),
        notification_from_remote_client_for_local_client(20, 10),
        request_from_remote_client_for_local_client(20, 10),
        response_from_remote_client_for_local_client(20, 10),
        true,
    )
    .await;

    debug!("waiting for a bit to let clients run");

    task::sleep(Duration::from_millis(1000)).await;

    debug!("Finished!");

    FINISH_CLIENTS.store(true, Ordering::SeqCst);

    debug!("set FINISH_CLIENTS to true");

    debug!(
        "FINISH_CLIENTS.load(): {}",
        FINISH_CLIENTS.load(Ordering::SeqCst)
    );

    let recv_1 = handle_1.join().expect("Unable to join on handle_1");
    let recv_2 = handle_2.join().expect("Unable to join on handle_2");

    println!("recv_1: {recv_1}");
    println!("recv_2: {recv_2}");

    debug!("All clients finished.");
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

pub fn notification_from_local_client_for_remote_client(local_id: u32, remote_id: u32) -> UMessage {
    UMessage {
        attributes: Some(UAttributes {
            source: Some(local_client_uuri(local_id)).into(),
            sink: Some(remote_client_uuri(remote_id)).into(),
            type_: UMESSAGE_TYPE_NOTIFICATION.into(),
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

pub fn notification_from_remote_client_for_local_client(remote_id: u32, local_id: u32) -> UMessage {
    UMessage {
        attributes: Some(UAttributes {
            source: Some(remote_client_uuri(remote_id)).into(),
            sink: Some(local_client_uuri(local_id)).into(),
            type_: UMESSAGE_TYPE_NOTIFICATION.into(),
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

#[derive(Clone)]
struct LocalClientListener;

#[async_trait]
impl UListener for LocalClientListener {
    async fn on_receive(&self, msg: UMessage) {
        debug!("within local_client_listener! msg: {:?}", msg);
    }

    async fn on_error(&self, err: UStatus) {
        debug!("within local_client_listener! err: {:?}", err);
    }
}

#[derive(Clone)]
struct RemoteClientListener;

#[async_trait]
impl UListener for RemoteClientListener {
    async fn on_receive(&self, msg: UMessage) {
        debug!("within remote_client_listener! msg: {:?}", msg);
    }

    async fn on_error(&self, err: UStatus) {
        debug!("within remote_client_listener! err: {:?}", err);
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn run_client(
    name: String,
    my_client_uuri: UUri,
    _other_client_uuri: UUri,
    listener: Arc<dyn UListener>,
    tx: Sender<Result<UMessage, UStatus>>,
    rx: Receiver<Result<UMessage, UStatus>>,
    _publish_msg: UMessage,
    notification_msg: UMessage,
    request_msg: UMessage,
    response_msg: UMessage,
    send: bool,
) -> JoinHandle<u64> {
    let join_handle = std::thread::spawn(move || {
        let res = task::block_on(async move {
            let client = UPClientFoo::new(&name, rx, tx).await;

            let register_res = client
                .register_listener(my_client_uuri.clone(), listener)
                .await;
            let Ok(_registration_string) = register_res else {
                panic!("Unable to register!");
            };

            // let register_res = client
            //     .register_listener(other_client_uuri.clone(), listener)
            //     .await;
            // let Ok(_registration_string) = register_res else {
            //     panic!("Unable to register!");
            // };

            loop {
                debug!("top of loop");

                // task::yield_now().await;

                task::sleep(Duration::from_millis(50)).await;

                if FINISH_CLIENTS.load(Ordering::SeqCst) {
                    debug!("Received a return request, performing the action...");
                    // Handle the return request
                    let times: u64 = client.times_received.load(Ordering::Relaxed);
                    println!("{name} had rx of: {times}");
                    return times;
                } else {
                    debug!("No request to quit yet");
                }

                debug!("-----------------------------------------------------------------------");

                if !send {
                    continue;
                }

                // TODO: Doesn't work currently
                //  Requires use of uSubscription
                // let send_res = client.send(publish_msg.clone()).await;
                // if send_res.is_err() {
                //     panic!("Unable to send from client: {}", &name);
                // }

                let mut notification_msg = notification_msg.clone();
                if let Some(attributes) = notification_msg.attributes.as_mut() {
                    let new_id = UUIDBuilder::build();
                    attributes.id.0 = Some(Box::new(new_id));
                }

                debug!(
                    "prior to sending from client {}, the request message: {:?}",
                    &name, &request_msg
                );

                let send_res = client.send(notification_msg).await;
                if send_res.is_err() {
                    error!("Unable to send from client: {}", &name);
                }

                let mut request_msg = request_msg.clone();
                if let Some(attributes) = request_msg.attributes.as_mut() {
                    let new_id = UUIDBuilder::build();
                    attributes.id.0 = Some(Box::new(new_id));
                }

                debug!(
                    "prior to sending from client {}, the request message: {:?}",
                    &name, &request_msg
                );

                let send_res = client.send(request_msg).await;
                if send_res.is_err() {
                    error!("Unable to send from client: {}", &name);
                }

                let mut response_msg = response_msg.clone();
                if let Some(attributes) = response_msg.attributes.as_mut() {
                    let new_id = UUIDBuilder::build();
                    attributes.id.0 = Some(Box::new(new_id));
                }

                debug!(
                    "prior to sending from client {}, the response message: {:?}",
                    &name, &response_msg
                );
            }
        });
        res
    });
    join_handle
}
