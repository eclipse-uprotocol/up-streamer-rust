use crate::claims::{evaluate_claims, load_claims_for_scenario, split_claim_outcomes, Thresholds};
use crate::env;
use crate::logs;
use crate::process::{run_shell_command, shell_escape, ManagedProcess, ProcessSpec};
use crate::report::{self, PhaseTiming, ProcessMetadata, ScenarioReport};
use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use clap::Args;
use std::cmp::min;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub const SCENARIO_IDS: [&str; 8] = [
    "smoke-zenoh-mqtt-rr-zenoh-client-mqtt-service",
    "smoke-zenoh-mqtt-rr-mqtt-client-zenoh-service",
    "smoke-zenoh-mqtt-ps-zenoh-publisher-mqtt-subscriber",
    "smoke-zenoh-mqtt-ps-mqtt-publisher-zenoh-subscriber",
    "smoke-zenoh-someip-rr-zenoh-client-someip-service",
    "smoke-zenoh-someip-rr-someip-client-zenoh-service",
    "smoke-zenoh-someip-ps-zenoh-publisher-someip-subscriber",
    "smoke-zenoh-someip-ps-someip-publisher-zenoh-subscriber",
];

const NO_ARGS: &[&str] = &[];
const MQTT_STREAMER_ENV: &[(&str, &str)] = &[(
    "RUST_LOG",
    "up_streamer=debug,up_transport_mqtt5=debug,configurable_streamer=debug",
)];
const SOMEIP_STREAMER_ENV: &[(&str, &str)] = &[(
    "RUST_LOG",
    "up_transport_vsomeip=trace,up_streamer=debug,up_linux_streamer=debug,example_streamer_uses=debug",
)];
const ACTIVE_DEBUG_ENV: &[(&str, &str)] = &[("RUST_LOG", "info,example_streamer_uses=debug")];
const PASSIVE_INFO_ENV: &[(&str, &str)] = &[("RUST_LOG", "info,example_streamer_uses=debug")];

const PASSIVE_MQTT_SUBSCRIBER_ARGS_A: &[&str] = &[
    "--uauthority",
    "authority-a",
    "--uentity",
    "0x5678",
    "--uversion",
    "0x1",
    "--resource",
    "0x1234",
    "--source-authority",
    "authority-b",
    "--source-uentity",
    "0x3039",
    "--source-uversion",
    "0x1",
    "--source-resource",
    "0x8001",
    "--broker-uri",
    "localhost:1883",
];

const PASSIVE_ZENOH_SUBSCRIBER_ARGS_B: &[&str] = &[
    "--uauthority",
    "authority-b",
    "--uentity",
    "0x5678",
    "--uversion",
    "0x1",
    "--resource",
    "0x1234",
    "--source-authority",
    "authority-a",
    "--source-uentity",
    "0x5BA0",
    "--source-uversion",
    "0x1",
    "--source-resource",
    "0x8001",
];

const PASSIVE_SOMEIP_SUBSCRIBER_ARGS_A: &[&str] = &[
    "--uauthority",
    "authority-a",
    "--uentity",
    "0x5678",
    "--uversion",
    "0x1",
    "--resource",
    "0x0",
    "--source-authority",
    "authority-b",
    "--source-uentity",
    "0x3039",
    "--source-uversion",
    "0x1",
    "--source-resource",
    "0x8001",
    "--remote-authority",
    "authority-b",
    "--vsomeip-config",
    "example-streamer-uses/vsomeip-configs/someip_client.json",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportFamily {
    Mqtt,
    Someip,
}

impl TransportFamily {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mqtt => "mqtt",
            Self::Someip => "someip",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ProcessTemplate {
    pub name: &'static str,
    pub workdir: &'static str,
    pub binary: &'static str,
    pub args: &'static [&'static str],
    pub env: &'static [(&'static str, &'static str)],
    pub log_file: &'static str,
    pub readiness_marker: Option<&'static str>,
    pub readiness_timeout_secs: Option<u64>,
    pub bounded_sender: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct ScenarioTemplate {
    pub id: &'static str,
    pub transport_family: TransportFamily,
    pub build_commands: &'static [&'static str],
    pub required_paths: &'static [&'static str],
    pub stale_process_signatures: &'static [&'static str],
    pub requires_docker: bool,
    pub requires_vsomeip_runtime: bool,
    pub streamer: ProcessTemplate,
    pub passive: ProcessTemplate,
    pub active: ProcessTemplate,
    pub hard_timeout_secs_default: u64,
}

const BUILD_MQTT_RR_ZENOH_CLIENT: &[&str] = &[
    "cargo build -p configurable-streamer",
    "cargo build -p example-streamer-uses --bin zenoh_client --features zenoh-transport",
    "cargo build -p example-streamer-uses --bin mqtt_service --features mqtt-transport",
];
const BUILD_MQTT_RR_MQTT_CLIENT: &[&str] = &[
    "cargo build -p configurable-streamer",
    "cargo build -p example-streamer-uses --bin mqtt_client --features mqtt-transport",
    "cargo build -p example-streamer-uses --bin zenoh_service --features zenoh-transport",
];
const BUILD_MQTT_PS_ZENOH_PUBLISHER: &[&str] = &[
    "cargo build -p configurable-streamer",
    "cargo build -p example-streamer-uses --bin zenoh_publisher --features zenoh-transport",
    "cargo build -p example-streamer-uses --bin mqtt_subscriber --features mqtt-transport",
];
const BUILD_MQTT_PS_MQTT_PUBLISHER: &[&str] = &[
    "cargo build -p configurable-streamer",
    "cargo build -p example-streamer-uses --bin mqtt_publisher --features mqtt-transport",
    "cargo build -p example-streamer-uses --bin zenoh_subscriber --features zenoh-transport",
];
const BUILD_SOMEIP_RR_ZENOH_CLIENT: &[&str] = &[
    "cargo build -p up-linux-streamer --bin zenoh_someip --features zenoh-transport,vsomeip-transport,bundled-vsomeip",
    "cargo build -p example-streamer-uses --bin zenoh_client --features zenoh-transport",
    "cargo build -p example-streamer-uses --bin someip_service --features vsomeip-transport,bundled-vsomeip",
];
const BUILD_SOMEIP_RR_SOMEIP_CLIENT: &[&str] = &[
    "cargo build -p up-linux-streamer --bin zenoh_someip --features zenoh-transport,vsomeip-transport,bundled-vsomeip",
    "cargo build -p example-streamer-uses --bin someip_client --features vsomeip-transport,bundled-vsomeip",
    "cargo build -p example-streamer-uses --bin zenoh_service --features zenoh-transport",
];
const BUILD_SOMEIP_PS_ZENOH_PUBLISHER: &[&str] = &[
    "cargo build -p up-linux-streamer --bin zenoh_someip --features zenoh-transport,vsomeip-transport,bundled-vsomeip",
    "cargo build -p example-streamer-uses --bin zenoh_publisher --features zenoh-transport",
    "cargo build -p example-streamer-uses --bin someip_subscriber --features vsomeip-transport,bundled-vsomeip",
];
const BUILD_SOMEIP_PS_SOMEIP_PUBLISHER: &[&str] = &[
    "cargo build -p up-linux-streamer --bin zenoh_someip --features zenoh-transport,vsomeip-transport,bundled-vsomeip",
    "cargo build -p example-streamer-uses --bin someip_publisher --features vsomeip-transport,bundled-vsomeip",
    "cargo build -p example-streamer-uses --bin zenoh_subscriber --features zenoh-transport",
];

