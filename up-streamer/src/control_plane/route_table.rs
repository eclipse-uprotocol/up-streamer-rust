//! Route-table data model and storage owner for control-plane route identity.

use crate::control_plane::transport_identity::TransportIdentityKey;
use crate::endpoint::Endpoint;
use std::collections::HashSet;
use tokio::sync::Mutex;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
/// Stable identity for a single ingress->egress route registration.
///
/// Equality intentionally includes transport identity to distinguish routes that share
/// authorities but are backed by different transport instances.
pub(crate) struct RouteKey {
    pub(crate) ingress_authority: String,
    pub(crate) egress_authority: String,
    pub(crate) ingress_transport: TransportIdentityKey,
    pub(crate) egress_transport: TransportIdentityKey,
}

impl RouteKey {
    /// Builds a `RouteKey` from outward API endpoints.
    #[inline(always)]
    pub(crate) fn from_endpoints(r#in: &Endpoint, out: &Endpoint) -> Self {
        Self {
            ingress_authority: r#in.authority.clone(),
            egress_authority: out.authority.clone(),
            ingress_transport: TransportIdentityKey::new(r#in.transport.clone()),
            egress_transport: TransportIdentityKey::new(out.transport.clone()),
        }
    }
}

/// Route registry storage owner for dedupe and idempotent presence checks.
pub(crate) struct RouteTable {
    routes: Mutex<HashSet<RouteKey>>,
}

impl RouteTable {
    /// Creates an empty route table.
    pub(crate) fn new() -> Self {
        Self {
            routes: Mutex::new(HashSet::new()),
        }
    }

    /// Inserts a route identity. Returns `true` only when first inserted.
    pub(crate) async fn insert_route(&self, route_key: RouteKey) -> bool {
        let mut routes = self.routes.lock().await;
        routes.insert(route_key)
    }

    /// Removes a route identity. Returns `true` only when the route existed.
    pub(crate) async fn remove_route(&self, route_key: &RouteKey) -> bool {
        let mut routes = self.routes.lock().await;
        routes.remove(route_key)
    }
}

#[cfg(test)]
mod tests {
    use super::{RouteKey, RouteTable};
    use crate::Endpoint;
    use async_trait::async_trait;
    use std::sync::Arc;
    use up_rust::{UCode, UListener, UMessage, UStatus, UTransport, UUri};

    struct NoopTransport;

    #[async_trait]
    impl UTransport for NoopTransport {
        async fn send(&self, _message: UMessage) -> Result<(), UStatus> {
            Ok(())
        }

        async fn receive(
            &self,
            _source_filter: &UUri,
            _sink_filter: Option<&UUri>,
        ) -> Result<UMessage, UStatus> {
            Err(UStatus::fail_with_code(
                UCode::UNIMPLEMENTED,
                "not used in tests",
            ))
        }

        async fn register_listener(
            &self,
            _source_filter: &UUri,
            _sink_filter: Option<&UUri>,
            _listener: Arc<dyn UListener>,
        ) -> Result<(), UStatus> {
            Ok(())
        }

        async fn unregister_listener(
            &self,
            _source_filter: &UUri,
            _sink_filter: Option<&UUri>,
            _listener: Arc<dyn UListener>,
        ) -> Result<(), UStatus> {
            Ok(())
        }
    }

    fn build_route(
        in_name: &str,
        in_authority: &str,
        in_transport: Arc<dyn UTransport>,
        out_name: &str,
        out_authority: &str,
        out_transport: Arc<dyn UTransport>,
    ) -> RouteKey {
        let in_endpoint = Endpoint::new(in_name, in_authority, in_transport);
        let out_endpoint = Endpoint::new(out_name, out_authority, out_transport);
        RouteKey::from_endpoints(&in_endpoint, &out_endpoint)
    }

    #[tokio::test]
    async fn route_key_uses_transport_identity() {
        let shared_transport: Arc<dyn UTransport> = Arc::new(NoopTransport);
        let another_transport: Arc<dyn UTransport> = Arc::new(NoopTransport);

        let route_a = build_route(
            "in",
            "authority-a",
            shared_transport.clone(),
            "out-a",
            "authority-b",
            another_transport.clone(),
        );
        let route_b = build_route(
            "in",
            "authority-a",
            shared_transport,
            "out-b",
            "authority-b",
            another_transport,
        );
        let route_c = build_route(
            "in",
            "authority-a",
            Arc::new(NoopTransport),
            "out-c",
            "authority-b",
            Arc::new(NoopTransport),
        );

        assert_eq!(route_a, route_b);
        assert_ne!(route_a, route_c);
    }

    #[tokio::test]
    async fn route_table_insert_and_remove_are_idempotent() {
        let route_table = RouteTable::new();
        let route = build_route(
            "in",
            "authority-a",
            Arc::new(NoopTransport),
            "out",
            "authority-b",
            Arc::new(NoopTransport),
        );

        assert!(route_table.insert_route(route.clone()).await);
        assert!(!route_table.insert_route(route.clone()).await);

        assert!(route_table.remove_route(&route).await);
        assert!(!route_table.remove_route(&route).await);
    }
}
