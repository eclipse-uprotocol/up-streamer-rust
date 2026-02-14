/********************************************************************************
 * Copyright (c) 2026 Contributors to the Eclipse Foundation
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

//! Runtime integration layer.
//!
//! Isolates subscription bootstrap and worker runtime boundaries so async/threading
//! behavior remains localized and predictable for the rest of the crate.
//!
//! ```
//! use std::sync::Arc;
//! use up_streamer::UStreamer;
//! use up_rust::core::usubscription::USubscription;
//! use usubscription_static_file::USubscriptionStaticFile;
//!
//! // Runtime adapters are internal helpers and should not carry route policy.
//! # tokio::runtime::Runtime::new().unwrap().block_on(async {
//! let usubscription: Arc<dyn USubscription> = Arc::new(USubscriptionStaticFile::new(
//!     "../utils/usubscription-static-file/static-configs/testdata.json".to_string(),
//! ));
//! let _streamer = UStreamer::new("runtime-doc", 16, usubscription).await.unwrap();
//! # });
//! ```

pub(crate) mod worker_runtime;