const REQUIRED_MQTT_PATHS: &[&str] = &[
    "configurable-streamer/CONFIG.json5",
    "configurable-streamer/ZENOH_CONFIG.json5",
    "utils/mosquitto/docker-compose.yaml",
];
const REQUIRED_SOMEIP_PATHS: &[&str] = &[
    "example-streamer-implementations/DEFAULT_CONFIG.json5",
    "example-streamer-uses/vsomeip-configs/someip_client.json",
    "example-streamer-uses/vsomeip-configs/someip_publisher.json",
    "example-streamer-uses/vsomeip-configs/someip_service.json",
    "example-streamer-uses/vsomeip-configs/someip_subscriber.json",
];

const SCENARIO_MQTT_RR_ZENOH_CLIENT_MQTT_SERVICE: ScenarioTemplate = ScenarioTemplate {
    id: "smoke-zenoh-mqtt-rr-zenoh-client-mqtt-service",
    transport_family: TransportFamily::Mqtt,
    build_commands: BUILD_MQTT_RR_ZENOH_CLIENT,
    required_paths: REQUIRED_MQTT_PATHS,
    stale_process_signatures: &["configurable-streamer", "mqtt_service", "zenoh_client"],
    requires_docker: true,
    requires_vsomeip_runtime: false,
    streamer: ProcessTemplate {
        name: "streamer",
        workdir: "configurable-streamer",
        binary: "configurable-streamer",
        args: &["--config", "CONFIG.json5"],
        env: MQTT_STREAMER_ENV,
        log_file: "streamer.log",
        readiness_marker: Some(env::READY_STREAMER_INITIALIZED),
        readiness_timeout_secs: Some(env::STREAMER_READY_TIMEOUT_SECS),
        bounded_sender: false,
    },
    passive: ProcessTemplate {
        name: "service",
        workdir: ".",
        binary: "mqtt_service",
        args: &["--broker-uri", "localhost:1883"],
        env: PASSIVE_INFO_ENV,
        log_file: "service.log",
        readiness_marker: Some(env::READY_LISTENER_REGISTERED),
        readiness_timeout_secs: Some(env::PASSIVE_READY_TIMEOUT_SECS),
        bounded_sender: false,
    },
    active: ProcessTemplate {
        name: "client",
        workdir: ".",
        binary: "zenoh_client",
        args: NO_ARGS,
        env: ACTIVE_DEBUG_ENV,
        log_file: "client.log",
        readiness_marker: None,
        readiness_timeout_secs: None,
        bounded_sender: true,
    },
    hard_timeout_secs_default: env::MQTT_HARD_TIMEOUT_SECS,
};

const SCENARIO_MQTT_RR_MQTT_CLIENT_ZENOH_SERVICE: ScenarioTemplate = ScenarioTemplate {
    id: "smoke-zenoh-mqtt-rr-mqtt-client-zenoh-service",
    transport_family: TransportFamily::Mqtt,
    build_commands: BUILD_MQTT_RR_MQTT_CLIENT,
    required_paths: REQUIRED_MQTT_PATHS,
    stale_process_signatures: &["configurable-streamer", "zenoh_service", "mqtt_client"],
    requires_docker: true,
    requires_vsomeip_runtime: false,
    streamer: ProcessTemplate {
        name: "streamer",
        workdir: "configurable-streamer",
        binary: "configurable-streamer",
        args: &["--config", "CONFIG.json5"],
        env: MQTT_STREAMER_ENV,
        log_file: "streamer.log",
        readiness_marker: Some(env::READY_STREAMER_INITIALIZED),
        readiness_timeout_secs: Some(env::STREAMER_READY_TIMEOUT_SECS),
        bounded_sender: false,
    },
    passive: ProcessTemplate {
        name: "service",
        workdir: ".",
        binary: "zenoh_service",
        args: NO_ARGS,
        env: PASSIVE_INFO_ENV,
        log_file: "service.log",
        readiness_marker: Some(env::READY_LISTENER_REGISTERED),
        readiness_timeout_secs: Some(env::PASSIVE_READY_TIMEOUT_SECS),
        bounded_sender: false,
    },
    active: ProcessTemplate {
        name: "client",
        workdir: ".",
        binary: "mqtt_client",
        args: &["--broker-uri", "localhost:1883"],
        env: ACTIVE_DEBUG_ENV,
        log_file: "client.log",
        readiness_marker: None,
        readiness_timeout_secs: None,
        bounded_sender: true,
    },
    hard_timeout_secs_default: env::MQTT_HARD_TIMEOUT_SECS,
};

