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

use crate::utransport_router::UTransportRouterHandle;
use log::*;
use std::sync::Arc;
use up_rust::UAuthority;

const ROUTE_TAG: &str = "Route:";
const ROUTEFN_NEW_TAG: &str = "new():";

///
/// [`Route`] is defined as a combination of [`UAuthority`][up_rust::UAuthority] and
/// [`UTransportRouterHandle`] as routes are at the [`UAuthority`][up_rust::UAuthority] level.
///
/// # Examples
///
/// ```
/// use std::sync::Arc;
/// use up_streamer::{Route, UTransportBuilder, UTransportRouter};
/// use up_rust::{Number, UAuthority};
///
/// # pub mod foo_transport_builder {
/// #     use up_rust::{UMessage, UTransport, UStatus, UUIDBuilder, UUri};
/// #     use async_trait::async_trait;
/// #     use up_streamer::UTransportBuilder;
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
/// #
/// # }
///
/// let local_transport_router =
///             UTransportRouter::start("FOO".to_string(),
///                 foo_transport_builder::UTransportBuilderFoo::new(), 100, 200);
///         assert!(local_transport_router.is_ok());
///         let local_transport_router_handle = Arc::new(local_transport_router.unwrap());
///
/// let authority_foo = UAuthority {
///     name: Some("foo_name".to_string()).into(),
///     number: Some(Number::Ip(vec![192, 168, 1, 100])),
///     ..Default::default()
/// };
///
/// let local_route = Route::new(&authority_foo, &local_transport_router_handle);
/// ```
#[derive(Clone)]
pub struct Route {
    authority: UAuthority,
    transport_router_handle: Arc<UTransportRouterHandle>,
}

impl Route {
    /// Builds a [`Route`] based on [`UAuthority`][up_rust::UAuthority] and [`UTransportRouterHandle`]
    ///
    /// # Parameters
    ///
    /// * `authority` - The [`UAuthority`][up_rust::UAuthority] to associate with this [`Route`]
    /// * `transport_router_handle` - a [`UTransportRouterHandle`][crate::UTransportRouterHandle]
    ///                               obtained by calling
    ///                               [`UTransport::start()`][crate::UTransportRouter::start]
    ///                               with the appropriate arguments.
    ///
    /// For more information see the example under [`Route`]
    pub fn new(
        authority: &UAuthority,
        transport_router_handle: &Arc<UTransportRouterHandle>,
    ) -> Self {
        // Try to initiate logging.
        // Required in case of dynamic lib, otherwise no logs.
        // But cannot be done twice in case of static link.
        let _ = env_logger::try_init();
        if log_enabled!(Level::Debug) {
            debug!(
                "{}:{}:{} Creating Route from: ({:?}, {})",
                &transport_router_handle.name,
                &ROUTE_TAG,
                &ROUTEFN_NEW_TAG,
                &authority,
                &transport_router_handle.name
            );
        }
        Self {
            authority: authority.clone(),
            transport_router_handle: transport_router_handle.clone(),
        }
    }

    pub(crate) fn get_authority(&self) -> UAuthority {
        self.authority.clone()
    }

    pub(crate) fn get_transport_router_handle(&self) -> Arc<UTransportRouterHandle> {
        self.transport_router_handle.clone()
    }
}
