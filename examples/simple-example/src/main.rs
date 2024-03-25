use async_broadcast::broadcast;
use async_broadcast::{Receiver, Sender};
use async_std::{future, task};
use example_up_client_foo::{UPClientFoo, UTransportBuilderFoo};
use std::sync::Arc;
use std::time::Duration;
use up_rust::UMessageType::{UMESSAGE_TYPE_PUBLISH, UMESSAGE_TYPE_REQUEST, UMESSAGE_TYPE_RESPONSE};
use up_rust::{
    Number, UAttributes, UAuthority, UEntity, UMessage, UPayload, UStatus, UTransport, UUri,
};
use up_streamer::{Route, UStreamer, UTransportRouter};

pub fn client_1_authority() -> UAuthority {
    UAuthority {
        name: Some("client_1_authority".to_string()),
        number: Number::Ip(vec![192, 168, 1, 100]).into(),
        ..Default::default()
    }
}

pub fn client_2_authority() -> UAuthority {
    UAuthority {
        name: Some("client_2_authority".to_string()),
        number: Number::Ip(vec![192, 168, 1, 200]).into(),
        ..Default::default()
    }
}

pub fn client_1_uuri() -> UUri {
    UUri {
        authority: Some(client_1_authority()).into(),
        entity: Some(UEntity {
            name: "entity_1".to_string(),
            id: Some(10),
            version_major: Some(1),
            ..Default::default()
        })
        .into(),
        ..Default::default()
    }
}

pub fn client_2_uuri() -> UUri {
    UUri {
        authority: Some(client_2_authority()).into(),
        entity: Some(UEntity {
            name: "entity_2".to_string(),
            id: Some(10),
            version_major: Some(1),
            ..Default::default()
        })
        .into(),
        ..Default::default()
    }
}

pub fn publish_from_client_1_for_client_2() -> UMessage {
    UMessage {
        attributes: Some(UAttributes {
            source: Some(client_1_uuri()).into(),
            type_: UMESSAGE_TYPE_PUBLISH.into(),
            ..Default::default()
        })
        .into(),
        payload: Some(UPayload {
            length: None,
            format: Default::default(),
            data: None,
            special_fields: Default::default(),
        })
        .into(),
        ..Default::default()
    }
}

pub fn request_from_client_1_for_client_2() -> UMessage {
    UMessage {
        attributes: Some(UAttributes {
            source: Some(client_1_uuri()).into(),
            sink: Some(client_2_uuri()).into(),
            type_: UMESSAGE_TYPE_REQUEST.into(),
            ..Default::default()
        })
        .into(),
        payload: Some(UPayload {
            length: None,
            format: Default::default(),
            data: None,
            special_fields: Default::default(),
        })
        .into(),
        ..Default::default()
    }
}

pub fn response_from_client_1_for_client_2() -> UMessage {
    UMessage {
        attributes: Some(UAttributes {
            source: Some(client_1_uuri()).into(),
            sink: Some(client_2_uuri()).into(),
            type_: UMESSAGE_TYPE_RESPONSE.into(),
            ..Default::default()
        })
        .into(),
        payload: Some(UPayload {
            length: None,
            format: Default::default(),
            data: None,
            special_fields: Default::default(),
        })
        .into(),
        ..Default::default()
    }
}

pub fn publish_from_client_2_for_client_1() -> UMessage {
    UMessage {
        attributes: Some(UAttributes {
            source: Some(client_2_uuri()).into(),
            type_: UMESSAGE_TYPE_PUBLISH.into(),
            ..Default::default()
        })
        .into(),
        payload: Some(UPayload {
            length: None,
            format: Default::default(),
            data: None,
            special_fields: Default::default(),
        })
        .into(),
        ..Default::default()
    }
}

