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

use async_std::sync::{Arc, Mutex};
use log::*;
use up_rust::{UAuthority, UTransport};

const ROUTE_TAG: &str = "Route:";
const ROUTEFN_NEW_TAG: &str = "new():";

///
/// [`Route`] is defined as a combination of [`UAuthority`][up_rust::UAuthority] and
/// [`Arc<Mutex<Box<dyn UTransport>>>`][up_rust::UTransport] as routes are at the [`UAuthority`][up_rust::UAuthority] level.
///
/// # Examples
///
/// ```
/// use std::sync::Arc;
/// use async_std::sync::Mutex;
/// use up_rust::{Number, UAuthority, UTransport};
/// use up_streamer::Route;
///
/// # pub mod up_client_foo {
/// #     use std::sync::Arc;
/// #     use up_rust::{UMessage, UTransport, UStatus, UUIDBuilder, UUri, UListener};
/// #     use async_trait::async_trait;
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
///
/// let local_transport: Arc<Mutex<Box<dyn UTransport>>> = Arc::new(Mutex::new(Box::new(up_client_foo::UPClientFoo::new())));
///
/// let authority_foo = UAuthority {
///     name: Some("foo_name".to_string()).into(),
///     number: Some(Number::Ip(vec![192, 168, 1, 100])),
///     ..Default::default()
/// };
///
/// let local_route = Route::new(authority_foo, local_transport);
/// ```
#[derive(Clone)]
pub struct Route {
    pub(crate) authority: UAuthority,
    pub(crate) transport: Arc<Mutex<Box<dyn UTransport>>>,
}

impl Route {
    pub fn new(authority: UAuthority, transport: Arc<Mutex<Box<dyn UTransport>>>) -> Self {
        // Try to initiate logging.
        // Required in case of dynamic lib, otherwise no logs.
        // But cannot be done twice in case of static link.
        let _ = env_logger::try_init();
        debug!(
            "{}:{} Creating Route from: ({:?})",
            &ROUTE_TAG, &ROUTEFN_NEW_TAG, &authority,
        );
        Self {
            authority,
            transport,
        }
    }
}
