//! Publish-source filter derivation and dedupe policy.

use std::collections::HashSet;
use subscription_cache::SubscriptionInformation;
use tracing::{debug, warn};
use up_rust::UUri;

/// Resolves publish source filters for route listeners under one ingress->egress pair.
pub(crate) struct PublishRouteResolver<'a> {
    ingress_authority: &'a str,
    egress_authority: &'a str,
    tag: &'a str,
    action: &'a str,
}

impl<'a> PublishRouteResolver<'a> {
    /// Creates a resolver for one route context and log scope.
    pub(crate) fn new(
        ingress_authority: &'a str,
        egress_authority: &'a str,
        tag: &'a str,
        action: &'a str,
    ) -> Self {
        Self {
            ingress_authority,
            egress_authority,
            tag,
            action,
        }
    }

    /// Returns `true` when a subscription topic can originate from the ingress authority.
    fn topic_matches_ingress_authority(&self, topic: &UUri) -> bool {
        topic.authority_name == "*" || topic.authority_name == self.ingress_authority
    }

    /// Builds a single publish source filter for a subscriber topic when applicable.
    fn derive_source_filter_for_topic(&self, topic: &UUri) -> Option<UUri> {
        if !self.topic_matches_ingress_authority(topic) {
            debug!(
                "{}:{} skipping publish listener {} for in_authority='{}', out_authority='{}', topic_authority='{}', topic={topic:?}",
                self.tag,
                self.action,
                self.action,
                self.ingress_authority,
                self.egress_authority,
                topic.authority_name,
            );
            return None;
        }

        match UUri::try_from_parts(
            self.ingress_authority,
            topic.ue_id,
            topic.uentity_major_version(),
            topic.resource_id(),
        ) {
            Ok(source_uri) => Some(source_uri),
            Err(err) => {
                warn!(
                    "{}:{} unable to build publish source URI for in_authority='{}', out_authority='{}', topic={topic:?}: {}",
                    self.tag,
                    self.action,
                    self.ingress_authority,
                    self.egress_authority,
                    err,
                );
                None
            }
        }
    }

    #[allow(clippy::mutable_key_type)]
    /// Derives deduplicated publish source filters for all matching subscribers.
    pub(crate) fn derive_source_filters(
        &self,
        subscribers: &HashSet<SubscriptionInformation>,
    ) -> HashSet<UUri> {
        let mut source_filters = HashSet::new();

        for subscriber in subscribers {
            if let Some(source_uri) = self.derive_source_filter_for_topic(&subscriber.topic) {
                source_filters.insert(source_uri);
            }
        }

        source_filters
    }
}

#[cfg(test)]
mod tests {
    use super::PublishRouteResolver;
    use std::collections::HashSet;
    use std::str::FromStr;
    use subscription_cache::SubscriptionInformation;
    use up_rust::core::usubscription::{
        EventDeliveryConfig, SubscribeAttributes, SubscriberInfo, SubscriptionStatus,
    };
    use up_rust::UUri;

    fn subscription_info(topic: &str, subscriber: &str) -> SubscriptionInformation {
        SubscriptionInformation {
            topic: UUri::from_str(topic).expect("valid topic UUri"),
            subscriber: SubscriberInfo {
                uri: Some(UUri::from_str(subscriber).expect("valid subscriber UUri")).into(),
                ..Default::default()
            },
            status: SubscriptionStatus::default(),
            attributes: SubscribeAttributes::default(),
            config: EventDeliveryConfig::default(),
        }
    }

    #[test]
    fn resolver_blocks_mismatched_topic_authority() {
        let topic = UUri::from_str("//authority-a/5BA0/1/8001").expect("valid topic UUri");
        let resolver =
            PublishRouteResolver::new("authority-c", "authority-b", "routing-test", "insert");

        let source = resolver.derive_source_filter_for_topic(&topic);

        assert!(source.is_none());
    }

    #[test]
    fn resolver_allows_wildcard_topic_authority() {
        let topic = UUri::from_str("//*/5BA0/1/8001").expect("valid wildcard topic UUri");
        let resolver =
            PublishRouteResolver::new("authority-c", "authority-b", "routing-test", "insert");

        let source = resolver
            .derive_source_filter_for_topic(&topic)
            .expect("wildcard topic should resolve");

        assert_eq!(source.authority_name, "authority-c");
        assert_eq!(source.ue_id, topic.ue_id);
        assert_eq!(
            source.uentity_major_version(),
            topic.uentity_major_version()
        );
        assert_eq!(source.resource_id(), topic.resource_id());
    }

    #[test]
    #[allow(clippy::mutable_key_type)]
    fn resolver_dedupes_sources_across_subscribers() {
        let mut subscribers = HashSet::new();
        subscribers.insert(subscription_info(
            "//authority-a/5BA0/1/8001",
            "//authority-b/5678/1/1234",
        ));
        subscribers.insert(subscription_info(
            "//authority-a/5BA0/1/8001",
            "//authority-b/5679/1/1234",
        ));
        subscribers.insert(subscription_info(
            "//authority-z/5BA0/1/8001",
            "//authority-b/567A/1/1234",
        ));

        let resolver =
            PublishRouteResolver::new("authority-a", "authority-b", "routing-test", "insert");
        let filters = resolver.derive_source_filters(&subscribers);

        assert_eq!(filters.len(), 1);
        let expected =
            UUri::try_from_parts("authority-a", 0x5BA0, 0x1, 0x8001).expect("expected source uri");
        assert!(filters.contains(&expected));
    }
}
