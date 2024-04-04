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

use crate::route::Route;
use async_std::sync::{Arc, Mutex};
use async_trait::async_trait;
use log::*;
use std::collections::HashMap;
use up_rust::{UAuthority, UCode, UListener, UMessage, UStatus, UTransport, UUri};

const USTREAMER_TAG: &str = "UStreamer:";
const _USTREAMER_FN_ADD_FORWARDING_RULE_TAG: &str = "add_forwarding_rule():";
const _USTREAMER_FN_DELETE_FORWARDING_RULE_TAG: &str = "delete_forwarding_rule():";

type ForwardingListenersMap = Arc<Mutex<HashMap<(UAuthority, UAuthority), Arc<dyn UListener>>>>;
#[derive(Clone)]
pub struct UStreamer {
    #[allow(dead_code)]
    name: String,
    forwarding_listeners: ForwardingListenersMap,
}

impl UStreamer {
    pub fn new(name: &str) -> Self {
        let name = format!("{USTREAMER_TAG}:{name}:");
        // Try to initiate logging.
        // Required in case of dynamic lib, otherwise no logs.
        // But cannot be done twice in case of static link.
        let _ = env_logger::try_init();
        debug!("{}: UStreamer started", &name);

        Self {
            name: name.to_string(),
            forwarding_listeners: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn uauthority_to_uuri(authority: UAuthority) -> UUri {
        UUri {
            authority: Some(authority).into(),
            ..Default::default()
        }
    }

    pub async fn add_forwarding_rule(&self, r#in: Route, out: Route) -> Result<(), UStatus> {
        debug!("adding forwarding rule");

        if r#in.authority == out.authority {
            return Err(UStatus::fail_with_code(
                UCode::INVALID_ARGUMENT,
                "Unable to send onto same authority!",
            ));
        }

        let forwarding_listener: Arc<dyn UListener> =
            Arc::new(ForwardingListener::new(out.transport.clone()).await);

        let mut forwarding_listeners = self.forwarding_listeners.lock().await;
        if let Some(_exists) = forwarding_listeners.insert(
            (r#in.authority.clone(), out.authority.clone()),
            forwarding_listener.clone(),
        ) {
            Err(UStatus::fail_with_code(
                UCode::ALREADY_EXISTS,
                "Already exists",
            ))
        } else {
            r#in.transport
                .lock()
                .await
                .register_listener(
                    Self::uauthority_to_uuri(out.authority),
                    &forwarding_listener,
                )
                .await
        }
    }

    pub async fn delete_forwarding_rule(&self, r#in: Route, out: Route) -> Result<(), UStatus> {
        if r#in.authority == out.authority {
            return Err(UStatus::fail_with_code(
                UCode::INVALID_ARGUMENT,
                "Unable to send onto same authority!",
            ));
        }

        let mut forwarding_listeners = self.forwarding_listeners.lock().await;
        if let Some(exists) =
            forwarding_listeners.remove(&(r#in.authority.clone(), out.authority.clone()))
        {
            r#in.transport
                .lock()
                .await
                .unregister_listener(Self::uauthority_to_uuri(out.authority), exists)
                .await
        } else {
            Err(UStatus::fail_with_code(UCode::NOT_FOUND, "Doesn't exist"))
        }
    }
}

#[derive(Clone)]
pub(crate) struct ForwardingListener {
    out_transport: Arc<Mutex<Box<dyn UTransport>>>,
}

impl ForwardingListener {
    pub(crate) async fn new(out_transport: Arc<Mutex<Box<dyn UTransport>>>) -> Self {
        Self { out_transport }
    }
}

#[async_trait]
impl UListener for ForwardingListener {
    async fn on_receive(&self, msg: UMessage) {
        // TODO: This will currently just immediately upon receipt of the message send it back out
        //  We may want to implement some kind of queueing mechanism here explicitly to handle
        //  if we're busy still receiving but another message is available
        let out_transport = self.out_transport.lock().await;
        let _res = out_transport.send(msg).await;
    }

    async fn on_error(&self, err: UStatus) {
        error!("Unable to send over transport! {err:?}");
    }
}

#[cfg(test)]
mod tests {
    use crate::{Route, UStreamer};
    use async_std::sync::Mutex;
    use async_trait::async_trait;
    use std::sync::Arc;
    use up_rust::{Number, UAuthority, UListener, UMessage, UStatus, UTransport, UUri};

    pub struct UPClientFoo;

    #[async_trait]
    impl UTransport for UPClientFoo {
        async fn send(&self, _message: UMessage) -> Result<(), UStatus> {
            todo!()
        }

        async fn receive(&self, _topic: UUri) -> Result<UMessage, UStatus> {
            todo!()
        }

        async fn register_listener(
            &self,
            topic: UUri,
            _listener: &Arc<dyn UListener>,
        ) -> Result<(), UStatus> {
            println!("UPClientFoo: registering topic: {:?}", topic);
            Ok(())
        }

        async fn unregister_listener(
            &self,
            topic: UUri,
            _listener: Arc<dyn UListener>,
        ) -> Result<(), UStatus> {
            println!("UPClientFoo: unregistering topic: {topic:?}");
            Ok(())
        }
    }

    pub struct UPClientBar;

    #[async_trait]
    impl UTransport for UPClientBar {
        async fn send(&self, _message: UMessage) -> Result<(), UStatus> {
            todo!()
        }

        async fn receive(&self, _topic: UUri) -> Result<UMessage, UStatus> {
            todo!()
        }

        async fn register_listener(
            &self,
            topic: UUri,
            _listener: &Arc<dyn UListener>,
        ) -> Result<(), UStatus> {
            println!("UPClientBar: registering topic: {:?}", topic);
            Ok(())
        }

        async fn unregister_listener(
            &self,
            topic: UUri,
            _listener: Arc<dyn UListener>,
        ) -> Result<(), UStatus> {
            println!("UPClientBar: unregistering topic: {topic:?}");
            Ok(())
        }
    }

    #[async_std::test]
    async fn test_simple_with_a_single_input_and_output_route() {
        // Local route
        let local_authority = UAuthority {
            name: Some("local".to_string()),
            number: Some(Number::Ip(vec![192, 168, 1, 100])),
            ..Default::default()
        };
        let local_transport: Arc<Mutex<Box<dyn UTransport>>> =
            Arc::new(Mutex::new(Box::new(UPClientFoo)));
        let local_route = Route::new(local_authority.clone(), local_transport.clone());

        // A remote route
        let remote_authority = UAuthority {
            name: Some("remote".to_string()),
            number: Some(Number::Ip(vec![192, 168, 1, 200])),
            ..Default::default()
        };
        let remote_transport: Arc<Mutex<Box<dyn UTransport>>> =
            Arc::new(Mutex::new(Box::new(UPClientBar)));
        let remote_route = Route::new(remote_authority.clone(), remote_transport.clone());

        let ustreamer = UStreamer::new("foo_bar_streamer");
        // Add forwarding rules to route local<->remote
        assert!(ustreamer
            .add_forwarding_rule(local_route.clone(), remote_route.clone())
            .await
            .is_ok());
        assert!(ustreamer
            .add_forwarding_rule(remote_route.clone(), local_route.clone())
            .await
            .is_ok());

        // Add forwarding rules to route local<->local, should report an error
        assert!(ustreamer
            .add_forwarding_rule(local_route.clone(), local_route.clone())
            .await
            .is_err());

        // Rule already exists so it should report an error
        assert!(ustreamer
            .add_forwarding_rule(local_route.clone(), remote_route.clone())
            .await
            .is_err());

        // Add forwarding rules to route remote<->remote, should report an error
        assert!(ustreamer
            .add_forwarding_rule(remote_route.clone(), remote_route.clone())
            .await
            .is_err());

        // remove valid routing rules
        assert!(ustreamer
            .delete_forwarding_rule(local_route.clone(), remote_route.clone())
            .await
            .is_ok());
        assert!(ustreamer
            .delete_forwarding_rule(remote_route.clone(), local_route.clone())
            .await
            .is_ok());

        // Try and remove a rule that doesn't exist, should report an error
        assert!(ustreamer
            .delete_forwarding_rule(local_route.clone(), remote_route.clone())
            .await
            .is_err());
    }

    #[async_std::test]
    async fn test_advanced_where_there_is_a_local_route_and_two_remote_routes() {
        // Local route
        let local_authority = UAuthority {
            name: Some("local".to_string()),
            number: Some(Number::Ip(vec![192, 168, 1, 100])),
            ..Default::default()
        };
        let local_transport: Arc<Mutex<Box<dyn UTransport>>> =
            Arc::new(Mutex::new(Box::new(UPClientFoo)));
        let local_route = Route::new(local_authority.clone(), local_transport.clone());

        // Remote route - A
        let remote_authority_a = UAuthority {
            name: Some("remote_a".to_string()),
            number: Some(Number::Ip(vec![192, 168, 1, 200])),
            ..Default::default()
        };
        let remote_transport_a: Arc<Mutex<Box<dyn UTransport>>> =
            Arc::new(Mutex::new(Box::new(UPClientBar)));
        let remote_route_a = Route::new(remote_authority_a.clone(), remote_transport_a.clone());

        // Remote route - B
        let remote_authority_b = UAuthority {
            name: Some("remote_b".to_string()),
            number: Some(Number::Ip(vec![192, 168, 1, 201])),
            ..Default::default()
        };
        let remote_transport_b: Arc<Mutex<Box<dyn UTransport>>> =
            Arc::new(Mutex::new(Box::new(UPClientBar)));
        let remote_route_b = Route::new(remote_authority_b.clone(), remote_transport_b.clone());

        let ustreamer = UStreamer::new("foo_bar_streamer");

        // Add forwarding rules to route local<->remote_a
        assert!(ustreamer
            .add_forwarding_rule(local_route.clone(), remote_route_a.clone())
            .await
            .is_ok());
        assert!(ustreamer
            .add_forwarding_rule(remote_route_a.clone(), local_route.clone())
            .await
            .is_ok());

        // Add forwarding rules to route local<->remote_b
        assert!(ustreamer
            .add_forwarding_rule(local_route.clone(), remote_route_b.clone())
            .await
            .is_ok());
        assert!(ustreamer
            .add_forwarding_rule(remote_route_b.clone(), local_route.clone())
            .await
            .is_ok());

        // Add forwarding rules to route remote_a<->remote_b
        assert!(ustreamer
            .add_forwarding_rule(remote_route_a.clone(), remote_route_b.clone())
            .await
            .is_ok());
        assert!(ustreamer
            .add_forwarding_rule(remote_route_b.clone(), remote_route_a.clone())
            .await
            .is_ok());
    }

    #[async_std::test]
    async fn test_advanced_where_there_is_a_local_route_and_two_remote_routes_but_the_remote_routes_have_the_same_instance_of_utransport(
    ) {
        // Local route
        let local_authority = UAuthority {
            name: Some("local".to_string()),
            number: Some(Number::Ip(vec![192, 168, 1, 100])),
            ..Default::default()
        };
        let local_transport: Arc<Mutex<Box<dyn UTransport>>> =
            Arc::new(Mutex::new(Box::new(UPClientFoo)));
        let local_route = Route::new(local_authority.clone(), local_transport.clone());

        let remote_transport: Arc<Mutex<Box<dyn UTransport>>> =
            Arc::new(Mutex::new(Box::new(UPClientBar)));

        // Remote route - A
        let remote_authority_a = UAuthority {
            name: Some("remote_a".to_string()),
            number: Some(Number::Ip(vec![192, 168, 1, 200])),
            ..Default::default()
        };
        let remote_route_a = Route::new(remote_authority_a.clone(), remote_transport.clone());

        // Remote route - B
        let remote_authority_b = UAuthority {
            name: Some("remote_b".to_string()),
            number: Some(Number::Ip(vec![192, 168, 1, 201])),
            ..Default::default()
        };
        let remote_route_b = Route::new(remote_authority_b.clone(), remote_transport.clone());

        let ustreamer = UStreamer::new("foo_bar_streamer");

        // Add forwarding rules to route local<->remote_a
        assert!(ustreamer
            .add_forwarding_rule(local_route.clone(), remote_route_a.clone())
            .await
            .is_ok());
        assert!(ustreamer
            .add_forwarding_rule(remote_route_a.clone(), local_route.clone())
            .await
            .is_ok());

        // Add forwarding rules to route local<->remote_b
        assert!(ustreamer
            .add_forwarding_rule(local_route.clone(), remote_route_b.clone())
            .await
            .is_ok());
        assert!(ustreamer
            .add_forwarding_rule(remote_route_b.clone(), local_route.clone())
            .await
            .is_ok());
    }
}
