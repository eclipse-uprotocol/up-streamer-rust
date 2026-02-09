//! Runtime integration layer.
//!
//! Isolates subscription bootstrap and worker runtime boundaries so async/threading
//! behavior remains localized and predictable for the rest of the crate.
//!
//! ```
//! use std::sync::Arc;
//! use up_streamer::UStreamer;
//! use usubscription_static_file::USubscriptionStaticFile;
//!
//! // Runtime adapters are internal helpers and should not carry route policy.
//! let usubscription = Arc::new(USubscriptionStaticFile::new(
//!     "../utils/usubscription-static-file/static-configs/testdata.json".to_string(),
//! ));
//! let _streamer = UStreamer::new("runtime-doc", 16, usubscription).unwrap();
//! ```

pub(crate) mod subscription_runtime;
pub(crate) mod worker_runtime;
