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
use log::*;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use up_rust::UStatus;

const USTREAMER_TAG: &str = "UStreamer:";
const USTREAMER_FN_ADD_FORWARDING_RULE_TAG: &str = "add_forwarding_rule():";
const USTREAMER_FN_DELETE_FORWARDING_RULE_TAG: &str = "delete_forwarding_rule():";

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
/// use up_rust::{Number, UAuthority};
/// use up_streamer::{Route, UStreamer, UTransportRouter};
/// # pub mod utransport_builder_foo {
/// #     use async_trait::async_trait;
/// #     use up_rust::{UMessage, UStatus, UUIDBuilder, UUri};
/// #     use up_rust::UTransport;
/// #     use up_streamer::UTransportBuilder;
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
/// #             _listener: Box<dyn Fn(Result<UMessage, UStatus>) + Send + Sync + 'static>,
/// #         ) -> Result<String, UStatus> {
/// #             println!("UPClientFoo: registering topic: {:?}", topic);
/// #             let uuid = UUIDBuilder::new().build();
/// #             Ok(uuid.to_string())
/// #         }
/// #
/// #         async fn unregister_listener(&self, topic: UUri, listener: &str) -> Result<(), UStatus> {
/// #             println!(
/// #                 "UPClientFoo: unregistering topic: {topic:?} with listener string: {listener}"
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
/// #     pub struct UTransportBuilderFoo;
/// #     impl UTransportBuilder for UTransportBuilderFoo {
/// #         fn build(&self) -> Result<Box<dyn UTransport>, UStatus> {
/// #             let utransport_foo: Box<dyn UTransport> = Box::new(UPClientFoo::new());
/// #             Ok(utransport_foo)
/// #         }
/// #     }
/// #
/// #     impl UTransportBuilderFoo {
/// #         pub fn new() -> Self {
/// #             Self {}
/// #         }
/// #     }
/// # }
/// #
/// # pub mod utransport_builder_bar {
/// #     use async_trait::async_trait;
/// #     use up_rust::{UMessage, UStatus, UTransport, UUIDBuilder, UUri};use up_streamer::UTransportBuilder;
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
/// #             _listener: Box<dyn Fn(Result<UMessage, UStatus>) + Send + Sync + 'static>,
/// #         ) -> Result<String, UStatus> {
/// #             println!("UPClientBar: registering topic: {:?}", topic);
/// #             let uuid = UUIDBuilder::new().build();
/// #             Ok(uuid.to_string())
/// #         }
/// #
/// #         async fn unregister_listener(&self, topic: UUri, listener: &str) -> Result<(), UStatus> {
/// #             println!(
/// #                 "UPClientFoo: unregistering topic: {topic:?} with listener string: {listener}"
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
/// #
/// #     pub struct UTransportBuilderBar;
/// #     impl UTransportBuilder for UTransportBuilderBar {
/// #         fn build(&self) -> Result<Box<dyn UTransport>, UStatus> {
/// #             let utransport_foo: Box<dyn UTransport> = Box::new(UPClientBar::new());
/// #             Ok(utransport_foo)
/// #         }
/// #     }
/// #
/// #     impl UTransportBuilderBar {
/// #         pub fn new() -> Self {
/// #             Self {}
/// #         }
/// #     }
/// # }
/// #
/// # async fn async_main() {
///
/// // Local transport router
/// let local_transport_router =
///     UTransportRouter::start("FOO".to_string(), utransport_builder_foo::UTransportBuilderFoo::new(), 100, 200, 300);
/// assert!(local_transport_router.is_ok());
/// let local_transport_router_handle = Arc::new(local_transport_router.unwrap());
///
/// // Remote transport router
/// let remote_transport_router =
///     UTransportRouter::start("BAR".to_string(), utransport_builder_bar::UTransportBuilderBar::new(), 100, 200, 300);
/// assert!(remote_transport_router.is_ok());
/// let remote_transport_router_handle = Arc::new(remote_transport_router.unwrap());
///
/// // Local route
/// let local_authority = UAuthority {
///     name: Some("local".to_string()),
///     number: Some(Number::Ip(vec![192, 168, 1, 100])),
///     ..Default::default()
/// };
/// let local_route = Route::new(&local_authority, &local_transport_router_handle);
///
/// // A remote route
/// let remote_authority = UAuthority {
///     name: Some("remote".to_string()),
///     number: Some(Number::Ip(vec![192, 168, 1, 200])),
///     ..Default::default()
/// };
/// let remote_route = Route::new(&remote_authority, &remote_transport_router_handle);
///
/// let streamer = UStreamer::new("hoge");
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
        }
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
        let out_message_sender = &out.get_transport_router_handle().clone().message_sender;

        if log_enabled!(Level::Debug) {
            let mut hasher = DefaultHasher::new();
            out_message_sender.hash(&mut hasher);
            let hash = hasher.finish();
            debug!(
                "{}:{} adding forwarding rule: ({:?}, {:?}, {})",
                &self.name,
                &USTREAMER_FN_ADD_FORWARDING_RULE_TAG,
                &r#in.get_authority(),
                &out.get_authority(),
                hash
            );
        }

        r#in.get_transport_router_handle()
            .register(
                r#in.get_authority(),
                out.get_authority(),
                out_message_sender.clone(),
            )
            .await
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
        let out_message_sender = &out.get_transport_router_handle().clone().message_sender;

        if log_enabled!(Level::Debug) {
            let mut hasher = DefaultHasher::new();
            out_message_sender.hash(&mut hasher);
            let hash = hasher.finish();
            debug!(
                "{}:{} deleting forwarding rule: ({:?}, {:?}, {})",
                &self.name,
                &USTREAMER_FN_DELETE_FORWARDING_RULE_TAG,
                &r#in.get_authority(),
                &out.get_authority(),
                hash
            );
        }

        r#in.get_transport_router_handle()
            .unregister(
                r#in.get_authority(),
                out.get_authority(),
                out_message_sender.clone(),
            )
            .await
    }
}

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use crate::route::Route;
    #[allow(unused_imports)]
    use crate::ustreamer::UStreamer;
    #[allow(unused_imports)]
    use crate::utransport_builder::UTransportBuilder;
    #[allow(unused_imports)]
    use crate::utransport_router::UTransportRouter;
    #[allow(unused_imports)]
    use async_trait::async_trait;
    #[allow(unused_imports)]
    use std::sync::Arc;
    use up_rust::UCode;
    #[allow(unused_imports)]
    use up_rust::{Number, UAuthority, UMessage, UStatus, UTransport, UUIDBuilder, UUri};

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
            _listener: Box<dyn Fn(Result<UMessage, UStatus>) + Send + Sync + 'static>,
        ) -> Result<String, UStatus> {
            println!("UPClientFoo: registering topic: {:?}", topic);
            let uuid = UUIDBuilder::new().build();
            Ok(uuid.to_string())
        }

        async fn unregister_listener(&self, topic: UUri, listener: &str) -> Result<(), UStatus> {
            println!(
                "UPClientFoo: unregistering topic: {topic:?} with listener string: {listener}"
            );
            Ok(())
        }
    }

    impl UPClientFoo {
        pub fn new() -> Self {
            Self {}
        }
    }

    pub struct UTransportBuilderFoo {
        succeed: bool,
    }

    impl UTransportBuilder for UTransportBuilderFoo {
        fn build(&self) -> Result<Box<dyn UTransport>, UStatus> {
            if self.succeed {
                let utransport_foo: Box<dyn UTransport> = Box::new(UPClientFoo::new());
                Ok(utransport_foo)
            } else {
                Err(UStatus::fail_with_code(
                    UCode::INTERNAL,
                    "Failed to build UPClientFoo",
                ))
            }
        }
    }

    impl UTransportBuilderFoo {
        pub fn new(succeed: bool) -> Self {
            Self { succeed }
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
            _listener: Box<dyn Fn(Result<UMessage, UStatus>) + Send + Sync + 'static>,
        ) -> Result<String, UStatus> {
            println!("UPClientBar: registering topic: {:?}", topic);
            let uuid = UUIDBuilder::new().build();
            Ok(uuid.to_string())
        }

        async fn unregister_listener(&self, topic: UUri, listener: &str) -> Result<(), UStatus> {
            println!(
                "UPClientFoo: unregistering topic: {topic:?} with listener string: {listener}"
            );
            Ok(())
        }
    }

    impl UPClientBar {
        pub fn new() -> Self {
            Self {}
        }
    }

    pub struct UTransportBuilderBar;

    impl UTransportBuilder for UTransportBuilderBar {
        fn build(&self) -> Result<Box<dyn UTransport>, UStatus> {
            let utransport_foo: Box<dyn UTransport> = Box::new(UPClientBar::new());
            Ok(utransport_foo)
        }
    }

    impl UTransportBuilderBar {
        pub fn new() -> Self {
            Self {}
        }
    }

    /// This is a simple test where we have a single input and output route.
    /// We also test the add_forwarding_rule() and delete_forwarding_rule() methods.
    #[async_std::test]
    async fn test_simple_with_a_single_input_and_output_route() {
        // Local transport router
        let local_transport_router = UTransportRouter::start(
            "FOO".to_string(),
            UTransportBuilderFoo::new(true),
            100,
            200,
            300,
        );
        assert!(local_transport_router.is_ok());
        let local_transport_router_handle = Arc::new(local_transport_router.unwrap());

        // Remote transport router
        let remote_transport_router = UTransportRouter::start(
            "BAR".to_string(),
            UTransportBuilderBar::new(),
            100,
            200,
            300,
        );
        assert!(remote_transport_router.is_ok());
        let remote_transport_router_handle = Arc::new(remote_transport_router.unwrap());

        // Local route
        let local_authority = UAuthority {
            name: Some("local".to_string()),
            number: Some(Number::Ip(vec![192, 168, 1, 100])),
            ..Default::default()
        };
        let local_route = Route::new(&local_authority, &local_transport_router_handle);

        // A remote route
        let remote_authority = UAuthority {
            name: Some("remote".to_string()),
            number: Some(Number::Ip(vec![192, 168, 1, 200])),
            ..Default::default()
        };
        let remote_route = Route::new(&remote_authority, &remote_transport_router_handle);

        let streamer = UStreamer::new("hoge");

        // Add forwarding rules to route local<->remote
        assert_eq!(
            streamer
                .add_forwarding_rule(local_route.clone(), remote_route.clone())
                .await,
            Ok(())
        );
        assert_eq!(
            streamer
                .add_forwarding_rule(remote_route.clone(), local_route.clone())
                .await,
            Ok(())
        );

        // Add forwarding rules to route local<->local, should report an error
        assert!(streamer
            .add_forwarding_rule(local_route.clone(), local_route.clone())
            .await
            .is_err());

        // Rule already exists so it should report an error
        assert!(streamer
            .add_forwarding_rule(local_route.clone(), remote_route.clone())
            .await
            .is_err());

        // Try and remove an invalid rule
        assert!(streamer
            .delete_forwarding_rule(remote_route.clone(), remote_route.clone())
            .await
            .is_err());

        // remove valid routing rules
        assert_eq!(
            streamer
                .delete_forwarding_rule(local_route.clone(), remote_route.clone())
                .await,
            Ok(())
        );
        assert_eq!(
            streamer
                .delete_forwarding_rule(remote_route.clone(), local_route.clone())
                .await,
            Ok(())
        );

        // Try and remove a rule that doesn't exist, should report an error
        assert!(streamer
            .delete_forwarding_rule(local_route.clone(), remote_route.clone())
            .await
            .is_err());
    }

    /// This is an example where we need to set up multiple routes to different destinations.
    #[async_std::test]
    async fn test_advanced_where_there_is_a_local_route_and_two_remote_routes() {
        // Local transport router
        let local_transport_router = UTransportRouter::start(
            "FOO".to_string(),
            UTransportBuilderFoo::new(true),
            100,
            200,
            300,
        );
        assert!(local_transport_router.is_ok());
        let local_transport_router_handle = Arc::new(local_transport_router.unwrap());

        // First remote transport router
        let remote_transport_router_1 = UTransportRouter::start(
            "BAR".to_string(),
            UTransportBuilderBar::new(),
            100,
            200,
            300,
        );
        assert!(remote_transport_router_1.is_ok());
        let remote_transport_router_handle_1 = Arc::new(remote_transport_router_1.unwrap());

        // Second remote transport router
        let remote_transport_router_2 = UTransportRouter::start(
            "BAR".to_string(),
            UTransportBuilderBar::new(),
            100,
            200,
            300,
        );
        assert!(remote_transport_router_2.is_ok());
        let remote_transport_router_handle_2 = Arc::new(remote_transport_router_2.unwrap());

        // Local route
        let local_authority = UAuthority {
            name: Some("local".to_string()),
            number: Some(Number::Ip(vec![192, 168, 1, 100])),
            ..Default::default()
        };
        let local_route = Route::new(&local_authority, &local_transport_router_handle);

        // A first remote route
        let remote_authority_1 = UAuthority {
            name: Some("remote_1".to_string()),
            number: Some(Number::Ip(vec![192, 168, 1, 200])),
            ..Default::default()
        };
        let remote_route_1 = Route::new(&remote_authority_1, &remote_transport_router_handle_1);

        // A second remote route
        let remote_authority_2 = UAuthority {
            name: Some("remote_2".to_string()),
            number: Some(Number::Ip(vec![192, 168, 1, 201])),
            ..Default::default()
        };
        let remote_route_2 = Route::new(&remote_authority_2, &remote_transport_router_handle_2);

        let streamer = UStreamer::new("hoge");

        // Add forwarding rules to route local_route<->remote_route_1
        assert_eq!(
            streamer
                .add_forwarding_rule(local_route.clone(), remote_route_1.clone())
                .await,
            Ok(())
        );
        assert_eq!(
            streamer
                .add_forwarding_rule(remote_route_1.clone(), local_route.clone())
                .await,
            Ok(())
        );

        // Add forwarding rules to route local_route<->remote_route_2
        assert_eq!(
            streamer
                .add_forwarding_rule(local_route.clone(), remote_route_2.clone())
                .await,
            Ok(())
        );
        assert_eq!(
            streamer
                .add_forwarding_rule(remote_route_2.clone(), local_route.clone())
                .await,
            Ok(())
        );
    }

    /// This is an example where we need to set up multiple routes to different destinations but using the same
    /// remote UTransport (i.e. connecting to multiple remote servers using the same UTransport instance).
    #[async_std::test]
    async fn test_advanced_where_there_is_an_local_route_and_two_remote_routes_but_the_remote_routes_have_the_same_instance_of_utransport(
    ) {
        // Local transport router
        let local_transport_router = UTransportRouter::start(
            "FOO".to_string(),
            UTransportBuilderFoo::new(true),
            100,
            200,
            300,
        );
        assert!(local_transport_router.is_ok());
        let local_transport_router_handle = Arc::new(local_transport_router.unwrap());

        // Remote transport router
        let remote_transport_router = UTransportRouter::start(
            "BAR".to_string(),
            UTransportBuilderBar::new(),
            100,
            200,
            300,
        );
        assert!(remote_transport_router.is_ok());
        let remote_transport_router_handle = Arc::new(remote_transport_router.unwrap());

        // Local route
        let local_authority = UAuthority {
            name: Some("local".to_string()),
            number: Some(Number::Ip(vec![192, 168, 1, 100])),
            ..Default::default()
        };
        let local_route = Route::new(&local_authority, &local_transport_router_handle);

        // A first remote route
        let remote_authority_1 = UAuthority {
            name: Some("remote_1".to_string()),
            number: Some(Number::Ip(vec![192, 168, 1, 200])),
            ..Default::default()
        };
        let remote_route_1 = Route::new(&remote_authority_1, &remote_transport_router_handle);

        // A second remote route
        let remote_authority_2 = UAuthority {
            name: Some("remote_2".to_string()),
            number: Some(Number::Ip(vec![192, 168, 1, 201])),
            ..Default::default()
        };
        let remote_route_2 = Route::new(&remote_authority_2, &remote_transport_router_handle);

        let streamer = UStreamer::new("hoge");

        // Add forwarding rules to route local_route<->remote_route_1
        assert_eq!(
            streamer
                .add_forwarding_rule(local_route.clone(), remote_route_1.clone())
                .await,
            Ok(())
        );
        assert_eq!(
            streamer
                .add_forwarding_rule(remote_route_1.clone(), local_route.clone())
                .await,
            Ok(())
        );

        // Add forwarding rules to route local_route<->remote_route_2
        assert_eq!(
            streamer
                .add_forwarding_rule(local_route.clone(), remote_route_2.clone())
                .await,
            Ok(())
        );
        assert_eq!(
            streamer
                .add_forwarding_rule(remote_route_2.clone(), local_route.clone())
                .await,
            Ok(())
        );
    }

    #[async_std::test]
    async fn test_fails_to_build_utransport() {
        // Local transport router
        let local_transport_router = UTransportRouter::start(
            "FOO".to_string(),
            UTransportBuilderFoo::new(false),
            100,
            200,
            300,
        );
        if let Err(e) = &local_transport_router {
            println!("local_transport_router failed: {e:?}");
        }
        assert!(local_transport_router.is_err());
    }
}