const SCENARIO_MQTT_PS_ZENOH_PUBLISHER_MQTT_SUBSCRIBER: ScenarioTemplate = ScenarioTemplate {
    id: "smoke-zenoh-mqtt-ps-zenoh-publisher-mqtt-subscriber",
    transport_family: TransportFamily::Mqtt,
    build_commands: BUILD_MQTT_PS_ZENOH_PUBLISHER,
    required_paths: REQUIRED_MQTT_PATHS,
    stale_process_signatures: &[
        "configurable-streamer",
        "mqtt_subscriber",
        "zenoh_publisher",
    ],
    requires_docker: true,
    requires_vsomeip_runtime: false,
    streamer: ProcessTemplate {
        name: "streamer",
        workdir: "configurable-streamer",
        binary: "configurable-streamer",
        args: &["--config", "CONFIG.json5"],
        env: MQTT_STREAMER_ENV,
        log_file: "streamer.log",
        readiness_marker: Some(env::READY_STREAMER_INITIALIZED),
        readiness_timeout_secs: Some(env::STREAMER_READY_TIMEOUT_SECS),
        bounded_sender: false,
    },
    passive: ProcessTemplate {
        name: "subscriber",
        workdir: ".",
        binary: "mqtt_subscriber",
        args: PASSIVE_MQTT_SUBSCRIBER_ARGS_A,
        env: PASSIVE_INFO_ENV,
        log_file: "subscriber.log",
        readiness_marker: Some(env::READY_LISTENER_REGISTERED),
        readiness_timeout_secs: Some(env::PASSIVE_READY_TIMEOUT_SECS),
        bounded_sender: false,
    },
    active: ProcessTemplate {
        name: "publisher",
        workdir: ".",
        binary: "zenoh_publisher",
        args: NO_ARGS,
        env: ACTIVE_DEBUG_ENV,
        log_file: "publisher.log",
        readiness_marker: None,
        readiness_timeout_secs: None,
        bounded_sender: true,
    },
    hard_timeout_secs_default: env::MQTT_HARD_TIMEOUT_SECS,
};

const SCENARIO_MQTT_PS_MQTT_PUBLISHER_ZENOH_SUBSCRIBER: ScenarioTemplate = ScenarioTemplate {
    id: "smoke-zenoh-mqtt-ps-mqtt-publisher-zenoh-subscriber",
    transport_family: TransportFamily::Mqtt,
    build_commands: BUILD_MQTT_PS_MQTT_PUBLISHER,
    required_paths: REQUIRED_MQTT_PATHS,
    stale_process_signatures: &[
        "configurable-streamer",
        "zenoh_subscriber",
        "mqtt_publisher",
    ],
    requires_docker: true,
    requires_vsomeip_runtime: false,
    streamer: ProcessTemplate {
        name: "streamer",
        workdir: "configurable-streamer",
        binary: "configurable-streamer",
        args: &["--config", "CONFIG.json5"],
        env: MQTT_STREAMER_ENV,
        log_file: "streamer.log",
        readiness_marker: Some(env::READY_STREAMER_INITIALIZED),
        readiness_timeout_secs: Some(env::STREAMER_READY_TIMEOUT_SECS),
        bounded_sender: false,
    },
    passive: ProcessTemplate {
        name: "subscriber",
        workdir: ".",
        binary: "zenoh_subscriber",
        args: PASSIVE_ZENOH_SUBSCRIBER_ARGS_B,
        env: PASSIVE_INFO_ENV,
        log_file: "subscriber.log",
        readiness_marker: Some(env::READY_LISTENER_REGISTERED),
        readiness_timeout_secs: Some(env::PASSIVE_READY_TIMEOUT_SECS),
        bounded_sender: false,
    },
    active: ProcessTemplate {
        name: "publisher",
        workdir: ".",
        binary: "mqtt_publisher",
        args: &["--broker-uri", "localhost:1883"],
        env: ACTIVE_DEBUG_ENV,
        log_file: "publisher.log",
        readiness_marker: None,
        readiness_timeout_secs: None,
        bounded_sender: true,
    },
    hard_timeout_secs_default: env::MQTT_HARD_TIMEOUT_SECS,
};

const SCENARIO_SOMEIP_RR_ZENOH_CLIENT_SOMEIP_SERVICE: ScenarioTemplate = ScenarioTemplate {
    id: "smoke-zenoh-someip-rr-zenoh-client-someip-service",
    transport_family: TransportFamily::Someip,
    build_commands: BUILD_SOMEIP_RR_ZENOH_CLIENT,
    required_paths: REQUIRED_SOMEIP_PATHS,
    stale_process_signatures: &["zenoh_someip", "someip_service", "zenoh_client"],
    requires_docker: false,
    requires_vsomeip_runtime: true,
    streamer: ProcessTemplate {
        name: "streamer",
        workdir: "example-streamer-implementations",
        binary: "zenoh_someip",
        args: &["--config", "DEFAULT_CONFIG.json5"],
        env: SOMEIP_STREAMER_ENV,
        log_file: "streamer.log",
        readiness_marker: Some(env::READY_STREAMER_INITIALIZED),
        readiness_timeout_secs: Some(env::STREAMER_READY_TIMEOUT_SECS),
        bounded_sender: false,
    },
    passive: ProcessTemplate {
        name: "service",
        workdir: ".",
        binary: "someip_service",
        args: NO_ARGS,
        env: PASSIVE_INFO_ENV,
        log_file: "service.log",
        readiness_marker: Some(env::READY_LISTENER_REGISTERED),
        readiness_timeout_secs: Some(env::PASSIVE_READY_TIMEOUT_SECS),
        bounded_sender: false,
    },
    active: ProcessTemplate {
        name: "client",
        workdir: ".",
        binary: "zenoh_client",
        args: NO_ARGS,
        env: ACTIVE_DEBUG_ENV,
        log_file: "client.log",
        readiness_marker: None,
        readiness_timeout_secs: None,
        bounded_sender: true,
    },
    hard_timeout_secs_default: env::SOMEIP_HARD_TIMEOUT_SECS,
};

const SCENARIO_SOMEIP_RR_SOMEIP_CLIENT_ZENOH_SERVICE: ScenarioTemplate = ScenarioTemplate {
    id: "smoke-zenoh-someip-rr-someip-client-zenoh-service",
    transport_family: TransportFamily::Someip,
    build_commands: BUILD_SOMEIP_RR_SOMEIP_CLIENT,
    required_paths: REQUIRED_SOMEIP_PATHS,
    stale_process_signatures: &["zenoh_someip", "zenoh_service", "someip_client"],
    requires_docker: false,
    requires_vsomeip_runtime: true,
    streamer: ProcessTemplate {
        name: "streamer",
        workdir: "example-streamer-implementations",
        binary: "zenoh_someip",
        args: &["--config", "DEFAULT_CONFIG.json5"],
        env: SOMEIP_STREAMER_ENV,
        log_file: "streamer.log",
        readiness_marker: Some(env::READY_STREAMER_INITIALIZED),
        readiness_timeout_secs: Some(env::STREAMER_READY_TIMEOUT_SECS),
        bounded_sender: false,
    },
    passive: ProcessTemplate {
        name: "service",
        workdir: ".",
        binary: "zenoh_service",
        args: NO_ARGS,
        env: PASSIVE_INFO_ENV,
        log_file: "service.log",
        readiness_marker: Some(env::READY_LISTENER_REGISTERED),
        readiness_timeout_secs: Some(env::PASSIVE_READY_TIMEOUT_SECS),
        bounded_sender: false,
    },
    active: ProcessTemplate {
        name: "client",
        workdir: ".",
        binary: "someip_client",
        args: NO_ARGS,
        env: ACTIVE_DEBUG_ENV,
        log_file: "client.log",
        readiness_marker: None,
        readiness_timeout_secs: None,
        bounded_sender: true,
    },
    hard_timeout_secs_default: env::SOMEIP_HARD_TIMEOUT_SECS,
};

