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
use async_std::channel::Sender;
use async_std::sync::{Arc, Mutex};
use async_std::{channel, task};
use async_trait::async_trait;
use log::*;
use std::collections::HashMap;
use up_rust::{UAuthority, UCode, UListener, UMessage, UStatus, UTransport, UUri};

const USTREAMER_TAG: &str = "UStreamer:";
const USTREAMER_FN_NEW_TAG: &str = "new():";
const USTREAMER_FN_ADD_FORWARDING_RULE_TAG: &str = "add_forwarding_rule():";
const USTREAMER_FN_DELETE_FORWARDING_RULE_TAG: &str = "delete_forwarding_rule():";

type ForwardingListenersMap = Arc<Mutex<HashMap<(UAuthority, UAuthority), Arc<dyn UListener>>>>;

/// A [`UStreamer`] is used to coordinate the addition and deletion of forwarding rules between
/// [`Route`][crate::Route]s
///
/// Essentially, it's a means of setting up rules so that messages from one transport (e.g. Zenoh)
/// are bridged onto another transport (e.g. SOME/IP).
///
/// # Examples
///
/// ## Typical usage
/// ```
/// use std::sync::Arc;
/// use async_std::sync::Mutex;
/// use up_rust::{Number, UAuthority, UListener, UTransport};
/// use up_streamer::{Route, UStreamer};
/// # pub mod up_client_foo {
/// #     use std::sync::Arc;
/// use async_trait::async_trait;
/// #     use up_rust::{UListener, UMessage, UStatus, UUIDBuilder, UUri};
/// #     use up_rust::UTransport;
/// #
/// #     pub struct UPClientFoo;
/// #
/// #     #[async_trait]
/// #     impl UTransport for UPClientFoo {
/// #         async fn send(&self, _message: UMessage) -> Result<(), UStatus> {
/// #             todo!()
/// #         }
/// #
/// #         async fn receive(&self, _topic: UUri) -> Result<UMessage, UStatus> {
/// #             todo!()
/// #         }
/// #
/// #         async fn register_listener(
/// #             &self,
/// #             topic: UUri,
/// #             _listener: Arc<dyn UListener>,
/// #         ) -> Result<(), UStatus> {
/// #             println!("UPClientFoo: registering topic: {:?}", topic);
/// #             Ok(())
/// #         }
/// #
/// #         async fn unregister_listener(&self, topic: UUri, _listener: Arc<dyn UListener>) -> Result<(), UStatus> {
/// #             println!(
/// #                 "UPClientFoo: unregistering topic: {topic:?}"
/// #             );
/// #             Ok(())
/// #         }
/// #     }
/// #
/// #     impl UPClientFoo {
/// #         pub fn new() -> Self {
/// #             Self {}
/// #         }
/// #     }
/// # }
/// #
/// # pub mod up_client_bar {
/// #     use std::sync::Arc;
/// #     use async_trait::async_trait;
/// #     use up_rust::{UListener, UMessage, UStatus, UTransport, UUIDBuilder, UUri};
/// #     pub struct UPClientBar;
/// #
/// #     #[async_trait]
/// #     impl UTransport for UPClientBar {
/// #         async fn send(&self, _message: UMessage) -> Result<(), UStatus> {
/// #             todo!()
/// #         }
/// #
/// #         async fn receive(&self, _topic: UUri) -> Result<UMessage, UStatus> {
/// #             todo!()
/// #         }
/// #
/// #         async fn register_listener(
/// #             &self,
/// #             topic: UUri,
/// #             _listener: Arc<dyn UListener>,
/// #         ) -> Result<(), UStatus> {
/// #             println!("UPClientBar: registering topic: {:?}", topic);
/// #             Ok(())
/// #         }
/// #
/// #         async fn unregister_listener(&self, topic: UUri, _listener: Arc<dyn UListener>) -> Result<(), UStatus> {
/// #             println!(
/// #                 "UPClientFoo: unregistering topic: {topic:?}"
/// #             );
/// #             Ok(())
/// #         }
/// #     }
/// #
/// #     impl UPClientBar {
/// #         pub fn new() -> Self {
/// #             Self {}
/// #         }
/// #     }
/// # }
/// #
/// # async fn async_main() {
///
/// // Local transport
/// let local_transport: Arc<Mutex<Box<dyn UTransport>>> = Arc::new(Mutex::new(Box::new(up_client_foo::UPClientFoo::new())));
///
/// // Remote transport router
/// let remote_transport: Arc<Mutex<Box<dyn UTransport>>> = Arc::new(Mutex::new(Box::new(up_client_bar::UPClientBar::new())));
///
/// // Local route
/// let local_authority = UAuthority {
///     name: Some("local".to_string()),
///     number: Some(Number::Ip(vec![192, 168, 1, 100])),
///     ..Default::default()
/// };
/// let local_route = Route::new("local_route", local_authority, local_transport.clone());
///
/// // A remote route
/// let remote_authority = UAuthority {
///     name: Some("remote".to_string()),
///     number: Some(Number::Ip(vec![192, 168, 1, 200])),
///     ..Default::default()
/// };
/// let remote_route = Route::new("remote_route", remote_authority, remote_transport.clone());
///
/// let streamer = UStreamer::new("hoge", 100, 2);
///
/// // Add forwarding rules to route local<->remote
/// assert_eq!(
///     streamer
///         .add_forwarding_rule(local_route.clone(), remote_route.clone())
///         .await,
///     Ok(())
/// );
/// assert_eq!(
///     streamer
///         .add_forwarding_rule(remote_route.clone(), local_route.clone())
///         .await,
///     Ok(())
/// );
///
/// // Add forwarding rules to route local<->local, should report an error
/// assert!(streamer
///     .add_forwarding_rule(local_route.clone(), local_route.clone())
///     .await
///     .is_err());
///
/// // Rule already exists so it should report an error
/// assert!(streamer
///     .add_forwarding_rule(local_route.clone(), remote_route.clone())
///     .await
///     .is_err());
///
/// // Try and remove an invalid rule
/// assert!(streamer
///     .delete_forwarding_rule(remote_route.clone(), remote_route.clone())
///     .await
///     .is_err());
///
/// // remove valid routing rules
/// assert_eq!(
///     streamer
///         .delete_forwarding_rule(local_route.clone(), remote_route.clone())
///         .await,
///     Ok(())
/// );
/// assert_eq!(
///     streamer
///         .delete_forwarding_rule(remote_route.clone(), local_route.clone())
///         .await,
///     Ok(())
/// );
///
/// // Try and remove a rule that doesn't exist, should report an error
/// assert!(streamer
///     .delete_forwarding_rule(local_route.clone(), remote_route.clone())
///     .await
///     .is_err());
/// # }
/// ```
#[derive(Clone)]
pub struct UStreamer {
    #[allow(dead_code)]
    name: String,
    forwarding_listeners: ForwardingListenersMap,
    message_queue_size: usize,
    worker_pool_size: usize,
}

