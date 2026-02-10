//! Internal subscription cache used by routing and listener resolution.

use crate::observability::events;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use tracing::{debug, error};
use up_rust::core::usubscription::{FetchSubscriptionsResponse, SubscriberInfo};
use up_rust::UUri;
use up_rust::{UCode, UStatus};

use crate::routing::uri_identity_key::UriIdentityKey;

const COMPONENT: &str = "subscription_cache";

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
    wildcard_merged_cache_map: HashMap<String, SubscriptionLookup>,
}

impl SubscriptionCache {
    fn build_wildcard_merged_cache(
        subscription_cache_map: &HashMap<String, SubscriptionLookup>,
    ) -> HashMap<String, SubscriptionLookup> {
        let wildcard_rows = subscription_cache_map.get("*");
        let mut merged_cache_map = HashMap::with_capacity(subscription_cache_map.len());

        for (authority, exact_rows) in subscription_cache_map {
            let mut merged_rows = exact_rows.clone();
            if authority != "*" {
                if let Some(wildcard_rows) = wildcard_rows {
                    merged_rows.extend(wildcard_rows.clone());
                }
            }
            merged_cache_map.insert(authority.clone(), merged_rows);
        }

        merged_cache_map
    }

    pub(crate) fn new(subscription_cache_map: FetchSubscriptionsResponse) -> Result<Self, UStatus> {
        let input_rows = subscription_cache_map.subscriptions.len();
        debug!(
            event = events::SUBSCRIPTION_SNAPSHOT_REBUILD_START,
            component = COMPONENT,
            input_rows,
            "starting subscription snapshot rebuild"
        );

        let mut subscription_cache_hash_map = HashMap::new();
        for subscription in subscription_cache_map.subscriptions {
            let topic = match subscription.topic.into_option() {
                Some(topic) => topic,
                None => {
                    let err = UStatus::fail_with_code(
                        UCode::INVALID_ARGUMENT,
                        "Unable to retrieve topic".to_string(),
                    );
                    error!(
                        event = events::SUBSCRIPTION_SNAPSHOT_REBUILD_FAILED,
                        component = COMPONENT,
                        reason = "missing_topic",
                        err = %err,
                        "subscription snapshot rebuild failed"
                    );
                    return Err(err);
                }
            };
            let subscriber = match subscription.subscriber.into_option() {
                Some(subscriber) => subscriber,
                None => {
                    let err = UStatus::fail_with_code(
                        UCode::INVALID_ARGUMENT,
                        "Unable to retrieve topic".to_string(),
                    );
                    error!(
                        event = events::SUBSCRIPTION_SNAPSHOT_REBUILD_FAILED,
                        component = COMPONENT,
                        reason = "missing_subscriber",
                        err = %err,
                        "subscription snapshot rebuild failed"
                    );
                    return Err(err);
                }
            };

            let subscription_information = SubscriptionInformation { topic, subscriber };
            let subscriber_authority_name = match subscription_information.subscriber.uri.as_ref() {
                Some(uri) => uri.authority_name.clone(),
                None => {
                    let err = UStatus::fail_with_code(
                        UCode::INVALID_ARGUMENT,
                        "Unable to retrieve authority name",
                    );
                    error!(
                        event = events::SUBSCRIPTION_SNAPSHOT_REBUILD_FAILED,
                        component = COMPONENT,
                        reason = "missing_subscriber_authority",
                        err = %err,
                        "subscription snapshot rebuild failed"
                    );
                    return Err(err);
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

        let authority_count = subscription_cache_hash_map.len();
        let subscription_count: usize =
            subscription_cache_hash_map.values().map(HashMap::len).sum();
        debug!(
            event = events::SUBSCRIPTION_SNAPSHOT_REBUILD_OK,
            component = COMPONENT,
            input_rows,
            authority_count,
            subscription_count,
            "subscription snapshot rebuild succeeded"
        );

        let wildcard_merged_cache_map =
            Self::build_wildcard_merged_cache(&subscription_cache_hash_map);

        Ok(Self {
            subscription_cache_map: subscription_cache_hash_map,
            wildcard_merged_cache_map,
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
        let exact_count = self
            .subscription_cache_map
            .get(entry)
            .map(HashMap::len)
            .unwrap_or(0);
        let wildcard_count = if entry == "*" {
            0
        } else {
            self.subscription_cache_map
                .get("*")
                .map(HashMap::len)
                .unwrap_or(0)
        };

        let merged = if entry == "*" {
            self.wildcard_merged_cache_map.get("*").cloned()
        } else {
            self.wildcard_merged_cache_map
                .get(entry)
                .cloned()
                .or_else(|| self.subscription_cache_map.get("*").cloned())
        };

        let merged_count = merged.as_ref().map(HashMap::len).unwrap_or(0);
        debug!(
            event = events::SUBSCRIPTION_WILDCARD_MERGE_SUMMARY,
            component = COMPONENT,
            out_authority = entry,
            exact_count,
            wildcard_count,
            merged_count,
            "subscription wildcard merge summary"
        );

        merged.filter(|rows| !rows.is_empty())
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