const SCENARIO_SOMEIP_PS_ZENOH_PUBLISHER_SOMEIP_SUBSCRIBER: ScenarioTemplate = ScenarioTemplate {
    id: "smoke-zenoh-someip-ps-zenoh-publisher-someip-subscriber",
    transport_family: TransportFamily::Someip,
    build_commands: BUILD_SOMEIP_PS_ZENOH_PUBLISHER,
    required_paths: REQUIRED_SOMEIP_PATHS,
    stale_process_signatures: &["zenoh_someip", "someip_subscriber", "zenoh_publisher"],
    requires_docker: false,
    requires_vsomeip_runtime: true,
    streamer: ProcessTemplate {
        name: "streamer",
        workdir: "example-streamer-implementations",
        binary: "zenoh_someip",
        args: &["--config", "DEFAULT_CONFIG.json5"],
        env: SOMEIP_STREAMER_ENV,
        log_file: "streamer.log",
        readiness_marker: Some(env::READY_STREAMER_INITIALIZED),
        readiness_timeout_secs: Some(env::STREAMER_READY_TIMEOUT_SECS),
        bounded_sender: false,
    },
    passive: ProcessTemplate {
        name: "subscriber",
        workdir: ".",
        binary: "someip_subscriber",
        args: PASSIVE_SOMEIP_SUBSCRIBER_ARGS_A,
        env: PASSIVE_INFO_ENV,
        log_file: "subscriber.log",
        readiness_marker: Some(env::READY_LISTENER_REGISTERED),
        readiness_timeout_secs: Some(env::PASSIVE_READY_TIMEOUT_SECS),
        bounded_sender: false,
    },
    active: ProcessTemplate {
        name: "publisher",
        workdir: ".",
        binary: "zenoh_publisher",
        args: NO_ARGS,
        env: ACTIVE_DEBUG_ENV,
        log_file: "publisher.log",
        readiness_marker: None,
        readiness_timeout_secs: None,
        bounded_sender: true,
    },
    hard_timeout_secs_default: env::SOMEIP_HARD_TIMEOUT_SECS,
};

const SCENARIO_SOMEIP_PS_SOMEIP_PUBLISHER_ZENOH_SUBSCRIBER: ScenarioTemplate = ScenarioTemplate {
    id: "smoke-zenoh-someip-ps-someip-publisher-zenoh-subscriber",
    transport_family: TransportFamily::Someip,
    build_commands: BUILD_SOMEIP_PS_SOMEIP_PUBLISHER,
    required_paths: REQUIRED_SOMEIP_PATHS,
    stale_process_signatures: &["zenoh_someip", "zenoh_subscriber", "someip_publisher"],
    requires_docker: false,
    requires_vsomeip_runtime: true,
    streamer: ProcessTemplate {
        name: "streamer",
        workdir: "example-streamer-implementations",
        binary: "zenoh_someip",
        args: &["--config", "DEFAULT_CONFIG.json5"],
        env: SOMEIP_STREAMER_ENV,
        log_file: "streamer.log",
        readiness_marker: Some(env::READY_STREAMER_INITIALIZED),
        readiness_timeout_secs: Some(env::STREAMER_READY_TIMEOUT_SECS),
        bounded_sender: false,
    },
    passive: ProcessTemplate {
        name: "subscriber",
        workdir: ".",
        binary: "zenoh_subscriber",
        args: PASSIVE_ZENOH_SUBSCRIBER_ARGS_B,
        env: PASSIVE_INFO_ENV,
        log_file: "subscriber.log",
        readiness_marker: Some(env::READY_LISTENER_REGISTERED),
        readiness_timeout_secs: Some(env::PASSIVE_READY_TIMEOUT_SECS),
        bounded_sender: false,
    },
    active: ProcessTemplate {
        name: "publisher",
        workdir: ".",
        binary: "someip_publisher",
        args: NO_ARGS,
        env: ACTIVE_DEBUG_ENV,
        log_file: "publisher.log",
        readiness_marker: None,
        readiness_timeout_secs: None,
        bounded_sender: true,
    },
    hard_timeout_secs_default: env::SOMEIP_HARD_TIMEOUT_SECS,
};

#[derive(Debug, Clone, Args)]
pub struct ScenarioCliArgs {
    #[arg(long)]
    pub skip_build: bool,

    #[arg(long)]
    pub artifacts_root: Option<PathBuf>,

    #[arg(long)]
    pub claims_path: Option<PathBuf>,

    #[arg(long, default_value_t = env::DEFAULT_SEND_COUNT)]
    pub send_count: u64,

    #[arg(long, default_value_t = env::DEFAULT_SEND_INTERVAL_MS)]
    pub send_interval_ms: u64,

    #[arg(long)]
    pub scenario_timeout_secs: Option<u64>,

    #[arg(long)]
    pub expected_branch: Option<String>,

    #[arg(long)]
    pub no_bootstrap: bool,

    #[arg(long, default_value_t = env::DEFAULT_ENDPOINT_CLAIM_MIN_COUNT)]
    pub endpoint_claim_min_count: usize,

    #[arg(long, default_value_t = env::DEFAULT_EGRESS_SEND_ATTEMPT_MIN_COUNT)]
    pub egress_send_attempt_min_count: usize,

    #[arg(long, default_value_t = env::DEFAULT_EGRESS_SEND_OK_MIN_COUNT)]
    pub egress_send_ok_min_count: usize,

