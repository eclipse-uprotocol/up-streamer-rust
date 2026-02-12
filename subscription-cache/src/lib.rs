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

use log::warn;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::Mutex;
use up_rust::core::usubscription::{
    EventDeliveryConfig, FetchSubscriptionsResponse, SubscribeAttributes, SubscriberInfo,
    SubscriptionStatus,
};
use up_rust::UUri;
use up_rust::{UCode, UStatus};

pub type SubscribersMap = Mutex<HashMap<String, HashSet<SubscriptionInformation>>>;

// Tracks subscription information inside the SubscriptionCache
pub struct SubscriptionInformation {
    pub topic: UUri,
    pub subscriber: SubscriberInfo,
    pub status: SubscriptionStatus,
    pub attributes: SubscribeAttributes,
    pub config: EventDeliveryConfig,
}

// Will be moving this to up-rust
// Issue: https://github.com/eclipse-uprotocol/up-rust/issues/178
impl Eq for SubscriptionInformation {}

impl PartialEq for SubscriptionInformation {
    fn eq(&self, other: &Self) -> bool {
        self.topic == other.topic && self.subscriber == other.subscriber
    }
}

impl Hash for SubscriptionInformation {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.topic.hash(state);
        self.subscriber.hash(state);
    }
}

impl Clone for SubscriptionInformation {
    fn clone(&self) -> Self {
        Self {
            topic: self.topic.clone(),
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

impl Default for SubscriptionCache {
    fn default() -> Self {
        Self {
            subscription_cache_map: Mutex::new(HashMap::new()),
        }
    }
}

/// A [`SubscriptionCache`] is used to store and manage subscriptions to
/// topics. It is kept local to the streamer. The streamer will receive updates
/// from the subscription service, and update the SubscriptionCache accordingly.
impl SubscriptionCache {
    pub fn new(subscription_cache_map: FetchSubscriptionsResponse) -> Result<Self, UStatus> {
        let mut subscription_cache_hash_map = HashMap::new();
        for subscription in subscription_cache_map.subscriptions {
            let topic = subscription.topic.into_option().ok_or_else(|| {
                UStatus::fail_with_code(
                    UCode::INVALID_ARGUMENT,
                    "Unable to retrieve topic".to_string(),
                )
            })?;
            let subscriber = subscription.subscriber.into_option().ok_or_else(|| {
                UStatus::fail_with_code(
                    UCode::INVALID_ARGUMENT,
                    "Unable to retrieve topic".to_string(),
                )
            })?;
            // At minimum, topic and subscriber are required to track a subscription.
            // status, attributes, and config can be used either within the subscription service,
            // or for tracking pending subscriptions, but they are not required for forwarding
            // subscriptions across the streamer, so if not included, they will be set to default.
            let status = if let Some(status) = subscription.status.into_option() {
                status
            } else {
                warn!("Unable to parse status from subscription, setting as default");
                SubscriptionStatus::default()
            };
            let attributes = if let Some(attributes) = subscription.attributes.into_option() {
                attributes
            } else {
                warn!("Unable to parse attributes from subscription, setting as default");
                SubscribeAttributes::default()
            };
            let config = if let Some(config) = subscription.config.into_option() {
                config
            } else {
                warn!("Unable to parse config from subscription, setting as default");
                EventDeliveryConfig::default()
            };
            // Create new hashset if the key does not exist and insert the subscription
            let subscription_information = SubscriptionInformation {
                topic: topic.clone(),
                subscriber: subscriber.clone(),
                status,
                attributes,
                config,
            };
            let subscriber_authority_name = match subscription_information.subscriber.uri.as_ref() {
                Some(uri) => uri.authority_name.clone(),
                None => {
                    return Err(UStatus::fail_with_code(
                        UCode::INVALID_ARGUMENT,
                        "Unable to retrieve authority name",
                    ))
                }
            };
            subscription_cache_hash_map
                .entry(subscriber_authority_name)
                .or_insert_with(HashSet::new)
                .insert(subscription_information);
        }
        Ok(Self {
            subscription_cache_map: Mutex::new(subscription_cache_hash_map),
        })
    }

    pub fn fetch_cache_entry(&self, entry: String) -> Option<HashSet<SubscriptionInformation>> {
        let map = match self.subscription_cache_map.lock() {
            Ok(map) => map,
            Err(_) => return None,
        };
        map.get(&entry).cloned()
    }

    pub fn fetch_cache_entry_with_wildcard(
        &self,
        entry: &str,
    ) -> Option<HashSet<SubscriptionInformation>> {
        let map = match self.subscription_cache_map.lock() {
            Ok(map) => map,
            Err(_) => return None,
        };

        #[allow(clippy::mutable_key_type)]
        let mut merged = HashSet::new();

        if let Some(exact_subscribers) = map.get(entry) {
            merged.extend(exact_subscribers.iter().cloned());
        }

        if entry != "*" {
            if let Some(wildcard_subscribers) = map.get("*") {
                merged.extend(wildcard_subscribers.iter().cloned());
            }
        }

        if merged.is_empty() {
            None
        } else {
            Some(merged)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SubscriptionCache;
    use std::str::FromStr;
    use up_rust::core::usubscription::{FetchSubscriptionsResponse, SubscriberInfo, Subscription};
    use up_rust::UUri;

    fn subscription(topic: &str, subscriber: &str) -> Subscription {
        Subscription {
            topic: Some(UUri::from_str(topic).unwrap()).into(),
            subscriber: Some(SubscriberInfo {
                uri: Some(UUri::from_str(subscriber).unwrap()).into(),
                ..Default::default()
            })
            .into(),
            ..Default::default()
        }
    }

    fn topics_for_authority(cache: &SubscriptionCache, authority: &str) -> Vec<UUri> {
        let mut topics: Vec<UUri> = cache
            .fetch_cache_entry(authority.to_string())
            .unwrap()
            .into_iter()
            .map(|subscription| subscription.topic)
            .collect();
        topics.sort_by_key(|topic| topic.to_uri(false));
        topics
    }

    #[test]
    fn same_subscriber_different_topics_coexist() {
        let cache = SubscriptionCache::new(FetchSubscriptionsResponse {
            subscriptions: vec![
                subscription("//authority-a/5BA0/1/8001", "//authority-b/5678/1/1234"),
                subscription("//authority-a/5BA1/1/8001", "//authority-b/5678/1/1234"),
            ],
            ..Default::default()
        })
        .unwrap();

        let topics = topics_for_authority(&cache, "authority-b");

        assert_eq!(topics.len(), 2);
        assert_eq!(
            topics[0],
            UUri::from_str("//authority-a/5BA0/1/8001").unwrap()
        );
        assert_eq!(
            topics[1],
            UUri::from_str("//authority-a/5BA1/1/8001").unwrap()
        );
    }

    #[test]
    fn rebuild_reflects_removed_rows() {
        let initial_cache = SubscriptionCache::new(FetchSubscriptionsResponse {
            subscriptions: vec![
                subscription("//authority-a/5BA0/1/8001", "//authority-b/5678/1/1234"),
                subscription("//authority-a/5BA1/1/8001", "//authority-b/5678/1/1234"),
            ],
            ..Default::default()
        })
        .unwrap();

        let rebuilt_cache = SubscriptionCache::new(FetchSubscriptionsResponse {
            subscriptions: vec![subscription(
                "//authority-a/5BA1/1/8001",
                "//authority-b/5678/1/1234",
            )],
            ..Default::default()
        })
        .unwrap();

        assert_eq!(topics_for_authority(&initial_cache, "authority-b").len(), 2);

        let rebuilt_topics = topics_for_authority(&rebuilt_cache, "authority-b");
        assert_eq!(rebuilt_topics.len(), 1);
        assert_eq!(
            rebuilt_topics[0],
            UUri::from_str("//authority-a/5BA1/1/8001").unwrap()
        );
    }

    #[test]
    #[allow(clippy::mutable_key_type)]
    fn wildcard_lookup_merges_exact_and_wildcard_rows() {
        let cache = SubscriptionCache::new(FetchSubscriptionsResponse {
            subscriptions: vec![
                subscription("//authority-a/5BA0/1/8001", "//authority-b/5678/1/1234"),
                subscription("//authority-a/5BA0/1/8002", "//*/5678/1/1234"),
            ],
            ..Default::default()
        })
        .unwrap();

        let merged_for_b = cache
            .fetch_cache_entry_with_wildcard("authority-b")
            .unwrap();
        let merged_for_d = cache
            .fetch_cache_entry_with_wildcard("authority-d")
            .unwrap();

        assert_eq!(merged_for_b.len(), 2);
        assert_eq!(merged_for_d.len(), 1);
    }
}
