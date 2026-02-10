//! Route lifecycle orchestration for control-plane route transitions.

use crate::control_plane::route_table::{RouteKey, RouteTable};
use crate::data_plane::egress_pool::EgressRoutePool;
use crate::data_plane::ingress_registry::{IngressRouteRegistrationError, IngressRouteRegistry};
use crate::endpoint::Endpoint;
use crate::observability::events;
use crate::routing::subscription_directory::SubscriptionDirectory;
use std::error::Error;
use std::fmt::{Display, Formatter};
use tracing::{debug, error};

const COMPONENT: &str = "route_lifecycle";

/// Failures for route insertion orchestration.
#[derive(Debug)]
pub(crate) enum AddRouteError {
    SameAuthority,
    AlreadyExists,
    FailedToRegisterIngressRoute(IngressRouteRegistrationError),
}

/// Failures for route deletion orchestration.
#[derive(Debug)]
pub(crate) enum RemoveRouteError {
    SameAuthority,
    NotFound,
}

impl Display for AddRouteError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            AddRouteError::SameAuthority => {
                write!(f, "ingress and egress authorities must differ")
            }
            AddRouteError::AlreadyExists => write!(f, "route already exists"),
            AddRouteError::FailedToRegisterIngressRoute(err) => {
                write!(f, "failed to register ingress route listener: {err}")
            }
        }
    }
}

impl Error for AddRouteError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            AddRouteError::FailedToRegisterIngressRoute(err) => Some(err),
            _ => None,
        }
    }
}

impl Display for RemoveRouteError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            RemoveRouteError::SameAuthority => {
                write!(f, "ingress and egress authorities must differ")
            }
            RemoveRouteError::NotFound => write!(f, "route not found"),
        }
    }
}

impl Error for RemoveRouteError {}

/// Orchestrates multi-owner route transitions across control, routing, and data planes.
pub(crate) struct RouteLifecycle<'a> {
    route_table: &'a RouteTable,
    ingress_route_registry: &'a IngressRouteRegistry,
    subscription_directory: &'a SubscriptionDirectory,
}

impl<'a> RouteLifecycle<'a> {
    /// Creates a lifecycle coordinator using existing domain owners.
    pub(crate) fn new(
        route_table: &'a RouteTable,
        ingress_route_registry: &'a IngressRouteRegistry,
        subscription_directory: &'a SubscriptionDirectory,
    ) -> Self {
        Self {
            route_table,
            ingress_route_registry,
            subscription_directory,
        }
    }

    /// Registers a route and applies rollback when ingress registration fails.
    pub(crate) async fn add_route(
        &self,
        egress_route_pool: &mut EgressRoutePool,
        r#in: &Endpoint,
        out: &Endpoint,
        route_label: &str,
    ) -> Result<(), AddRouteError> {
        debug!(
            event = events::ROUTE_ADD_START,
            component = COMPONENT,
            route_label,
            in_authority = r#in.authority.as_str(),
            out_authority = out.authority.as_str(),
            "starting route add lifecycle"
        );

        if r#in.authority == out.authority {
            error!(
                event = events::ROUTE_ADD_FAILED,
                component = COMPONENT,
                route_label,
                in_authority = r#in.authority.as_str(),
                out_authority = out.authority.as_str(),
                reason = "same_authority",
                "route add rejected because authorities are equal"
            );
            return Err(AddRouteError::SameAuthority);
        }

        let route_key = RouteKey::from_endpoints(r#in, out);
        let inserted = self.route_table.insert_route(route_key.clone()).await;

        if !inserted {
            error!(
                event = events::ROUTE_ADD_FAILED,
                component = COMPONENT,
                route_label,
                in_authority = r#in.authority.as_str(),
                out_authority = out.authority.as_str(),
                reason = "already_exists",
                "route add failed because route already exists"
            );
            return Err(AddRouteError::AlreadyExists);
        }

        let out_sender = egress_route_pool
            .attach_route(out.transport.clone(), route_label)
            .await;

        if let Err(err) = self
            .ingress_route_registry
            .register_route(
                r#in.transport.clone(),
                &r#in.authority,
                &out.authority,
                route_label,
                out_sender,
                self.subscription_directory,
            )
            .await
        {
            self.route_table.remove_route(&route_key).await;
            egress_route_pool.detach_route(out.transport.clone()).await;

            error!(
                event = events::ROUTE_ADD_FAILED,
                component = COMPONENT,
                route_label,
                in_authority = r#in.authority.as_str(),
                out_authority = out.authority.as_str(),
                reason = "ingress_registration_failed",
                err = %err,
                "route add failed while registering ingress listener"
            );

            return Err(AddRouteError::FailedToRegisterIngressRoute(err));
        }

        debug!(
            event = events::ROUTE_ADD_OK,
            component = COMPONENT,
            route_label,
            in_authority = r#in.authority.as_str(),
            out_authority = out.authority.as_str(),
            "route add lifecycle completed"
        );

        Ok(())
    }

    /// Removes a route and detaches ingress/egress resources when present.
    pub(crate) async fn remove_route(
        &self,
        egress_route_pool: &mut EgressRoutePool,
        r#in: &Endpoint,
        out: &Endpoint,
        route_label: &str,
    ) -> Result<(), RemoveRouteError> {
        debug!(
            event = events::ROUTE_DELETE_START,
            component = COMPONENT,
            route_label,
            in_authority = r#in.authority.as_str(),
            out_authority = out.authority.as_str(),
            "starting route delete lifecycle"
        );

        if r#in.authority == out.authority {
            error!(
                event = events::ROUTE_DELETE_FAILED,
                component = COMPONENT,
                route_label,
                in_authority = r#in.authority.as_str(),
                out_authority = out.authority.as_str(),
                reason = "same_authority",
                "route delete rejected because authorities are equal"
            );
            return Err(RemoveRouteError::SameAuthority);
        }

        let route_key = RouteKey::from_endpoints(r#in, out);
        let removed = self.route_table.remove_route(&route_key).await;

        if !removed {
            error!(
                event = events::ROUTE_DELETE_FAILED,
                component = COMPONENT,
                route_label,
                in_authority = r#in.authority.as_str(),
                out_authority = out.authority.as_str(),
                reason = "not_found",
                "route delete failed because route was not found"
            );
            return Err(RemoveRouteError::NotFound);
        }

        egress_route_pool.detach_route(out.transport.clone()).await;
        self.ingress_route_registry
            .unregister_route(
                r#in.transport.clone(),
                &r#in.authority,
                &out.authority,
                self.subscription_directory,
            )
            .await;

        debug!(
            event = events::ROUTE_DELETE_OK,
            component = COMPONENT,
            route_label,
            in_authority = r#in.authority.as_str(),
            out_authority = out.authority.as_str(),
            "route delete lifecycle completed"
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{AddRouteError, RemoveRouteError};
    use crate::data_plane::ingress_registry::IngressRouteRegistrationError;
    use std::error::Error;

    #[test]
    fn add_route_error_exposes_display_and_source_for_ingress_failure() {
        let error = AddRouteError::FailedToRegisterIngressRoute(
            IngressRouteRegistrationError::FailedToRegisterRequestRouteListener,
        );

        assert!(error
            .to_string()
            .contains("failed to register ingress route listener"));
        assert!(error.source().is_some());
    }

    #[test]
    fn remove_route_error_display_is_stable_for_not_found() {
        let error = RemoveRouteError::NotFound;

        assert_eq!(error.to_string(), "route not found");
        assert!(error.source().is_none());
    }
}
