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

use up_rust::{UStatus, UTransport};

/// [`UTransportBuilder`] is required to allow us to carefully instantiate the `Box<dyn UTransport`
/// on a separate OS thread since [`UTransport`][up_rust::UTransport] is not thread-safe
///
/// # Examples
///
/// ## Example usage
///
/// For examples on how [`UTransportBuilder`] is used see [`UTransportRouter`][crate::UTransportRouter]
///
/// ## `impl`ing the [`UTransportBuilder`] trait
///
/// ```
/// // -- The definition of `UPClientFoo`, defined elsewhere --
/// pub mod up_client_foo {
///     use up_rust::{UMessage, UTransport, UStatus, UUIDBuilder, UUri};
///     use async_trait::async_trait;
///     use up_streamer::UTransportBuilder;
///     pub struct UPClientFoo;
///
///     #[async_trait]
///     impl UTransport for UPClientFoo {
///         async fn send(&self, _message: UMessage) -> Result<(), UStatus> {
///             todo!()
///         }
///
///         async fn receive(&self, _topic: UUri) -> Result<UMessage, UStatus> {
///             todo!()
///         }
///
///         async fn register_listener(
///             &self,
///             topic: UUri,
///             _listener: Box<dyn Fn(Result<UMessage, UStatus>) + Send + Sync + 'static>,
///         ) -> Result<String, UStatus> {
///             println!("UPClientFoo: registering topic: {:?}", topic);
///             let uuid = UUIDBuilder::new().build();
///             Ok(uuid.to_string())
///         }
///
///         async fn unregister_listener(&self, topic: UUri, listener: &str) -> Result<(), UStatus> {
///             println!(
///                 "UPClientFoo: unregistering topic: {topic:?} with listener string: {listener}"
///             );
///             Ok(())
///         }
///     }
///
///     impl UPClientFoo {
///         pub fn new() -> Self {
///             Self {}
///         }
///     }
///
///     #[derive(Clone)]
///     pub struct Session {
///         session_id: u64
///     }
///
///     impl Session {
///         pub fn new(session_id: u64) -> Self {
///             Self {
///                session_id
///             }
///         }
///     }
/// }
///
/// use std::sync::Arc;
/// use up_streamer::{Route, UTransportBuilder, UTransportRouter};
/// use up_rust::{Number, UAuthority, UStatus, UTransport};
///
/// // -- implementing `UTransportBuilder` for `UPClientFoo` --
/// pub struct UTransportBuilderFoo {
///     pub session: Arc<up_client_foo::Session>,
/// }
///
/// impl UTransportBuilder for UTransportBuilderFoo {
///     fn build(&self) -> Result<Box<dyn UTransport>, UStatus> {
///         let utransport_foo: Box<dyn UTransport> = Box::new(up_client_foo::UPClientFoo::new());
///         Ok(utransport_foo)
///     }
/// }
///
/// impl UTransportBuilderFoo {
///     pub fn new(session_id: u64) -> Self {
///         Self {
///             session: Arc::new(up_client_foo::Session::new(session_id))
///         }
///     }
/// }
/// ```
pub trait UTransportBuilder: Send + Sync {
    fn build(&self) -> Result<Box<dyn UTransport>, UStatus>;
}
