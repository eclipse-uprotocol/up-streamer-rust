//! Canonical structured event names used across `up-streamer`.

// Egress worker and pool events.
pub const EGRESS_SEND_ATTEMPT: &str = "egress_send_attempt";
pub const EGRESS_SEND_OK: &str = "egress_send_ok";
pub const EGRESS_SEND_FAILED: &str = "egress_send_failed";
pub const EGRESS_RECV_LAGGED: &str = "egress_recv_lagged";
pub const EGRESS_RECV_CLOSED: &str = "egress_recv_closed";
pub const EGRESS_WORKER_CREATE: &str = "egress_worker_create";
pub const EGRESS_WORKER_REUSE: &str = "egress_worker_reuse";
pub const EGRESS_WORKER_REMOVE: &str = "egress_worker_remove";

// Ingress and routing events.
pub const INGRESS_RECEIVE: &str = "ingress_receive";
pub const INGRESS_DROP_UNSUPPORTED_PAYLOAD: &str = "ingress_drop_unsupported_payload";
pub const INGRESS_SEND_TO_POOL_FAILED: &str = "ingress_send_to_pool_failed";
pub const INGRESS_REGISTER_REQUEST_LISTENER_OK: &str = "ingress_register_request_listener_ok";
pub const INGRESS_REGISTER_REQUEST_LISTENER_FAILED: &str =
    "ingress_register_request_listener_failed";
pub const INGRESS_REGISTER_PUBLISH_LISTENER_OK: &str = "ingress_register_publish_listener_ok";
pub const INGRESS_REGISTER_PUBLISH_LISTENER_FAILED: &str =
    "ingress_register_publish_listener_failed";
pub const INGRESS_UNREGISTER_REQUEST_LISTENER_OK: &str = "ingress_unregister_request_listener_ok";
pub const INGRESS_UNREGISTER_REQUEST_LISTENER_FAILED: &str =
    "ingress_unregister_request_listener_failed";
pub const INGRESS_UNREGISTER_PUBLISH_LISTENER_OK: &str = "ingress_unregister_publish_listener_ok";
pub const INGRESS_UNREGISTER_PUBLISH_LISTENER_FAILED: &str =
    "ingress_unregister_publish_listener_failed";
pub const SUBSCRIPTION_LOOKUP_EMPTY: &str = "subscription_lookup_empty";
pub const PUBLISH_SOURCE_FILTER_SKIPPED: &str = "publish_source_filter_skipped";
pub const PUBLISH_SOURCE_FILTER_BUILD_FAILED: &str = "publish_source_filter_build_failed";

// Control-plane lifecycle events.
pub const ROUTE_ADD_START: &str = "route_add_start";
pub const ROUTE_ADD_OK: &str = "route_add_ok";
pub const ROUTE_ADD_FAILED: &str = "route_add_failed";
pub const ROUTE_DELETE_START: &str = "route_delete_start";
pub const ROUTE_DELETE_OK: &str = "route_delete_ok";
pub const ROUTE_DELETE_FAILED: &str = "route_delete_failed";

// Runtime/cache observability events for key low-log modules.
pub const RUNTIME_THREAD_NAME_FALLBACK: &str = "runtime_thread_name_fallback";
pub const RUNTIME_SPAWN_START: &str = "runtime_spawn_start";
pub const RUNTIME_SPAWN_OK: &str = "runtime_spawn_ok";
pub const RUNTIME_SPAWN_FAILED: &str = "runtime_spawn_failed";
pub const SUBSCRIPTION_SNAPSHOT_REBUILD_START: &str = "subscription_snapshot_rebuild_start";
pub const SUBSCRIPTION_SNAPSHOT_REBUILD_OK: &str = "subscription_snapshot_rebuild_ok";
pub const SUBSCRIPTION_SNAPSHOT_REBUILD_FAILED: &str = "subscription_snapshot_rebuild_failed";
pub const SUBSCRIPTION_WILDCARD_MERGE_SUMMARY: &str = "subscription_wildcard_merge_summary";
