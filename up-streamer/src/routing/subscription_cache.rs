//! Internal subscription cache used by routing and listener resolution.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use up_rust::core::usubscription::{FetchSubscriptionsResponse, SubscriberInfo};
use up_rust::UUri;
use up_rust::{UCode, UStatus};

use crate::routing::uri_identity_key::UriIdentityKey;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct SubscriptionIdentityKey {
    topic: UriIdentityKey,
    subscriber: Option<UriIdentityKey>,
}

impl From<&SubscriptionInformation> for SubscriptionIdentityKey {
    fn from(subscription_information: &SubscriptionInformation) -> Self {
        Self {
            topic: UriIdentityKey::from(&subscription_information.topic),
            subscriber: subscription_information
                .subscriber
                .uri
                .as_ref()
                .map(UriIdentityKey::from),
        }
    }
}

pub(crate) type SubscriptionLookup = HashMap<SubscriptionIdentityKey, SubscriptionInformation>;

#[derive(Clone)]
pub(crate) struct SubscriptionInformation {
    pub topic: UUri,
    pub subscriber: SubscriberInfo,
}

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

#[derive(Default)]
pub(crate) struct SubscriptionCache {
    subscription_cache_map: HashMap<String, SubscriptionLookup>,
}

impl SubscriptionCache {
    pub(crate) fn new(subscription_cache_map: FetchSubscriptionsResponse) -> Result<Self, UStatus> {
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

            let subscription_information = SubscriptionInformation { topic, subscriber };
            let subscriber_authority_name = match subscription_information.subscriber.uri.as_ref() {
                Some(uri) => uri.authority_name.clone(),
                None => {
                    return Err(UStatus::fail_with_code(
                        UCode::INVALID_ARGUMENT,
                        "Unable to retrieve authority name",
                    ))
                }
            };
            let subscription_identity = SubscriptionIdentityKey::from(&subscription_information);
            let authority_subscriptions = subscription_cache_hash_map
                .entry(subscriber_authority_name)
                .or_insert_with(HashMap::new);

            authority_subscriptions
                .entry(subscription_identity)
                .or_insert(subscription_information);
        }
        Ok(Self {
            subscription_cache_map: subscription_cache_hash_map,
        })
    }

    #[cfg(test)]
    pub(crate) fn fetch_cache_entry(&self, entry: &str) -> Option<SubscriptionLookup> {
        self.subscription_cache_map.get(entry).cloned()
    }

    pub(crate) fn fetch_cache_entry_with_wildcard(
        &self,
        entry: &str,
    ) -> Option<SubscriptionLookup> {
        let mut merged: SubscriptionLookup = HashMap::new();

        if let Some(exact_subscribers) = self.subscription_cache_map.get(entry) {
            merged.extend(exact_subscribers.clone());
        }

        if entry != "*" {
            if let Some(wildcard_subscribers) = self.subscription_cache_map.get("*") {
                merged.extend(wildcard_subscribers.clone());
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
            topic: Some(UUri::from_str(topic).expect("valid topic URI")).into(),
            subscriber: Some(SubscriberInfo {
                uri: Some(UUri::from_str(subscriber).expect("valid subscriber URI")).into(),
                ..Default::default()
            })
            .into(),
            ..Default::default()
        }
    }

    fn topics_for_authority(cache: &SubscriptionCache, authority: &str) -> Vec<UUri> {
        let mut topics: Vec<UUri> = cache
            .fetch_cache_entry(authority)
            .expect("authority should exist")
            .into_values()
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
        .expect("cache should build");

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
        .expect("initial cache should build");

        let rebuilt_cache = SubscriptionCache::new(FetchSubscriptionsResponse {
            subscriptions: vec![subscription(
                "//authority-a/5BA1/1/8001",
                "//authority-b/5678/1/1234",
            )],
            ..Default::default()
        })
        .expect("rebuilt cache should build");

        assert_eq!(topics_for_authority(&initial_cache, "authority-b").len(), 2);

        let rebuilt_topics = topics_for_authority(&rebuilt_cache, "authority-b");
        assert_eq!(rebuilt_topics.len(), 1);
        assert_eq!(
            rebuilt_topics[0],
            UUri::from_str("//authority-a/5BA1/1/8001").unwrap()
        );
    }

    #[test]
    fn wildcard_lookup_merges_exact_and_wildcard_rows() {
        let cache = SubscriptionCache::new(FetchSubscriptionsResponse {
            subscriptions: vec![
                subscription("//authority-a/5BA0/1/8001", "//authority-b/5678/1/1234"),
                subscription("//authority-a/5BA0/1/8002", "//*/5678/1/1234"),
            ],
            ..Default::default()
        })
        .expect("cache should build");

        let merged_for_b = cache
            .fetch_cache_entry_with_wildcard("authority-b")
            .expect("authority-b should resolve");
        let merged_for_d = cache
            .fetch_cache_entry_with_wildcard("authority-d")
            .expect("authority-d should resolve wildcard");

        assert_eq!(merged_for_b.len(), 2);
        assert_eq!(merged_for_d.len(), 1);
    }
}
