use log::trace;
use std::fs::canonicalize;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use up_rust::UStatus;
use up_rust::UTransport;

use std::str::FromStr;
use up_streamer::{Endpoint, UStreamer};
use up_transport_vsomeip::UPTransportVsomeip;
use up_transport_zenoh::UPClientZenoh;
use usubscription_static_file::USubscriptionStaticFile;
use zenoh::config::{Config, EndPoint};

#[tokio::main]
async fn main() -> Result<(), UStatus> {
    std::env::set_var("RUST_LOG", "debug");
    env_logger::init();

    let usubscription = Arc::new(USubscriptionStaticFile::new(Some(PathBuf::from(
        "example-utils/usubscription-static-file/static-configs/testdata.json",
    ))));
    let mut streamer = UStreamer::new("up-linux-streamer", 10000, usubscription);

    let crate_dir = env!("CARGO_MANIFEST_DIR");
    // TODO: Make configurable to pass the path to the vsomeip config as a command line argument
    let vsomeip_config = PathBuf::from(crate_dir).join("vsomeip-configs/point_to_point.json");
    let vsomeip_config = canonicalize(vsomeip_config).ok();
    trace!("vsomeip_config: {vsomeip_config:?}");

    // There will be a single vsomeip_transport, as there is a connection into device and a streamer
    // TODO: Add error handling if we fail to create a UPTransportVsomeip
    let vsomeip_transport: Arc<dyn UTransport> = Arc::new(
        UPTransportVsomeip::new_with_config(&"linux".to_string(), 10, &vsomeip_config.unwrap())
            .unwrap(),
    );

    // TODO: Probably make somewhat configurable?
    // Create a configuration object
    let mut zenoh_config = Config::default();

    // Specify the address to listen on using IPv4
    let ipv4_endpoint = EndPoint::from_str("tcp/0.0.0.0:7447");

    // Add the IPv4 endpoint to the Zenoh configuration
    zenoh_config
        .listen
        .endpoints
        .push(ipv4_endpoint.expect("FAIL"));
    // TODO: Add error handling if we fail to create a UPClientZenoh
    let zenoh_transport: Arc<dyn UTransport> = Arc::new(
        UPClientZenoh::new(zenoh_config, "linux".to_string())
            .await
            .unwrap(),
    );
    // TODO: Make configurable to pass the name of the mE authority as a  command line argument
    let vsomeip_endpoint = Endpoint::new(
        "vsomeip_endpoint",
        "me_authority",
        vsomeip_transport.clone(),
    );

    // TODO: Make configurable the ability to have perhaps a config file we pass in that has all the
    //  relevant authorities over Zenoh that should be forwarded
    let zenoh_transport_endpoint_a = Endpoint::new(
        "zenoh_transport_endpoint_a",
        "linux", // simple initial case of streamer + intended high compute destination on same device
        zenoh_transport.clone(),
    );

    // TODO: Per Zenoh endpoint configured, run these two rules
    streamer
        .add_forwarding_rule(vsomeip_endpoint.clone(), zenoh_transport_endpoint_a.clone())
        .await?;
    streamer
        .add_forwarding_rule(zenoh_transport_endpoint_a.clone(), vsomeip_endpoint.clone())
        .await?;

    thread::park();

    Ok(())
}
