//! Subscription-directory adapter used by routing and data-plane flows.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::warn;
use up_rust::core::usubscription::FetchSubscriptionsResponse;
use up_rust::UStatus;

use crate::routing::subscription_cache::{SubscriptionCache, SubscriptionLookup};

#[derive(Clone)]
/// Route-subscriber directory facade over the subscription cache.
pub(crate) struct SubscriptionDirectory {
    cache: Arc<Mutex<SubscriptionCache>>,
}

impl SubscriptionDirectory {
    /// Creates a directory with an empty subscription cache.
    pub(crate) fn empty() -> Self {
        Self {
            cache: Arc::new(Mutex::new(SubscriptionCache::default())),
        }
    }

    /// Creates a directory facade over a shared subscription cache.
    #[cfg(test)]
    pub(crate) fn new(cache: Arc<Mutex<SubscriptionCache>>) -> Self {
        Self { cache }
    }

    /// Atomically applies one fetched subscription snapshot.
    pub(crate) async fn apply_snapshot(
        &self,
        snapshot: FetchSubscriptionsResponse,
    ) -> Result<(), UStatus> {
        let next_cache = SubscriptionCache::new(snapshot)?;
        let mut current_cache = self.cache.lock().await;
        *current_cache = next_cache;
        Ok(())
    }

    /// Looks up subscribers for one egress authority with wildcard matching.
    pub(crate) async fn lookup_route_subscribers(
        &self,
        out_authority: &str,
        tag: &str,
        action: &str,
    ) -> SubscriptionLookup {
        match self
            .cache
            .lock()
            .await
            .fetch_cache_entry_with_wildcard(out_authority)
        {
            Some(subscribers) => subscribers,
            None => {
                warn!("{tag}:{action} no subscribers found for out_authority: {out_authority:?}");
                HashMap::new()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SubscriptionDirectory;
    use crate::routing::subscription_cache::SubscriptionCache;
    use std::str::FromStr;
    use std::sync::Arc;
    use tokio::sync::Mutex;
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

    #[tokio::test]
    async fn apply_snapshot_keeps_previous_cache_when_rebuild_fails() {
        let initial_cache = SubscriptionCache::new(FetchSubscriptionsResponse {
            subscriptions: vec![subscription(
                "//authority-a/5BA0/1/8001",
                "//authority-b/5678/1/1234",
            )],
            ..Default::default()
        })
        .expect("initial cache should build");

        let directory = SubscriptionDirectory::new(Arc::new(Mutex::new(initial_cache)));

        let invalid_snapshot = FetchSubscriptionsResponse {
            subscriptions: vec![Subscription {
                topic: None.into(),
                subscriber: Some(SubscriberInfo {
                    uri: Some(UUri::from_str("//authority-b/5678/1/1234").unwrap()).into(),
                    ..Default::default()
                })
                .into(),
                ..Default::default()
            }],
            ..Default::default()
        };

        assert!(directory.apply_snapshot(invalid_snapshot).await.is_err());

        let remaining = directory
            .lookup_route_subscribers("authority-b", "test", "lookup")
            .await;
        assert_eq!(remaining.len(), 1);
    }
}
