use crate::up_client_foo::{UPClientFoo, UTransportBuilderFoo};
use async_std::channel;
use std::sync::Arc;
use up_rust::{Number, UAuthority, UMessage, UStatus};
use up_streamer::{Route, UTransportRouter};

mod up_client_foo;

pub type UtransportListener = Box<dyn Fn(Result<UMessage, UStatus>) + Send + Sync + 'static>;

#[async_std::main]
async fn main() {
    let (tx_1, rx_1) = channel::bounded(100);
    let (tx_2, rx_2) = channel::bounded(100);

    let client_1 = UPClientFoo::new(rx_1.clone(), tx_1.clone()).await;
    let client_2 = UPClientFoo::new(rx_2.clone(), tx_2.clone()).await;

    let utransport_builder_1 = UTransportBuilderFoo::new(rx_1.clone(), tx_2.clone());
    let utransport_router_handle_1 = Arc::new(
        UTransportRouter::start("foo_1".to_string(), utransport_builder_1, 100, 100).unwrap(),
    );

    let utransport_builder_2 = UTransportBuilderFoo::new(rx_2.clone(), tx_1.clone());
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
}
