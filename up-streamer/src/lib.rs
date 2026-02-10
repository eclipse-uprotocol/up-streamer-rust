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

//! # up-streamer
//!
//! `up-streamer` implements the `UStreamer` specification to bridge traffic between
//! uProtocol transports.
//!
//! Typical usage is API-first and remains centered on [`Endpoint`] and [`UStreamer`].
//! Internal modules are organized by domain layer to keep behavior ownership explicit.
//!
//! ## Static Configuration Mode
//!
//! ```
//! use std::sync::Arc;
//! use up_streamer::{Endpoint, UStreamer};
//! use up_rust::core::usubscription::USubscription;
//! use usubscription_static_file::USubscriptionStaticFile;
//! use up_rust::UTransport;
//!
//! # pub mod mock_transport {
//! #     use std::sync::Arc;
//! #     use async_trait::async_trait;
//! #     use up_rust::{UListener, UMessage, UStatus, UTransport, UUri};
//! #
//! #     pub struct MockTransport;
//! #
//! #     #[async_trait]
//! #     impl UTransport for MockTransport {
//! #         async fn send(&self, _message: UMessage) -> Result<(), UStatus> { Ok(()) }
//! #         async fn receive(
//! #             &self,
//! #             _source_filter: &UUri,
//! #             _sink_filter: Option<&UUri>,
//! #         ) -> Result<UMessage, UStatus> {
//! #             unimplemented!("not needed for this doctest")
//! #         }
//! #         async fn register_listener(
//! #             &self,
//! #             _source_filter: &UUri,
//! #             _sink_filter: Option<&UUri>,
//! #             _listener: Arc<dyn UListener>,
//! #         ) -> Result<(), UStatus> {
//! #             Ok(())
//! #         }
//! #         async fn unregister_listener(
//! #             &self,
//! #             _source_filter: &UUri,
//! #             _sink_filter: Option<&UUri>,
//! #             _listener: Arc<dyn UListener>,
//! #         ) -> Result<(), UStatus> {
//! #             Ok(())
//! #         }
//! #     }
//! # }
//!
//! # tokio::runtime::Runtime::new().unwrap().block_on(async {
//! let usubscription: Arc<dyn USubscription> = Arc::new(USubscriptionStaticFile::new(
//!     "../utils/usubscription-static-file/static-configs/testdata.json".to_string(),
//! ));
//! let mut streamer = UStreamer::new("quick-start", 16, usubscription).await.unwrap();
//!
//! let left_transport: Arc<dyn UTransport> = Arc::new(mock_transport::MockTransport);
//! let right_transport: Arc<dyn UTransport> = Arc::new(mock_transport::MockTransport);
//! let left = Endpoint::new("left", "left-authority", left_transport);
//! let right = Endpoint::new("right", "right-authority", right_transport);
//!
//! streamer
//!     .add_route(left.clone(), right.clone())
//!     .await
//!     .unwrap();
//! streamer.delete_route(left, right).await.unwrap();
//! # });
//! ```
//!
//! ## Live Canonical uSubscription Mode
//!
//! `UStreamer` consumes `Arc<dyn USubscription>`. In live mode, pass a trait-object backed by
//! canonical uSubscription APIs (for example an RPC-backed client).
//!
//! ```no_run
//! use std::sync::Arc;
//! use up_streamer::UStreamer;
//! use up_rust::core::usubscription::USubscription;
//! # use usubscription_static_file::USubscriptionStaticFile;
//!
//! // In real live mode, build a canonical client:
//! // let usubscription: Arc<dyn USubscription> = Arc::new(RpcClientUSubscription::new(...));
//!
//! # tokio::runtime::Runtime::new().unwrap().block_on(async {
//! # let usubscription: Arc<dyn USubscription> = Arc::new(USubscriptionStaticFile::new(
//! #     "../utils/usubscription-static-file/static-configs/testdata.json".to_string(),
//! # ));
//! let _streamer = UStreamer::new("live-canonical", 16, usubscription).await.unwrap();
//! # });
//! ```
//!
//! `refresh_subscriptions()` returns `SubscriptionSyncHealth` only. Canonical subscription
//! listings/counts/deltas remain the responsibility of the uSubscription service.
//!
//! Note for this migration phase: required streamer binaries expose `live_usubscription` as a
//! reserved mode and fail fast with an explicit deferred-integration message.
//!
//! Route lifecycle APIs are `add_route` / `delete_route`.
//!
//! ## Route contract
//!
//! This doctest focuses on route lifecycle behavior exposed by the API facade:
//! same-authority rules are rejected, duplicate inserts fail, and deleting a missing
//! rule returns an error.
//!
//! ```
//! use std::sync::Arc;
//! use up_streamer::{Endpoint, UStreamer};
//! use up_rust::core::usubscription::USubscription;
//! use usubscription_static_file::USubscriptionStaticFile;
//! use up_rust::UTransport;
//!
//! # pub mod mock_transport {
//! #     use std::sync::Arc;
//! #     use async_trait::async_trait;
//! #     use up_rust::{UListener, UMessage, UStatus, UTransport, UUri};
//! #
//! #     pub struct MockTransport;
//! #
//! #     #[async_trait]
//! #     impl UTransport for MockTransport {
//! #         async fn send(&self, _message: UMessage) -> Result<(), UStatus> { Ok(()) }
//! #         async fn receive(
//! #             &self,
//! #             _source_filter: &UUri,
//! #             _sink_filter: Option<&UUri>,
//! #         ) -> Result<UMessage, UStatus> {
//! #             unimplemented!("not needed for this doctest")
//! #         }
//! #         async fn register_listener(
//! #             &self,
//! #             _source_filter: &UUri,
//! #             _sink_filter: Option<&UUri>,
//! #             _listener: Arc<dyn UListener>,
//! #         ) -> Result<(), UStatus> {
//! #             Ok(())
//! #         }
//! #         async fn unregister_listener(
//! #             &self,
//! #             _source_filter: &UUri,
//! #             _sink_filter: Option<&UUri>,
//! #             _listener: Arc<dyn UListener>,
//! #         ) -> Result<(), UStatus> {
//! #             Ok(())
//! #         }
//! #     }
//! # }
//!
//! # tokio::runtime::Runtime::new().unwrap().block_on(async {
//! let usubscription: Arc<dyn USubscription> = Arc::new(USubscriptionStaticFile::new(
//!     "../utils/usubscription-static-file/static-configs/testdata.json".to_string(),
//! ));
//! let mut streamer = UStreamer::new("contract", 16, usubscription).await.unwrap();
//!
//! let left_transport: Arc<dyn UTransport> = Arc::new(mock_transport::MockTransport);
//! let right_transport: Arc<dyn UTransport> = Arc::new(mock_transport::MockTransport);
//! let left = Endpoint::new("left", "left-authority", left_transport);
//! let right = Endpoint::new("right", "right-authority", right_transport);
//! let left_again = Endpoint::new(
//!     "left-again",
//!     "left-authority",
//!     Arc::new(mock_transport::MockTransport),
//! );
//!
//! assert!(streamer
//!     .add_route(left.clone(), left_again.clone())
//!     .await
//!     .is_err());
//!
//! assert!(streamer
//!     .add_route(left.clone(), right.clone())
//!     .await
//!     .is_ok());
//! assert!(streamer
//!     .add_route(left.clone(), right.clone())
//!     .await
//!     .is_err());
//!
//! assert!(streamer
//!     .delete_route(left.clone(), right.clone())
//!     .await
//!     .is_ok());
//! assert!(streamer
//!     .delete_route(left, right)
//!     .await
//!     .is_err());
//! # });
//! ```
//!
//! ## Internal architecture map
//!
//! - API facade: outward `Endpoint`/`UStreamer` surface
//! - Control plane: route-registration lifecycle and route-table ownership
//! - Routing: publish-source and subscription-resolution policy
//! - Data plane: ingress listeners and egress route worker pool
//! - Runtime: subscription bootstrap and worker runtime boundaries
//!
//! ## Observability model
//!
//! The workspace uses `tracing` for logs/events.
//! Library code emits events/spans and does not unconditionally initialize a global
//! subscriber. Binaries/plugins/tests are responsible for one-time
//! `tracing_subscriber` initialization at process boundaries.

mod control_plane;
mod data_plane;
mod endpoint;
pub use endpoint::Endpoint;

mod subscription_sync_health;
pub use subscription_sync_health::SubscriptionSyncHealth;

#[doc(hidden)]
pub mod observability;
mod routing;
mod runtime;

mod ustreamer;
pub use ustreamer::UStreamer;