impl UStreamer {
    pub fn new(name: &str, message_queue_size: usize, worker_pool_size: usize) -> Self {
        let name = format!("{USTREAMER_TAG}:{name}:");
        // Try to initiate logging.
        // Required in case of dynamic lib, otherwise no logs.
        // But cannot be done twice in case of static link.
        let _ = env_logger::try_init();
        debug!(
            "{}:{}:{} UStreamer created",
            &name, USTREAMER_TAG, USTREAMER_FN_NEW_TAG
        );

        Self {
            name: name.to_string(),
            forwarding_listeners: Arc::new(Mutex::new(HashMap::new())),
            message_queue_size,
            worker_pool_size,
        }
    }

    fn uauthority_to_uuri(authority: UAuthority) -> UUri {
        UUri {
            authority: Some(authority).into(),
            ..Default::default()
        }
    }

    #[inline(always)]
    fn forwarding_id(r#in: &Route, out: &Route) -> String {
        format!(
            "[in.name: {}, in.authority: {:?} ; out.name: {}, out.authority: {:?}]",
            r#in.name, r#in.authority, out.name, out.authority
        )
    }

    #[inline(always)]
    fn fail_due_to_same_authority(&self, r#in: &Route, out: &Route) -> Result<(), UStatus> {
        let err = Err(UStatus::fail_with_code(
            UCode::INVALID_ARGUMENT,
            format!(
                "{} are the same. Unable to delete.",
                Self::forwarding_id(r#in, out)
            ),
        ));
        error!(
            "{}:{}:{} Deleting forwarding rule failed: {:?}",
            self.name, USTREAMER_TAG, USTREAMER_FN_ADD_FORWARDING_RULE_TAG, err
        );
        err
    }

    /// Adds a forwarding rule to the [`UStreamer`] based on an in [`Route`][crate::Route] and an
    /// out [`Route`][crate::Route]
    ///
    /// Works for any [`UMessage`][up_rust::UMessage] type which has a destination / sink contained
    /// in its attributes, i.e.
    /// * [`UMessageType::UMESSAGE_TYPE_NOTIFICATION`][up_rust::UMessageType::UMESSAGE_TYPE_NOTIFICATION]
    /// * [`UMessageType::UMESSAGE_TYPE_REQUEST`][up_rust::UMessageType::UMESSAGE_TYPE_REQUEST]
    /// * [`UMessageType::UMESSAGE_TYPE_RESPONSE`][up_rust::UMessageType::UMESSAGE_TYPE_RESPONSE]
    ///
    /// # Parameters
    ///
    /// * `in` - [`Route`][crate::Route] we will bridge _from_
    /// * `out` - [`Route`][crate::Route] we will bridge _onto_
    ///
    /// # Errors
    ///
    /// If unable to add this forwarding rule, we return a [`UStatus`][up_rust::UStatus] noting
    /// the error.
    ///
    /// Typical errors include
    /// * already have this forwarding rule registered
    /// * attempting to forward onto the same [`Route`][crate::Route]
    pub async fn add_forwarding_rule(&self, r#in: Route, out: Route) -> Result<(), UStatus> {
        debug!(
            "{}:{}:{} Adding forwarding rule for {}",
            self.name,
            USTREAMER_TAG,
            USTREAMER_FN_ADD_FORWARDING_RULE_TAG,
            Self::forwarding_id(&r#in, &out)
        );

        if r#in.authority == out.authority {
            return self.fail_due_to_same_authority(&r#in, &out);
        }

        // TODO: Make message_queue_size and worker_pool_size configurable
        let forwarding_listener: Arc<dyn UListener> = Arc::new(
            ForwardingListener::new(
                &Self::forwarding_id(&r#in, &out),
                out.transport.clone(),
                self.message_queue_size,
                self.worker_pool_size,
            )
            .await,
        );

        let mut forwarding_listeners = self.forwarding_listeners.lock().await;
        if let Some(_exists) = forwarding_listeners.insert(
            (r#in.authority.clone(), out.authority.clone()),
            forwarding_listener.clone(),
        ) {
            let err = Err(UStatus::fail_with_code(
                UCode::ALREADY_EXISTS,
                format!(
                    "{} are already routed. Unable to add same rule twice.",
                    Self::forwarding_id(&r#in, &out)
                ),
            ));
            error!(
                "{}:{}:{} Adding forwarding rule failed: {:?}",
                self.name, USTREAMER_TAG, USTREAMER_FN_ADD_FORWARDING_RULE_TAG, err
            );
            err
        } else {
            r#in.transport
                .lock()
                .await
                .register_listener(Self::uauthority_to_uuri(out.authority), forwarding_listener)
                .await
        }
    }

    /// Deletes a forwarding rule from the [`UStreamer`] based on an in [`Route`][crate::Route] and an
    /// out [`Route`][crate::Route]
    ///
    /// Works for any [`UMessage`][up_rust::UMessage] type which has a destination / sink contained
    /// in its attributes, i.e.
    /// * [`UMessageType::UMESSAGE_TYPE_NOTIFICATION`][up_rust::UMessageType::UMESSAGE_TYPE_NOTIFICATION]
    /// * [`UMessageType::UMESSAGE_TYPE_REQUEST`][up_rust::UMessageType::UMESSAGE_TYPE_REQUEST]
    /// * [`UMessageType::UMESSAGE_TYPE_RESPONSE`][up_rust::UMessageType::UMESSAGE_TYPE_RESPONSE]
    ///
    /// # Parameters
    ///
    /// * `in` - [`Route`][crate::Route] we will bridge _from_
    /// * `out` - [`Route`][crate::Route] we will bridge _onto_
    ///
    /// # Errors
    ///
    /// If unable to delete this forwarding rule, we return a [`UStatus`][up_rust::UStatus] noting
    /// the error.
    ///
    /// Typical errors include
    /// * No such route has been added
    /// * attempting to delete a forwarding rule where we would forward onto the same [`Route`][crate::Route]
    pub async fn delete_forwarding_rule(&self, r#in: Route, out: Route) -> Result<(), UStatus> {
        debug!(
            "{}:{}:{} Deleting forwarding rule for {}",
            self.name,
            USTREAMER_TAG,
            USTREAMER_FN_DELETE_FORWARDING_RULE_TAG,
            Self::forwarding_id(&r#in, &out)
        );

        if r#in.authority == out.authority {
            return self.fail_due_to_same_authority(&r#in, &out);
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
            let err = Err(UStatus::fail_with_code(
                UCode::NOT_FOUND,
                format!(
                    "{} are not routed. No rule to delete.",
                    Self::forwarding_id(&r#in, &out)
                ),
            ));
            error!(
                "{}:{}:{} Adding forwarding rule failed: {:?}",
                self.name, USTREAMER_TAG, USTREAMER_FN_ADD_FORWARDING_RULE_TAG, err
            );
            err
        }
    }
}

const FORWARDING_LISTENER_TAG: &str = "ForwardingListener:";
const FORWARDING_LISTENER_FN_ON_RECEIVE_TAG: &str = "on_receive():";
const FORWARDING_LISTENER_FN_ON_ERROR_TAG: &str = "on_error():";

#[derive(Clone)]
pub(crate) struct ForwardingListener {
    forwarding_id: String,
    sender: Sender<UMessage>,
}

impl ForwardingListener {
    pub(crate) async fn new(
        forwarding_id: &str,
        out_transport: Arc<Mutex<Box<dyn UTransport>>>,
        message_queue_size: usize,
        worker_pool_size: usize,
    ) -> Self {
        let (sender, receiver) = channel::bounded(message_queue_size);

        for _ in 0..worker_pool_size {
            let receiver: channel::Receiver<UMessage> = receiver.clone();
            let out_transport = out_transport.clone();
            let forwarding_id_clone = forwarding_id.to_string();

            task::spawn(async move {
                while let Ok(msg) = receiver.recv().await {
                    let Some(attr) = msg.attributes.as_ref() else {
                        error!(
                            "{}:{}:{} Unable to forward message, missing attributes: {:?}",
                            forwarding_id_clone,
                            FORWARDING_LISTENER_TAG,
                            FORWARDING_LISTENER_FN_ON_RECEIVE_TAG,
                            msg
                        );
                        return;
                    };

                    let Some(id) = attr.id.as_ref() else {
                        error!(
                            "{}:{}:{} Unable to forward message, missing id: {:?}",
                            forwarding_id_clone,
                            FORWARDING_LISTENER_TAG,
                            FORWARDING_LISTENER_FN_ON_RECEIVE_TAG,
                            msg
                        );
                        return;
                    };
                    let msg_id = id.clone();

                    let out_transport_guard = out_transport.lock().await;
                    let send_res = out_transport_guard.send(msg).await;
                    match send_res {
                        Ok(_) => {
                            debug!(
                                "{}:{}:{} Succeeded sending message id: {}",
                                forwarding_id_clone,
                                FORWARDING_LISTENER_TAG,
                                FORWARDING_LISTENER_FN_ON_RECEIVE_TAG,
                                msg_id
                            );
                        }
                        Err(err) => {
                            error!(
                                "{}:{}:{} Failed sending message id: {} with error: {err:?}",
                                forwarding_id_clone,
                                FORWARDING_LISTENER_TAG,
                                FORWARDING_LISTENER_FN_ON_RECEIVE_TAG,
                                msg_id
                            );
                        }
                    }
                }
            });
        }

        Self {
            forwarding_id: forwarding_id.to_string(),
            sender,
        }
    }
}

#[async_trait]
impl UListener for ForwardingListener {
    async fn on_receive(&self, msg: UMessage) {
        debug!(
            "{}:{}:{} Received message: {:?}",
            self.forwarding_id,
            FORWARDING_LISTENER_TAG,
            FORWARDING_LISTENER_FN_ON_RECEIVE_TAG,
            &msg
        );
        if let Err(e) = self.sender.send(msg).await {
            error!(
                "{}:{}:{} Unable to send message to worker pool: {e:?}",
                self.forwarding_id, FORWARDING_LISTENER_TAG, FORWARDING_LISTENER_FN_ON_RECEIVE_TAG,
            );
        }
    }

    async fn on_error(&self, err: UStatus) {
        error!(
            "{}:{}:{} Received error instead of message from UTransport, with error: {err:?}",
            self.forwarding_id, FORWARDING_LISTENER_TAG, FORWARDING_LISTENER_FN_ON_ERROR_TAG
        );
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
            _listener: Arc<dyn UListener>,
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
            _listener: Arc<dyn UListener>,
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
        let local_route = Route::new(
            "local_route",
            local_authority.clone(),
            local_transport.clone(),
        );

        // A remote route
        let remote_authority = UAuthority {
            name: Some("remote".to_string()),
            number: Some(Number::Ip(vec![192, 168, 1, 200])),
            ..Default::default()
        };
        let remote_transport: Arc<Mutex<Box<dyn UTransport>>> =
            Arc::new(Mutex::new(Box::new(UPClientBar)));
        let remote_route = Route::new(
            "remote_route",
            remote_authority.clone(),
            remote_transport.clone(),
        );

        let ustreamer = UStreamer::new("foo_bar_streamer", 100, 2);
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
        let local_route = Route::new(
            "local_route",
            local_authority.clone(),
            local_transport.clone(),
        );

        // Remote route - A
        let remote_authority_a = UAuthority {
            name: Some("remote_a".to_string()),
            number: Some(Number::Ip(vec![192, 168, 1, 200])),
            ..Default::default()
        };
        let remote_transport_a: Arc<Mutex<Box<dyn UTransport>>> =
            Arc::new(Mutex::new(Box::new(UPClientBar)));
        let remote_route_a = Route::new(
            "remote_route_a",
            remote_authority_a.clone(),
            remote_transport_a.clone(),
        );

        // Remote route - B
        let remote_authority_b = UAuthority {
            name: Some("remote_b".to_string()),
            number: Some(Number::Ip(vec![192, 168, 1, 201])),
            ..Default::default()
        };
        let remote_transport_b: Arc<Mutex<Box<dyn UTransport>>> =
            Arc::new(Mutex::new(Box::new(UPClientBar)));
        let remote_route_b = Route::new(
            "remote_route_b",
            remote_authority_b.clone(),
            remote_transport_b.clone(),
        );

        let ustreamer = UStreamer::new("foo_bar_streamer", 100, 2);

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
        let local_route = Route::new(
            "local_route",
            local_authority.clone(),
            local_transport.clone(),
        );

        let remote_transport: Arc<Mutex<Box<dyn UTransport>>> =
            Arc::new(Mutex::new(Box::new(UPClientBar)));

        // Remote route - A
        let remote_authority_a = UAuthority {
            name: Some("remote_a".to_string()),
            number: Some(Number::Ip(vec![192, 168, 1, 200])),
            ..Default::default()
        };
        let remote_route_a = Route::new(
            "remote_route_a",
            remote_authority_a.clone(),
            remote_transport.clone(),
        );

        // Remote route - B
        let remote_authority_b = UAuthority {
            name: Some("remote_b".to_string()),
            number: Some(Number::Ip(vec![192, 168, 1, 201])),
            ..Default::default()
        };
        let remote_route_b = Route::new(
            "remote_route_b",
            remote_authority_b.clone(),
            remote_transport.clone(),
        );

        let ustreamer = UStreamer::new("foo_bar_streamer", 100, 2);

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
