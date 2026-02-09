use up_streamer::{Endpoint, UStreamer};
use usubscription_static_file::USubscriptionStaticFile;

const SUBSCRIPTION_CONFIG: &str = "../utils/usubscription-static-file/static-configs/testdata.json";

pub(crate) fn make_streamer(name: &str, message_queue_size: u16) -> UStreamer {
    let usubscription = std::sync::Arc::new(USubscriptionStaticFile::new(
        SUBSCRIPTION_CONFIG.to_string(),
    ));

    UStreamer::new(name, message_queue_size, usubscription)
        .expect("streamer creation should succeed")
}

pub(crate) async fn assert_add_rule_ok(streamer: &mut UStreamer, r#in: &Endpoint, out: &Endpoint) {
    assert!(streamer
        .add_forwarding_rule(r#in.clone(), out.clone())
        .await
        .is_ok());
}

#[allow(dead_code)]
pub(crate) async fn assert_delete_rule_ok(
    streamer: &mut UStreamer,
    r#in: &Endpoint,
    out: &Endpoint,
) {
    assert!(streamer
        .delete_forwarding_rule(r#in.clone(), out.clone())
        .await
        .is_ok());
}