pub fn request_from_client_2_for_client_1() -> UMessage {
    UMessage {
        attributes: Some(UAttributes {
            source: Some(client_2_uuri()).into(),
            sink: Some(client_1_uuri()).into(),
            type_: UMESSAGE_TYPE_REQUEST.into(),
            ..Default::default()
        })
        .into(),
        payload: Some(UPayload {
            length: None,
            format: Default::default(),
            data: None,
            special_fields: Default::default(),
        })
        .into(),
        ..Default::default()
    }
}

pub fn response_from_client_2_for_client_1() -> UMessage {
    UMessage {
        attributes: Some(UAttributes {
            source: Some(client_2_uuri()).into(),
            sink: Some(client_1_uuri()).into(),
            type_: UMESSAGE_TYPE_RESPONSE.into(),
            ..Default::default()
        })
        .into(),
        payload: Some(UPayload {
            length: None,
            format: Default::default(),
            data: None,
            special_fields: Default::default(),
        })
        .into(),
        ..Default::default()
    }
}

pub fn client_1_listener(received: Result<UMessage, UStatus>) {
    println!("within client_1_listener! received: {:?}", received);
}

pub fn client_2_listener(received: Result<UMessage, UStatus>) {
    println!("within client_2_listener! received: {:?}", received);
}

#[allow(clippy::too_many_arguments)]
pub async fn run_client(
    name: String,
    other_client_uuri: UUri,
    listener: fn(Result<UMessage, UStatus>),
    tx: Sender<Result<UMessage, UStatus>>,
    rx: Receiver<Result<UMessage, UStatus>>,
    publish_msg: UMessage,
    request_msg: UMessage,
    response_msg: UMessage,
) {
    std::thread::spawn(move || {
        task::block_on(async move {
            let client = UPClientFoo::new(&name, rx, tx).await;

            let register_res = client
                .register_listener(other_client_uuri.clone(), Box::new(listener))
                .await;
            let Ok(_registration_string) = register_res else {
                panic!("Unable to register!");
            };

            loop {
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

                task::sleep(Duration::from_millis(1000)).await;
            }
        });
    });
}

#[async_std::main]
async fn main() {
    let (tx_1, rx_1) = broadcast(100);
    let (tx_2, rx_2) = broadcast(100);

    let utransport_builder_1 =
        UTransportBuilderFoo::new("utransport_builder_1", rx_1.clone(), tx_1.clone());
    let utransport_router_handle_1 = Arc::new(
        UTransportRouter::start("foo_1".to_string(), utransport_builder_1, 100, 100).unwrap(),
    );

    let utransport_builder_2 =
        UTransportBuilderFoo::new("utransport_builder_2", rx_2.clone(), tx_2.clone());
    let utransport_router_handle_2 = Arc::new(
        UTransportRouter::start("foo_2".to_string(), utransport_builder_2, 100, 100).unwrap(),
    );

    let route_a = Route::new(&client_1_authority(), &utransport_router_handle_1);

    let route_b = Route::new(&client_2_authority(), &utransport_router_handle_2);

    let ustreamer = UStreamer::new("my_streamer");

    let add_forwarding_rule_res = ustreamer
        .add_forwarding_rule(route_a.clone(), route_b.clone())
        .await;
    assert!(add_forwarding_rule_res.is_ok());

    let add_forwarding_rule_res = ustreamer
        .add_forwarding_rule(route_b.clone(), route_a.clone())
        .await;
    assert!(add_forwarding_rule_res.is_ok());

    run_client(
        "client_1".to_string(),
        client_2_uuri(),
        client_1_listener,
        tx_1.clone(),
        rx_1.clone(),
        publish_from_client_1_for_client_2(),
        request_from_client_1_for_client_2(),
        response_from_client_1_for_client_2(),
    )
    .await;
    run_client(
        "client_2".to_string(),
        client_1_uuri(),
        client_2_listener,
        tx_2.clone(),
        rx_2.clone(),
        publish_from_client_2_for_client_1(),
        request_from_client_2_for_client_1(),
        response_from_client_2_for_client_1(),
    )
    .await;

    future::pending::<()>().await;
}
