//! Control-plane layer.
//!
//! Owns route-registration lifecycle semantics and the route-table identity model.
//! This layer is responsible for idempotent insert/remove behavior and rollback-safe
//! transitions when listener registration fails.
//!
//! ```
//! use std::sync::Arc;
//! use async_trait::async_trait;
//! use up_rust::{UCode, UListener, UMessage, UStatus, UTransport, UUri};
//! use up_streamer::{Endpoint, UStreamer};
//! use usubscription_static_file::USubscriptionStaticFile;
//!
//! # struct MockTransport;
//! #
//! # #[async_trait]
//! # impl UTransport for MockTransport {
//! #     async fn send(&self, _message: UMessage) -> Result<(), UStatus> {
//! #         Ok(())
//! #     }
//! #
//! #     async fn receive(
//! #         &self,
//! #         _source_filter: &UUri,
//! #         _sink_filter: Option<&UUri>,
//! #     ) -> Result<UMessage, UStatus> {
//! #         Err(UStatus::fail_with_code(
//! #             UCode::INVALID_ARGUMENT,
//! #             "receive not used in control-plane doctest",
//! #         ))
//! #     }
//! #
//! #     async fn register_listener(
//! #         &self,
//! #         _source_filter: &UUri,
//! #         _sink_filter: Option<&UUri>,
//! #         _listener: Arc<dyn UListener>,
//! #     ) -> Result<(), UStatus> {
//! #         Ok(())
//! #     }
//! #
//! #     async fn unregister_listener(
//! #         &self,
//! #         _source_filter: &UUri,
//! #         _sink_filter: Option<&UUri>,
//! #         _listener: Arc<dyn UListener>,
//! #     ) -> Result<(), UStatus> {
//! #         Ok(())
//! #     }
//! # }
//! #
//! # tokio::runtime::Runtime::new().unwrap().block_on(async {
//! let usubscription = Arc::new(USubscriptionStaticFile::new(
//!     "../utils/usubscription-static-file/static-configs/testdata.json".to_string(),
//! ));
//! let mut streamer = UStreamer::new("control-plane-doc", 16, usubscription).unwrap();
//! let left_transport: Arc<dyn UTransport> = Arc::new(MockTransport);
//! let right_transport: Arc<dyn UTransport> = Arc::new(MockTransport);
//! let left = Endpoint::new("left", "left-authority", left_transport);
//! let right = Endpoint::new("right", "right-authority", right_transport);
//!
//! // The control plane ensures duplicate insert/remove transitions stay idempotent.
//! streamer.add_route(left.clone(), right.clone()).await.unwrap();
//! assert!(streamer.add_route(left.clone(), right.clone()).await.is_err());
//! streamer.delete_route(left.clone(), right.clone()).await.unwrap();
//! assert!(streamer.delete_route(left, right).await.is_err());
//! # });
//! ```

pub(crate) mod route_lifecycle;
pub(crate) mod route_table;
pub(crate) mod transport_identity;