    #[arg(long, default_value_t = env::DEFAULT_EGRESS_WORKER_MIN_COUNT)]
    pub egress_worker_min_count: usize,
}

#[derive(Debug, Clone)]
pub struct ScenarioRunResult {
    pub pass: bool,
    pub exit_code: i32,
    pub artifact_dir: PathBuf,
    pub scenario_report_json: PathBuf,
    pub scenario_report_txt: PathBuf,
    pub failure_reason: Option<String>,
}

pub fn scenario_ids() -> &'static [&'static str] {
    &SCENARIO_IDS
}

pub fn scenario_template(scenario_id: &str) -> Option<&'static ScenarioTemplate> {
    match scenario_id {
        "smoke-zenoh-mqtt-rr-zenoh-client-mqtt-service" => {
            Some(&SCENARIO_MQTT_RR_ZENOH_CLIENT_MQTT_SERVICE)
        }
        "smoke-zenoh-mqtt-rr-mqtt-client-zenoh-service" => {
            Some(&SCENARIO_MQTT_RR_MQTT_CLIENT_ZENOH_SERVICE)
        }
        "smoke-zenoh-mqtt-ps-zenoh-publisher-mqtt-subscriber" => {
            Some(&SCENARIO_MQTT_PS_ZENOH_PUBLISHER_MQTT_SUBSCRIBER)
        }
        "smoke-zenoh-mqtt-ps-mqtt-publisher-zenoh-subscriber" => {
            Some(&SCENARIO_MQTT_PS_MQTT_PUBLISHER_ZENOH_SUBSCRIBER)
        }
        "smoke-zenoh-someip-rr-zenoh-client-someip-service" => {
            Some(&SCENARIO_SOMEIP_RR_ZENOH_CLIENT_SOMEIP_SERVICE)
        }
        "smoke-zenoh-someip-rr-someip-client-zenoh-service" => {
            Some(&SCENARIO_SOMEIP_RR_SOMEIP_CLIENT_ZENOH_SERVICE)
        }
        "smoke-zenoh-someip-ps-zenoh-publisher-someip-subscriber" => {
            Some(&SCENARIO_SOMEIP_PS_ZENOH_PUBLISHER_SOMEIP_SUBSCRIBER)
        }
        "smoke-zenoh-someip-ps-someip-publisher-zenoh-subscriber" => {
            Some(&SCENARIO_SOMEIP_PS_SOMEIP_PUBLISHER_ZENOH_SUBSCRIBER)
        }
        _ => None,
    }
}

