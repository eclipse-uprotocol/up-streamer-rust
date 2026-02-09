//! Public sync-health metadata for subscription refresh attempts.

use std::time::SystemTime;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SubscriptionSyncHealth {
    pub last_attempt_at: Option<SystemTime>,
    pub last_success_at: Option<SystemTime>,
    pub last_attempt_succeeded: Option<bool>,
    pub previous_attempt_succeeded: Option<bool>,
}
