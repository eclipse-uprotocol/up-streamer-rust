use up_rust::UUri;

pub fn local_authority() -> String {
    "local_authority".to_string()
}

pub fn remote_authority_a() -> String {
    "remote_authority_a".to_string()
}

pub fn remote_authority_b() -> String {
    "remote_authority_b".to_string()
}

pub fn local_client_uuri(id: u32) -> UUri {
    UUri {
        authority_name: local_authority(),
        ue_id: id,
        ue_version_major: 1,
        resource_id: 2,
        ..Default::default()
    }
}

pub fn remote_client_uuri(authority: String, id: u32) -> UUri {
    UUri {
        authority_name: authority,
        ue_id: id,
        ue_version_major: 1,
        resource_id: 2,
        ..Default::default()
    }
}
