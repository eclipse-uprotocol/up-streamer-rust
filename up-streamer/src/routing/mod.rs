//! Routing and subscription-resolution layer.
//!
//! Encapsulates publish-source derivation, wildcard authority handling, and dedupe
//! policy used when converting subscription directory state into listener filters.
//!
//! ```
//! use std::sync::Arc;
//! use async_trait::async_trait;
//! use up_rust::core::usubscription::USubscription;
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
//! #             "receive not used in routing doctest",
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
//! let usubscription: Arc<dyn USubscription> = Arc::new(USubscriptionStaticFile::new(
//!     "../utils/usubscription-static-file/static-configs/testdata.json".to_string(),
//! ));
//! let mut streamer = UStreamer::new("routing-doc", 16, usubscription).await.unwrap();
//! let ingress_transport: Arc<dyn UTransport> = Arc::new(MockTransport);
//! let egress_transport: Arc<dyn UTransport> = Arc::new(MockTransport);
//! let ingress = Endpoint::new("ingress", "authority-a", ingress_transport);
//! let egress = Endpoint::new("egress", "authority-b", egress_transport);
//!
//! // Routing policy resolves publish filters from subscription directory state.
//! streamer.add_route(ingress, egress).await.unwrap();
//! # });
//! ```

pub(crate) mod authority_filter;
pub(crate) mod publish_resolution;
pub(crate) mod subscription_cache;
pub(crate) mod subscription_directory;
pub(crate) mod uri_identity_key;