pub async fn run_scenario(
    scenario_id: &str,
    cli_args: ScenarioCliArgs,
) -> Result<ScenarioRunResult> {
    let template = scenario_template(scenario_id)
        .ok_or_else(|| anyhow!("unknown scenario id '{}'", scenario_id))?;

    let repo_root = env::repo_root()?;
    let expected_branch = env::resolve_expected_branch(cli_args.expected_branch.clone());
    let artifacts_root = env::resolve_artifacts_root(&repo_root, cli_args.artifacts_root.clone());
    let artifact_dir = artifacts_root
        .join(template.id)
        .join(env::scenario_timestamp());
    std::fs::create_dir_all(&artifact_dir)
        .with_context(|| format!("unable to create artifact dir {}", artifact_dir.display()))?;

    let scenario_start_wall = Utc::now();
    let scenario_start_instant = Instant::now();
    let hard_timeout_secs = cli_args
        .scenario_timeout_secs
        .unwrap_or(template.hard_timeout_secs_default);
    let scenario_deadline = scenario_start_instant + Duration::from_secs(hard_timeout_secs);

    let thresholds = Thresholds {
        endpoint_communication_min_count: cli_args.endpoint_claim_min_count,
        egress_send_attempt_min_count: cli_args.egress_send_attempt_min_count,
        egress_send_ok_min_count: cli_args.egress_send_ok_min_count,
        egress_worker_create_or_reuse_min_count: cli_args.egress_worker_min_count,
    };

    let mut phase_timings = Vec::new();
    let mut failure_reason = None;
    let mut claim_outcomes = Vec::new();
    let mut forbidden_claim_outcomes = Vec::new();
    let mut loaded_claims = Vec::new();
    let mut claims_source_path: Option<PathBuf> = None;

    let mut streamer_process: Option<ManagedProcess> = None;
    let mut passive_process: Option<ManagedProcess> = None;
    let mut active_process: Option<ManagedProcess> = None;
    let mut mqtt_broker_started = false;
    let mut vsomeip_runtime_lib: Option<PathBuf> = None;

    let preflight_result = execute_phase("Preflight", &mut phase_timings, || async {
        ensure_remaining_timeout(scenario_deadline, "Preflight")?;

        env::enforce_expected_branch(&repo_root, expected_branch.as_deref()).await?;
        env::ensure_paths_exist(&repo_root, template.required_paths)?;

        let loaded = load_claims_for_scenario(
            &repo_root,
            template.id,
            cli_args.claims_path.as_deref(),
            thresholds,
        )?;
        claims_source_path = Some(loaded.source_path.clone());
        loaded_claims = loaded.claims;

        ensure_no_stale_processes(template.stale_process_signatures).await?;

        if template.requires_docker {
            assert_command_success(
                run_shell_command(&repo_root, &repo_root, "docker --version", true).await?,
                "docker --version",
            )?;
            assert_command_success(
                run_shell_command(&repo_root, &repo_root, "docker compose version", true).await?,
                "docker compose version",
            )?;
        }

        if template.requires_vsomeip_runtime {
            vsomeip_runtime_lib = Some(env::detect_vsomeip_runtime_lib(&repo_root)?);
        }

        if !cli_args.skip_build {
            for build_command in template.build_commands {
                let outcome =
                    run_shell_command(&repo_root, &repo_root, build_command, cli_args.no_bootstrap)
                        .await?;
                assert_command_success(outcome, build_command)?;
            }
        }

        Ok(())
    })
    .await;

    if let Err(error) = preflight_result {
        failure_reason = Some(error.to_string());
    }

    if failure_reason.is_none() {
        let start_infra_result = execute_phase("StartInfra", &mut phase_timings, || async {
            ensure_remaining_timeout(scenario_deadline, "StartInfra")?;

            if template.requires_docker {
                start_mqtt_broker(&repo_root, cli_args.no_bootstrap, scenario_deadline).await?;
                mqtt_broker_started = true;
            }

            streamer_process = Some(
                spawn_template_process(
                    &repo_root,
                    &artifact_dir,
                    template,
                    &template.streamer,
                    &cli_args,
                    vsomeip_runtime_lib.as_ref(),
                )
                .await?,
            );

            Ok(())
        })
        .await;

        if let Err(error) = start_infra_result {
            failure_reason = Some(error.to_string());
        }
    }

    if failure_reason.is_none() {
        let wait_streamer_ready_result =
            execute_phase("WaitStreamerReady", &mut phase_timings, || async {
                ensure_remaining_timeout(scenario_deadline, "WaitStreamerReady")?;

                let streamer = streamer_process
                    .as_ref()
                    .ok_or_else(|| anyhow!("streamer process missing before readiness wait"))?;
                let marker = template
                    .streamer
                    .readiness_marker
                    .ok_or_else(|| anyhow!("streamer readiness marker not configured"))?;

                let marker_timeout = Duration::from_secs(
                    template
                        .streamer
                        .readiness_timeout_secs
                        .unwrap_or(env::STREAMER_READY_TIMEOUT_SECS),
                );
                let timeout = min(
                    marker_timeout,
                    ensure_remaining_timeout(scenario_deadline, "WaitStreamerReady")?,
                );

                logs::wait_for_exact_marker(
                    &streamer.log_path,
                    marker,
                    timeout,
                    Duration::from_millis(env::LOG_POLL_INTERVAL_MS),
                )
                .await?;

                Ok(())
            })
            .await;

        if let Err(error) = wait_streamer_ready_result {
            failure_reason = Some(error.to_string());
        }
    }

    if failure_reason.is_none() {
        let start_passive_result = execute_phase("StartPassive", &mut phase_timings, || async {
            ensure_remaining_timeout(scenario_deadline, "StartPassive")?;

            passive_process = Some(
                spawn_template_process(
                    &repo_root,
                    &artifact_dir,
                    template,
                    &template.passive,
                    &cli_args,
                    vsomeip_runtime_lib.as_ref(),
                )
                .await?,
            );

            Ok(())
        })
        .await;

        if let Err(error) = start_passive_result {
            failure_reason = Some(error.to_string());
        }
    }

    if failure_reason.is_none() {
        let wait_passive_ready_result =
            execute_phase("WaitPassiveReady", &mut phase_timings, || async {
                ensure_remaining_timeout(scenario_deadline, "WaitPassiveReady")?;

                let passive = passive_process
                    .as_ref()
                    .ok_or_else(|| anyhow!("passive process missing before readiness wait"))?;
                let marker = template
                    .passive
                    .readiness_marker
                    .ok_or_else(|| anyhow!("passive readiness marker not configured"))?;

                let marker_timeout = Duration::from_secs(
                    template
                        .passive
                        .readiness_timeout_secs
                        .unwrap_or(env::PASSIVE_READY_TIMEOUT_SECS),
                );
                let timeout = min(
                    marker_timeout,
                    ensure_remaining_timeout(scenario_deadline, "WaitPassiveReady")?,
                );

                logs::wait_for_exact_marker(
                    &passive.log_path,
                    marker,
                    timeout,
                    Duration::from_millis(env::LOG_POLL_INTERVAL_MS),
                )
                .await?;

                Ok(())
            })
            .await;

        if let Err(error) = wait_passive_ready_result {
            failure_reason = Some(error.to_string());
        }
    }

    if failure_reason.is_none() {
        let start_active_result = execute_phase("StartActive", &mut phase_timings, || async {
            ensure_remaining_timeout(scenario_deadline, "StartActive")?;

            active_process = Some(
                spawn_template_process(
                    &repo_root,
                    &artifact_dir,
                    template,
                    &template.active,
                    &cli_args,
                    vsomeip_runtime_lib.as_ref(),
                )
                .await?,
            );

            let active = active_process
                .as_mut()
                .ok_or_else(|| anyhow!("active process missing after spawn"))?;
            let remaining = ensure_remaining_timeout(scenario_deadline, "StartActive")?;
            let exited = active.wait_with_timeout(remaining).await?;
            if !exited {
                return Err(anyhow!(
                    "scenario hard timeout reached while waiting for bounded active sender completion"
                ));
            }

            if active.exit_status_code != Some(0) {
                return Err(anyhow!(
                    "active process '{}' exited with status {:?}",
                    active.name,
                    active.exit_status_code
                ));
            }

            Ok(())
        })
        .await;

        if let Err(error) = start_active_result {
            failure_reason = Some(error.to_string());
        }
    }

    if failure_reason.is_none() {
        let validate_claims_result =
            execute_phase("ValidateClaims", &mut phase_timings, || async {
                ensure_remaining_timeout(scenario_deadline, "ValidateClaims")?;

                if loaded_claims.is_empty() {
                    return Err(anyhow!(
                        "no claims were loaded before validation for scenario '{}'",
                        template.id
                    ));
                }

                let outcomes = evaluate_claims(&artifact_dir, &loaded_claims);
                let (must_outcomes, forbidden_outcomes, first_failed_claim_reason) =
                    split_claim_outcomes(outcomes);

                claim_outcomes = must_outcomes;
                forbidden_claim_outcomes = forbidden_outcomes;

                let streamer = streamer_process
                    .as_ref()
                    .ok_or_else(|| anyhow!("streamer process missing during claim validation"))?;
                let passive = passive_process
                    .as_ref()
                    .ok_or_else(|| anyhow!("passive process missing during claim validation"))?;

                let streamer_marker_count =
                    logs::count_exact_marker(&streamer.log_path, env::READY_STREAMER_INITIALIZED)?;
                if streamer_marker_count != 1 {
                    return Err(anyhow!(
                        "streamer readiness marker '{}' observed {} times (expected exactly 1)",
                        env::READY_STREAMER_INITIALIZED,
                        streamer_marker_count
                    ));
                }

                let passive_marker_count =
                    logs::count_exact_marker(&passive.log_path, env::READY_LISTENER_REGISTERED)?;
                if passive_marker_count != 1 {
                    return Err(anyhow!(
                        "passive readiness marker '{}' observed {} times (expected exactly 1)",
                        env::READY_LISTENER_REGISTERED,
                        passive_marker_count
                    ));
                }

                if let Some(first_failed_claim_reason) = first_failed_claim_reason {
                    return Err(anyhow!(first_failed_claim_reason));
                }

                Ok(())
            })
            .await;

        if let Err(error) = validate_claims_result {
            failure_reason = Some(error.to_string());
        }
    }

    let teardown_result = execute_phase("Teardown", &mut phase_timings, || async {
        if let Some(active) = active_process.as_mut() {
            active
                .terminate_gracefully(
                    Duration::from_secs(env::SIGINT_GRACE_SECS),
                    Duration::from_secs(env::SIGTERM_GRACE_SECS),
                )
                .await?;
        }

        if let Some(passive) = passive_process.as_mut() {
            passive
                .terminate_gracefully(
                    Duration::from_secs(env::SIGINT_GRACE_SECS),
                    Duration::from_secs(env::SIGTERM_GRACE_SECS),
                )
                .await?;
        }

        if let Some(streamer) = streamer_process.as_mut() {
            streamer
                .terminate_gracefully(
                    Duration::from_secs(env::SIGINT_GRACE_SECS),
                    Duration::from_secs(env::SIGTERM_GRACE_SECS),
                )
                .await?;
        }

        if template.requires_docker && mqtt_broker_started {
            stop_mqtt_broker(&repo_root, cli_args.no_bootstrap).await?;
        }

        ensure_process_exited(active_process.as_mut(), "active")?;
        ensure_process_exited(passive_process.as_mut(), "passive")?;
        ensure_process_exited(streamer_process.as_mut(), "streamer")?;

        Ok(())
    })
    .await;

    if failure_reason.is_none() {
        if let Err(error) = teardown_result {
            failure_reason = Some(error.to_string());
        }
    }

    let finalize_report_result =
        execute_phase("FinalizeReport", &mut phase_timings, || async { Ok(()) }).await;
    if failure_reason.is_none() {
        if let Err(error) = finalize_report_result {
            failure_reason = Some(error.to_string());
        }
    }

    let scenario_end_wall = Utc::now();
    let scenario_duration_ms = scenario_start_instant.elapsed().as_millis();

    let process_metadata =
        gather_process_metadata(&streamer_process, &passive_process, &active_process);

    let pass = failure_reason.is_none()
        && claim_outcomes.iter().all(|outcome| outcome.pass)
        && forbidden_claim_outcomes.iter().all(|outcome| outcome.pass);
    let exit_code = if pass { 0 } else { 1 };

    let repro_command = render_repro_command(template.id, &cli_args, &artifact_dir);

    let scenario_report = ScenarioReport {
        schema_version: report::SCENARIO_REPORT_SCHEMA_VERSION.to_string(),
        scenario_id: template.id.to_string(),
        transport_family: template.transport_family.as_str().to_string(),
        pass,
        exit_code,
        phase_timings,
        processes: process_metadata,
        claim_outcomes,
        forbidden_claim_outcomes,
        failure_reason: failure_reason.clone(),
        repro_command,
        artifact_dir: artifact_dir.display().to_string(),
        claims_source_path: claims_source_path
            .as_ref()
            .map(|path| path.display().to_string()),
        start_ts: report::timestamp_to_string(scenario_start_wall),
        end_ts: report::timestamp_to_string(scenario_end_wall),
        duration_ms: scenario_duration_ms,
    };

    let (scenario_report_json, scenario_report_txt) =
        report::write_scenario_report(&scenario_report, &artifact_dir)?;

    println!("SCENARIO_ID={}", template.id);
    println!(
        "SCENARIO_STATUS={}",
        if scenario_report.pass { "PASS" } else { "FAIL" }
    );
    println!("SCENARIO_ARTIFACT_DIR={}", artifact_dir.display());
    println!("SCENARIO_REPORT_JSON={}", scenario_report_json.display());
    println!(
        "SCENARIO_CLAIMS_SOURCE_PATH={}",
        scenario_report
            .claims_source_path
            .as_deref()
            .unwrap_or("<not-resolved>")
    );

    Ok(ScenarioRunResult {
        pass: scenario_report.pass,
        exit_code,
        artifact_dir,
        scenario_report_json,
        scenario_report_txt,
        failure_reason,
    })
}

