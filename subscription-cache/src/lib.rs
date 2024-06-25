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

use async_std::sync::Mutex;
use std::collections::{HashMap, HashSet};
use up_rust::UUri;

pub type SubscribersMap = Mutex<HashMap<UUri, HashSet<UUri>>>;
pub struct SubscriptionCache {
    subscription_cache_map: SubscribersMap,
}

/// A [`SubscriptionCache`] is used to store and manage subscriptions to
/// topics. It is kept local to the streamer. The streamer will receive updates
/// from the subscription service, and update the SubscriptionCache accordingly.
impl SubscriptionCache {
    pub fn new(subscription_cache_map: SubscribersMap) -> Self {
        Self {
            subscription_cache_map,
        }
    }

    pub async fn fetch_cache(&self) -> HashMap<UUri, HashSet<UUri>> {
        self.subscription_cache_map.lock().await.clone()
    }
}
