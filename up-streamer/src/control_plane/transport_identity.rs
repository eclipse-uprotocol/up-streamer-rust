//! Local transport identity keying used by route ownership.

use std::fmt::{Debug, Formatter};
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use up_rust::UTransport;

#[derive(Clone)]
pub(crate) struct TransportIdentityKey {
    transport: Arc<dyn UTransport>,
}

impl TransportIdentityKey {
    pub(crate) fn new(transport: Arc<dyn UTransport>) -> Self {
        Self { transport }
    }
}

impl Hash for TransportIdentityKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        Arc::as_ptr(&self.transport).hash(state);
    }
}

impl PartialEq for TransportIdentityKey {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.transport, &other.transport)
    }
}

impl Eq for TransportIdentityKey {}

impl Debug for TransportIdentityKey {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TransportIdentityKey")
            .finish_non_exhaustive()
    }
}