fn render_repro_command(
    scenario_id: &str,
    cli_args: &ScenarioCliArgs,
    artifact_dir: &Path,
) -> String {
    let mut args = vec![
        "cargo run -p transport-smoke-suite --bin".to_string(),
        scenario_id.to_string(),
        "--".to_string(),
        format!(
            "--artifacts-root {}",
            shell_escape(
                artifact_dir
                    .parent()
                    .unwrap_or(artifact_dir)
                    .display()
                    .to_string()
                    .as_str()
            )
        ),
        format!("--send-count {}", cli_args.send_count),
        format!("--send-interval-ms {}", cli_args.send_interval_ms),
    ];

    if cli_args.skip_build {
        args.push("--skip-build".to_string());
    }
    if cli_args.no_bootstrap {
        args.push("--no-bootstrap".to_string());
    }
    if let Some(claims_path) = &cli_args.claims_path {
        args.push(format!(
            "--claims-path {}",
            shell_escape(claims_path.display().to_string().as_str())
        ));
    }
    if let Some(expected_branch) = &cli_args.expected_branch {
        args.push(format!(
            "--expected-branch {}",
            shell_escape(expected_branch)
        ));
    }
    if let Some(scenario_timeout_secs) = cli_args.scenario_timeout_secs {
        args.push(format!("--scenario-timeout-secs {}", scenario_timeout_secs));
    }

    args.join(" ")
}

fn gather_process_metadata(
    streamer_process: &Option<ManagedProcess>,
    passive_process: &Option<ManagedProcess>,
    active_process: &Option<ManagedProcess>,
) -> Vec<ProcessMetadata> {
    [streamer_process, passive_process, active_process]
        .into_iter()
        .flatten()
        .map(|process| ProcessMetadata {
            name: process.name.clone(),
            pid: process.pid,
            process_group_id: process.process_group_id,
            command: process.command_line.clone(),
            workdir: process.workdir.display().to_string(),
            log_file: process.log_path.display().to_string(),
            exit_status: process.exit_status_code,
        })
        .collect()
}

fn ensure_process_exited(process: Option<&mut ManagedProcess>, role: &str) -> Result<()> {
    let Some(process) = process else {
        return Ok(());
    };

    if !process.has_exited() {
        return Err(anyhow!(
            "{} process '{}' remained alive after teardown",
            role,
            process.name
        ));
    }
    Ok(())
}

