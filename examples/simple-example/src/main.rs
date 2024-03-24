use crate::up_client_foo::{UPClientFoo, UTransportBuilderFoo};
use async_std::{channel, task};
use std::sync::Arc;
use std::time::Duration;
use up_rust::UMessageType::{UMESSAGE_TYPE_PUBLISH, UMESSAGE_TYPE_REQUEST};
use up_rust::{
    Number, UAttributes, UAuthority, UEntity, UMessage, UPayload, UStatus, UTransport, UUri,
};
use up_streamer::{Route, UStreamer, UTransportRouter};

mod up_client_foo;

pub type UtransportListener = Box<dyn Fn(Result<UMessage, UStatus>) + Send + Sync + 'static>;

pub fn listener_fn(received: Result<UMessage, UStatus>) {
    println!("within listener_fn! received: {:?}", received);
}

#[async_std::main]
async fn main() {
    let (tx_1, rx_1) = channel::bounded(100);
    let (tx_2, rx_2) = channel::bounded(100);
    let (tx_3, rx_3) = channel::bounded(100);
    let (_tx_4, rx_4) = channel::bounded(100);

    let client_1 = UPClientFoo::new("client_1", rx_4.clone(), tx_1.clone()).await;
    let client_2 = UPClientFoo::new("client_2", rx_3.clone(), tx_2.clone()).await;

    let utransport_builder_1 =
        UTransportBuilderFoo::new("utransport_builder_1", rx_1.clone(), tx_2.clone());
    let utransport_router_handle_1 = Arc::new(
        UTransportRouter::start("foo_1".to_string(), utransport_builder_1, 100, 100).unwrap(),
    );

    let utransport_builder_2 =
        UTransportBuilderFoo::new("utransport_builder_2", rx_2.clone(), tx_3.clone());
    let utransport_router_handle_2 = Arc::new(
        UTransportRouter::start("foo_2".to_string(), utransport_builder_2, 100, 100).unwrap(),
    );

    let client_1_authority = UAuthority {
        name: Some("client_1_authority".to_string()),
        number: Number::Ip(vec![192, 168, 1, 100]).into(),
        ..Default::default()
    };
    let route_a = Route::new(&client_1_authority, &utransport_router_handle_1);

    let client_2_authority = UAuthority {
        name: Some("client_2_authority".to_string()),
        number: Number::Ip(vec![192, 168, 1, 200]).into(),
        ..Default::default()
    };
    let route_b = Route::new(&client_2_authority, &utransport_router_handle_2);

    let ustreamer = UStreamer::new("my_streamer");
    let add_forwarding_rule_res = ustreamer
        .add_forwarding_rule(route_a.clone(), route_b.clone())
        .await;
    assert!(add_forwarding_rule_res.is_ok());
    let add_forwarding_rule_res = ustreamer
        .add_forwarding_rule(route_b.clone(), route_a.clone())
        .await;
    assert!(add_forwarding_rule_res.is_ok());

    let client_1_uuri = UUri {
        authority: Some(client_1_authority).into(),
        entity: Some(UEntity {
            name: "entity_1".to_string(),
            id: Some(10),
            version_major: Some(1),
            ..Default::default()
        })
        .into(),
        ..Default::default()
    };

    let client_2_uuri = UUri {
        authority: Some(client_2_authority).into(),
        entity: Some(UEntity {
            name: "entity_2".to_string(),
            id: Some(10),
            version_major: Some(1),
            ..Default::default()
        })
        .into(),
        ..Default::default()
    };

    let register_res = client_1
        .register_listener(client_2_uuri.clone(), Box::new(listener_fn))
        .await;
    let Ok(_registration_string_client_1) = register_res else {
        panic!("Unable to register!");
    };

    let register_res = client_2
        .register_listener(client_1_uuri.clone(), Box::new(listener_fn))
        .await;
    let Ok(_registration_string_client_2) = register_res else {
        panic!("Unable to register!");
    };

    let publish_from_client_1_for_client_2 = UMessage {
        attributes: Some(UAttributes {
            source: Some(client_1_uuri.clone()).into(),
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
    };

    let send_res = client_1.send(publish_from_client_1_for_client_2).await;
    assert!(send_res.is_ok());

    let request_from_client_1_for_client_2 = UMessage {
        attributes: Some(UAttributes {
            source: Some(client_1_uuri.clone()).into(),
            sink: Some(client_2_uuri.clone()).into(),
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
    };

    // let send_res = client_1.send(request_from_client_1_for_client_2).await;
    // assert!(send_res.is_ok());

    task::sleep(Duration::from_millis(3000)).await;
}
