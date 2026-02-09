//! Authority-level URI filter helpers shared across route and data-plane internals.

use up_rust::UUri;

pub(crate) fn authority_to_wildcard_filter(authority_name: &str) -> UUri {
    UUri {
        authority_name: authority_name.to_string(),
        ue_id: 0xFFFF_FFFF,
        ue_version_major: 0xFF,
        resource_id: 0xFFFF,
        ..Default::default()
    }
}
