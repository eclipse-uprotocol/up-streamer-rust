use crate::local_client_uuri;
use up_rust::UMessageType::{
    UMESSAGE_TYPE_NOTIFICATION, UMESSAGE_TYPE_PUBLISH, UMESSAGE_TYPE_REQUEST,
    UMESSAGE_TYPE_RESPONSE,
};
use up_rust::{UAttributes, UMessage, UUri};

pub fn publish_from_local_client_for_remote_client(local_id: u32) -> UMessage {
    UMessage {
        attributes: Some(UAttributes {
            source: Some(local_client_uuri(local_id)).into(),
            type_: UMESSAGE_TYPE_PUBLISH.into(),
            ..Default::default()
        })
        .into(),
        ..Default::default()
    }
}

pub fn notification_from_local_client_for_remote_client(
    local_id: u32,
    remote_uuri: UUri,
) -> UMessage {
    UMessage {
        attributes: Some(UAttributes {
            source: Some(local_client_uuri(local_id)).into(),
            sink: Some(remote_uuri).into(),
            type_: UMESSAGE_TYPE_NOTIFICATION.into(),
            ..Default::default()
        })
        .into(),
        ..Default::default()
    }
}

pub fn request_from_local_client_for_remote_client(local_id: u32, remote_uuri: UUri) -> UMessage {
    UMessage {
        attributes: Some(UAttributes {
            source: Some(local_client_uuri(local_id)).into(),
            sink: Some(remote_uuri).into(),
            type_: UMESSAGE_TYPE_REQUEST.into(),
            ..Default::default()
        })
        .into(),
        ..Default::default()
    }
}

pub fn response_from_local_client_for_remote_client(local_id: u32, remote_uuri: UUri) -> UMessage {
    UMessage {
        attributes: Some(UAttributes {
            source: Some(local_client_uuri(local_id)).into(),
            sink: Some(remote_uuri).into(),
            type_: UMESSAGE_TYPE_RESPONSE.into(),
            ..Default::default()
        })
        .into(),
        ..Default::default()
    }
}

pub fn publish_from_remote_client_for_local_client(remote_uuri: UUri) -> UMessage {
    UMessage {
        attributes: Some(UAttributes {
            source: Some(remote_uuri).into(),
            type_: UMESSAGE_TYPE_PUBLISH.into(),
            ..Default::default()
        })
        .into(),
        ..Default::default()
    }
}

pub fn notification_from_remote_client_for_local_client(
    remote_uuri: UUri,
    local_id: u32,
) -> UMessage {
    UMessage {
        attributes: Some(UAttributes {
            source: Some(remote_uuri).into(),
            sink: Some(local_client_uuri(local_id)).into(),
            type_: UMESSAGE_TYPE_NOTIFICATION.into(),
            ..Default::default()
        })
        .into(),
        ..Default::default()
    }
}

pub fn request_from_remote_client_for_local_client(remote_uuri: UUri, local_id: u32) -> UMessage {
    UMessage {
        attributes: Some(UAttributes {
            source: Some(remote_uuri).into(),
            sink: Some(local_client_uuri(local_id)).into(),
            type_: UMESSAGE_TYPE_REQUEST.into(),
            ..Default::default()
        })
        .into(),
        ..Default::default()
    }
}

pub fn response_from_remote_client_for_local_client(remote_uuri: UUri, local_id: u32) -> UMessage {
    UMessage {
        attributes: Some(UAttributes {
            source: Some(remote_uuri).into(),
            sink: Some(local_client_uuri(local_id)).into(),
            type_: UMESSAGE_TYPE_RESPONSE.into(),
            ..Default::default()
        })
        .into(),
        ..Default::default()
    }
}
