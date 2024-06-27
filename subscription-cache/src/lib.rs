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
use protobuf::MessageField;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use up_rust::core::usubscription::{
    EventDeliveryConfig, FetchSubscriptionsResponse, SubscribeAttributes, SubscriberInfo,
    SubscriptionStatus,
};
use up_rust::UUri;

pub type SubscribersMap = Mutex<HashMap<MessageField<UUri>, HashSet<SubscriptionInformation>>>;

pub struct SubscriptionInformation {
    pub subscriber: MessageField<SubscriberInfo>,
    pub status: MessageField<SubscriptionStatus>,
    pub attributes: MessageField<SubscribeAttributes>,
    pub config: MessageField<EventDeliveryConfig>,
}

impl Eq for SubscriptionInformation {}

impl PartialEq for SubscriptionInformation {
    fn eq(&self, other: &Self) -> bool {
        self.subscriber == other.subscriber
    }
}

impl Hash for SubscriptionInformation {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.subscriber.hash(state);
    }
}

impl Clone for SubscriptionInformation {
    fn clone(&self) -> Self {
        Self {
            subscriber: self.subscriber.clone(),
            status: self.status.clone(),
            attributes: self.attributes.clone(),
            config: self.config.clone(),
        }
    }
}

pub struct SubscriptionCache {
    subscription_cache_map: SubscribersMap,
}

/// A [`SubscriptionCache`] is used to store and manage subscriptions to
/// topics. It is kept local to the streamer. The streamer will receive updates
/// from the subscription service, and update the SubscriptionCache accordingly.
impl SubscriptionCache {
    pub fn new(subscription_cache_map: FetchSubscriptionsResponse) -> Self {
        let mut subscription_cache_hash_map = HashMap::new();
        for subscription in subscription_cache_map.subscriptions {
            let uri = subscription.topic;
            // Create new hashset if the key does not exist and insert the subscription
            let subscription_information = SubscriptionInformation {
                subscriber: subscription.subscriber,
                status: subscription.status,
                attributes: subscription.attributes,
                config: subscription.config,
            };
            subscription_cache_hash_map
                .entry(uri)
                .or_insert_with(HashSet::new)
                .insert(subscription_information);
        }
        Self {
            subscription_cache_map: Mutex::new(subscription_cache_hash_map),
        }
    }

    pub async fn fetch_cache(
        &self,
    ) -> HashMap<MessageField<UUri>, HashSet<SubscriptionInformation>> {
        let cache_map = self.subscription_cache_map.lock().await;
        let mut cloned_map = HashMap::new();

        for (key, value) in cache_map.iter() {
            let cloned_key = key.clone();
            let cloned_value = value.iter().cloned().collect::<HashSet<_>>();
            cloned_map.insert(cloned_key, cloned_value);
        }

        cloned_map
    }
}
