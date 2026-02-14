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

use std::sync::Arc;
use up_rust::UTransport;

///
/// [`Endpoint`] is defined as a combination of `authority_name` and
/// [`Arc<Mutex<Box<dyn UTransport>>>`][up_rust::UTransport] as endpoints are at the authority level.
///
/// # Examples
///
/// ```
/// use std::sync::Arc;
/// use tokio::sync::Mutex;
/// use up_rust::UTransport;
/// use up_streamer::Endpoint;
///
/// # pub mod up_client_foo {
/// #     use std::sync::Arc;
/// #     use up_rust::{UMessage, UTransport, UStatus, UUri, UListener};
/// #     use async_trait::async_trait;
/// #     pub struct UPClientFoo;
/// #
/// #     #[async_trait]
/// #     impl UTransport for UPClientFoo {
/// #         async fn send(&self, _message: UMessage) -> Result<(), UStatus> {
/// #             todo!()
/// #         }
/// #
/// #         async fn receive(
/// #             &self,
/// #            _source_filter: &UUri,
/// #            _sink_filter: Option<&UUri>,
/// #         ) -> Result<UMessage, UStatus> {
/// #             todo!()
/// #         }
/// #
/// #         async fn register_listener(
/// #                     &self,
/// #                     source_filter: &UUri,
/// #                     sink_filter: Option<&UUri>,
/// #                     listener: Arc<dyn UListener>,
/// #         ) -> Result<(), UStatus> {
/// #             println!("UPClientFoo: registering source_filter: {:?}", source_filter);
/// #             Ok(())
/// #         }
/// #
/// #         async fn unregister_listener(
/// #                     &self,
/// #                     source_filter: &UUri,
/// #                     sink_filter: Option<&UUri>,
/// #                     listener: Arc<dyn UListener>,
/// #         ) -> Result<(), UStatus> {
/// #             println!(
/// #                 "UPClientFoo: unregistering source_filter: {source_filter:?}"
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
/// let local_transport: Arc<dyn UTransport> = Arc::new(up_client_foo::UPClientFoo::new());
///
/// let authority_foo = "foo_authority";
///
/// let local_endpoint = Endpoint::new("local_endpoint", authority_foo, local_transport);
/// ```
#[derive(Clone)]
pub struct Endpoint {
    pub(crate) name: String,
    pub(crate) authority: String,
    pub(crate) transport: Arc<dyn UTransport>,
}

impl Endpoint {
    pub fn new(name: &str, authority: &str, transport: Arc<dyn UTransport>) -> Self {
        Self {
            name: name.to_string(),
            authority: authority.to_string(),
            transport,
        }
    }
}
