//! Subscription-directory adapter used by routing and data-plane flows.

use arc_swap::ArcSwap;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tracing::warn;
use up_rust::core::usubscription::FetchSubscriptionsResponse;
use up_rust::UStatus;

use crate::observability::events;
use crate::routing::subscription_cache::{SubscriptionCache, SubscriptionLookup};

const COMPONENT: &str = "subscription_directory";

struct SubscriptionDirectorySnapshot {
    version: u64,
    cache: SubscriptionCache,
}

#[derive(Clone)]
/// Route-subscriber directory facade over the subscription cache.
pub(crate) struct SubscriptionDirectory {
    snapshot: Arc<ArcSwap<SubscriptionDirectorySnapshot>>,
    next_version: Arc<AtomicU64>,
}

impl SubscriptionDirectory {
    fn from_cache(version: u64, cache: SubscriptionCache) -> Self {
        Self {
            snapshot: Arc::new(ArcSwap::from_pointee(SubscriptionDirectorySnapshot {
                version,
                cache,
            })),
            next_version: Arc::new(AtomicU64::new(version + 1)),
        }
    }

    fn lookup_route_subscribers_from_snapshot(
        snapshot: &SubscriptionDirectorySnapshot,
        out_authority: &str,
    ) -> SubscriptionLookup {
        match snapshot
            .cache
            .fetch_cache_entry_with_wildcard(out_authority)
        {
            Some(subscribers) => subscribers,
            None => {
                warn!(
                    event = events::SUBSCRIPTION_LOOKUP_EMPTY,
                    component = COMPONENT,
                    out_authority,
                    snapshot_version = snapshot.version,
                    "no subscribers found for egress authority"
                );
                HashMap::new()
            }
        }
    }

    /// Creates a directory with an empty subscription cache.
    pub(crate) fn empty() -> Self {
        Self::from_cache(0, SubscriptionCache::default())
    }

    /// Creates a directory facade over a shared subscription cache.
    #[cfg(test)]
    pub(crate) fn new(cache: SubscriptionCache) -> Self {
        Self::from_cache(0, cache)
    }

    /// Atomically applies one fetched subscription snapshot.
    pub(crate) async fn apply_snapshot(
        &self,
        snapshot: FetchSubscriptionsResponse,
    ) -> Result<(), UStatus> {
        let next_cache = SubscriptionCache::new(snapshot)?;
        let next_version = self.next_version.fetch_add(1, Ordering::Relaxed);
        self.snapshot.store(Arc::new(SubscriptionDirectorySnapshot {
            version: next_version,
            cache: next_cache,
        }));
        Ok(())
    }

    #[allow(dead_code)]
    /// Looks up subscribers for one egress authority with wildcard matching.
    pub(crate) async fn lookup_route_subscribers(&self, out_authority: &str) -> SubscriptionLookup {
        let snapshot = self.snapshot.load();
        Self::lookup_route_subscribers_from_snapshot(&snapshot, out_authority)
    }

    /// Looks up subscribers and returns the snapshot version used for derivation.
    pub(crate) async fn lookup_route_subscribers_with_version(
        &self,
        out_authority: &str,
    ) -> (u64, SubscriptionLookup) {
        let snapshot = self.snapshot.load();
        (
            snapshot.version,
            Self::lookup_route_subscribers_from_snapshot(&snapshot, out_authority),
        )
    }

    #[cfg(test)]
    pub(crate) fn current_version(&self) -> u64 {
        self.snapshot.load().version
    }
}

#[cfg(test)]
mod tests {
    use super::SubscriptionDirectory;
    use crate::routing::subscription_cache::SubscriptionCache;
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

        let directory = SubscriptionDirectory::new(initial_cache);
        assert_eq!(directory.current_version(), 0);

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
        assert_eq!(directory.current_version(), 0);

        let remaining = directory.lookup_route_subscribers("authority-b").await;
        assert_eq!(remaining.len(), 1);
    }

    #[tokio::test]
    async fn apply_snapshot_advances_version_and_lookup_version_matches() {
        let directory = SubscriptionDirectory::empty();
        assert_eq!(directory.current_version(), 0);

        directory
            .apply_snapshot(FetchSubscriptionsResponse {
                subscriptions: vec![subscription(
                    "//authority-a/5BA0/1/8001",
                    "//authority-b/5678/1/1234",
                )],
                ..Default::default()
            })
            .await
            .expect("first snapshot should apply");

        assert_eq!(directory.current_version(), 1);

        let (snapshot_version, subscribers) = directory
            .lookup_route_subscribers_with_version("authority-b")
            .await;
        assert_eq!(snapshot_version, 1);
        assert_eq!(subscribers.len(), 1);
    }
}