async fn start_mqtt_broker(repo_root: &Path, no_bootstrap: bool, deadline: Instant) -> Result<()> {
    let compose_path = repo_root
        .join("utils")
        .join("mosquitto")
        .join("docker-compose.yaml");
    let compose_path_quoted = shell_escape(compose_path.display().to_string().as_str());

    let down_command = format!("docker compose -f {compose_path_quoted} down --remove-orphans");
    let _ = run_shell_command(repo_root, repo_root, &down_command, no_bootstrap).await;

    let up_command = format!("docker compose -f {compose_path_quoted} up -d");
    let up_outcome = run_shell_command(repo_root, repo_root, &up_command, no_bootstrap).await?;
    assert_command_success(up_outcome, "docker compose up -d")?;

    let broker_deadline = Instant::now() + Duration::from_secs(env::BROKER_READY_TIMEOUT_SECS);
    loop {
        let poll_command =
            format!("docker compose -f {compose_path_quoted} ps --status running --services");
        let poll_outcome = run_shell_command(repo_root, repo_root, &poll_command, true).await?;
        if poll_outcome.status_code == Some(0)
            && poll_outcome
                .stdout
                .lines()
                .any(|line| line.trim() == "mosquitto")
        {
            return Ok(());
        }

        if Instant::now() >= broker_deadline {
            return Err(anyhow!(
                "timed out waiting for MQTT broker readiness (stdout='{}', stderr='{}')",
                poll_outcome.stdout.trim(),
                poll_outcome.stderr.trim()
            ));
        }
        if Instant::now() >= deadline {
            return Err(anyhow!(
                "scenario hard timeout reached while waiting for MQTT broker readiness"
            ));
        }

        tokio::time::sleep(Duration::from_millis(env::LOG_POLL_INTERVAL_MS)).await;
    }
}

async fn stop_mqtt_broker(repo_root: &Path, no_bootstrap: bool) -> Result<()> {
    let compose_path = repo_root
        .join("utils")
        .join("mosquitto")
        .join("docker-compose.yaml");
    let compose_path_quoted = shell_escape(compose_path.display().to_string().as_str());
    let down_command = format!("docker compose -f {compose_path_quoted} down --remove-orphans");

    let outcome = run_shell_command(repo_root, repo_root, &down_command, no_bootstrap).await?;
    assert_command_success(outcome, "docker compose down")
}

async fn spawn_template_process(
    repo_root: &Path,
    artifact_dir: &Path,
    scenario_template: &ScenarioTemplate,
    template: &ProcessTemplate,
    cli_args: &ScenarioCliArgs,
    vsomeip_runtime_lib: Option<&PathBuf>,
) -> Result<ManagedProcess> {
    let mut args = template
        .args
        .iter()
        .map(|arg| arg.to_string())
        .collect::<Vec<_>>();
    if template.bounded_sender {
        args.push("--send-count".to_string());
        args.push(cli_args.send_count.to_string());
        args.push("--send-interval-ms".to_string());
        args.push(cli_args.send_interval_ms.to_string());
    }

    let mut env_pairs = template
        .env
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect::<Vec<_>>();

    if scenario_template.requires_vsomeip_runtime {
        let runtime_lib = vsomeip_runtime_lib.ok_or_else(|| {
            anyhow!("vsomeip runtime library was required but no path was resolved")
        })?;
        let existing_ld_library_path = std::env::var("LD_LIBRARY_PATH").unwrap_or_default();
        let merged_ld_library_path = if existing_ld_library_path.is_empty() {
            runtime_lib.display().to_string()
        } else {
            format!("{}:{existing_ld_library_path}", runtime_lib.display())
        };
        env_pairs.push(("LD_LIBRARY_PATH".to_string(), merged_ld_library_path));
    }

    let process_spec = ProcessSpec {
        name: template.name.to_string(),
        workdir: repo_root.join(template.workdir),
        executable: repo_root.join("target").join("debug").join(template.binary),
        args,
        env: env_pairs,
        log_file_name: template.log_file.to_string(),
    };

    ManagedProcess::spawn(process_spec, repo_root, artifact_dir, cli_args.no_bootstrap).await
}

async fn ensure_no_stale_processes(signatures: &[&str]) -> Result<()> {
    if signatures.is_empty() {
        return Ok(());
    }

    let pattern = signatures.join("|");
    let output = tokio::process::Command::new("pgrep")
        .arg("-fa")
        .arg(&pattern)
        .output()
        .await
        .context("failed to run stale-process preflight check")?;

    if output.status.code() == Some(1) {
        return Ok(());
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !stdout.is_empty() {
        return Err(anyhow!(
            "stale process check failed; matching processes still running: {}",
            stdout
        ));
    }

    Err(anyhow!(
        "stale process check failed with status {:?}: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    ))
}

fn assert_command_success(outcome: crate::process::CommandOutcome, label: &str) -> Result<()> {
    if outcome.status_code == Some(0) {
        return Ok(());
    }

    Err(anyhow!(
        "{} failed (status={:?})\ncommand: {}\nstdout:\n{}\nstderr:\n{}",
        label,
        outcome.status_code,
        outcome.command,
        outcome.stdout,
        outcome.stderr
    ))
}

fn ensure_remaining_timeout(deadline: Instant, phase: &str) -> Result<Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .ok_or_else(|| anyhow!("scenario hard timeout reached in phase '{phase}'"))
}

async fn execute_phase<F, Fut>(
    phase_name: &str,
    phase_timings: &mut Vec<PhaseTiming>,
    f: F,
) -> Result<()>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<()>>,
{
    let phase_start_wall = Utc::now();
    let phase_start_instant = Instant::now();
    let result = f().await;
    let phase_end_wall = Utc::now();

    phase_timings.push(PhaseTiming {
        phase: phase_name.to_string(),
        start_ts: report::timestamp_to_string(phase_start_wall),
        end_ts: report::timestamp_to_string(phase_end_wall),
        duration_ms: phase_start_instant.elapsed().as_millis(),
    });

    result
}

pub fn scenario_ids_for_transport(transport_family: TransportFamily) -> Vec<&'static str> {
    SCENARIO_IDS
        .iter()
        .copied()
        .filter(|scenario_id| {
            scenario_template(scenario_id)
                .map(|template| template.transport_family == transport_family)
                .unwrap_or(false)
        })
        .collect()
}
