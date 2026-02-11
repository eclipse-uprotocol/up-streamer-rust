use clap::Parser;
use std::process::ExitCode;
use transport_smoke_suite::scenario::{run_scenario, ScenarioCliArgs};

const SCENARIO_ID: &str = "smoke-zenoh-someip-ps-zenoh-publisher-someip-subscriber";

#[derive(Debug, Parser)]
#[command(name = SCENARIO_ID)]
#[command(about = "Deterministic smoke scenario: zenoh publisher to someip subscriber")]
struct Cli {
    #[command(flatten)]
    common: ScenarioCliArgs,
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();

    match run_scenario(SCENARIO_ID, cli.common).await {
        Ok(result) => {
            if result.pass {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(error) => {
            eprintln!("{SCENARIO_ID} failed: {error:#}");
            ExitCode::from(2)
        }
    }
}
