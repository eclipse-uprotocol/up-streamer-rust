/********************************************************************************
 * Copyright (c) 2026 Contributors to the Eclipse Foundation
 *
 * See the NOTICE file(s) distributed with this work for additional
 * information regarding copyright ownership.
 *
 * This program and the accompanying materials are made available under the
 * terms of the Apache License Version 2.0 which is available at
 * https://www.apache.org/licenses/LICENSE-2.0
 *
 * SPDX-License-Identifier: Apache-2.0
 ********************************************************************************/

use clap::Parser;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;
use tokio::process::Command;
use transport_smoke_suite::claims::{claims_override_kind, ClaimsPathKind};
use transport_smoke_suite::env;
use transport_smoke_suite::process::{run_shell_command, shell_escape};
use transport_smoke_suite::report::{
    self, FailedScenarioSummary, MatrixScenarioSummary, MatrixSummary, ScenarioReport,
};
use transport_smoke_suite::scenario;

#[derive(Debug, Parser)]
#[command(name = "transport-smoke-matrix")]
#[command(about = "Run deterministic transport smoke scenarios sequentially")]
struct Cli {
    #[arg(long)]
    all: bool,

    #[arg(long = "only")]
    only: Vec<String>,

    #[arg(long)]
    skip_build: bool,

    #[arg(long)]
    artifacts_root: Option<PathBuf>,

    #[arg(long)]
    claims_path: Option<PathBuf>,

    #[arg(long, default_value_t = env::DEFAULT_SEND_COUNT)]
    send_count: u64,

    #[arg(long, default_value_t = env::DEFAULT_SEND_INTERVAL_MS)]
    send_interval_ms: u64,

    #[arg(long)]
    scenario_timeout_secs: Option<u64>,

    #[arg(long)]
    expected_branch: Option<String>,

    #[arg(long)]
    no_bootstrap: bool,

    #[arg(long, default_value_t = env::DEFAULT_ENDPOINT_CLAIM_MIN_COUNT)]
    endpoint_claim_min_count: usize,

    #[arg(long, default_value_t = env::DEFAULT_EGRESS_SEND_ATTEMPT_MIN_COUNT)]
    egress_send_attempt_min_count: usize,

    #[arg(long, default_value_t = env::DEFAULT_EGRESS_SEND_OK_MIN_COUNT)]
    egress_send_ok_min_count: usize,

    #[arg(long, default_value_t = env::DEFAULT_EGRESS_WORKER_MIN_COUNT)]
    egress_worker_min_count: usize,
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();

    match run(cli).await {
        Ok(exit_code) => {
            if exit_code == 0 {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(error) => {
            eprintln!("transport-smoke-matrix failed: {error:#}");
            ExitCode::from(2)
        }
    }
}

async fn run(cli: Cli) -> anyhow::Result<i32> {
    let repo_root = env::repo_root()?;
    let expected_branch = env::resolve_expected_branch(cli.expected_branch.clone());
    env::enforce_expected_branch(&repo_root, expected_branch.as_deref()).await?;

    let selected_scenarios = resolve_selected_scenarios(&cli.only)?;
    if let Some(claims_path) = cli.claims_path.as_deref() {
        if claims_override_kind(&repo_root, claims_path)? == ClaimsPathKind::File
            && selected_scenarios.len() > 1
        {
            anyhow::bail!(
                "--claims-path points to a file but {} scenarios are selected. Use a directory override for multi-scenario matrix runs or select a single scenario with --only",
                selected_scenarios.len()
            );
        }
    }

    let artifacts_root = env::resolve_artifacts_root(&repo_root, cli.artifacts_root.clone());
    let matrix_artifact_dir = artifacts_root
        .join("matrix")
        .join(env::scenario_timestamp());
    fs::create_dir_all(&matrix_artifact_dir)?;

    if !cli.skip_build {
        let outcome = run_shell_command(
            &repo_root,
            &repo_root,
            "cargo build -p transport-smoke-suite --bins",
            cli.no_bootstrap,
        )
        .await?;
        if outcome.status_code != Some(0) {
            anyhow::bail!(
                "failed to build transport-smoke-suite binaries\ncommand: {}\nstdout:\n{}\nstderr:\n{}",
                outcome.command,
                outcome.stdout,
                outcome.stderr
            );
        }
    }

    let matrix_start_wall = chrono::Utc::now();
    let matrix_start_instant = Instant::now();

    let mut summaries = Vec::new();
    for scenario_id in &selected_scenarios {
        let scenario_start = Instant::now();
        println!("RUNNING {scenario_id}");

        let scenario_run = run_single_scenario(
            &repo_root,
            &artifacts_root,
            scenario_id,
            &cli,
            expected_branch.as_deref(),
        )
        .await;

        let duration_ms = scenario_start.elapsed().as_millis();
        let summary = match scenario_run {
            Ok(report) => build_summary_from_report(report, duration_ms),
            Err(error) => MatrixScenarioSummary {
                scenario_id: scenario_id.to_string(),
                pass: false,
                exit_code: 1,
                artifact_dir: None,
                failure_reason: Some(error.to_string()),
                duration_ms,
            },
        };

        println!(
            "{} -> {}",
            summary.scenario_id,
            if summary.pass { "PASS" } else { "FAIL" }
        );

        summaries.push(summary);
    }

    let pass_count = summaries.iter().filter(|summary| summary.pass).count();
    let fail_count = summaries.len() - pass_count;

    let failed_scenarios = summaries
        .iter()
        .filter(|summary| !summary.pass)
        .map(|summary| FailedScenarioSummary {
            scenario_id: summary.scenario_id.clone(),
            failure_reason: summary
                .failure_reason
                .clone()
                .unwrap_or_else(|| "unknown failure".to_string()),
        })
        .collect::<Vec<_>>();

    let aggregate_exit_rationale = if fail_count == 0 {
        "all selected scenarios passed".to_string()
    } else {
        format!(
            "{} of {} scenarios failed; matrix exits non-zero",
            fail_count,
            summaries.len()
        )
    };

    let matrix_end_wall = chrono::Utc::now();
    let matrix_summary = MatrixSummary {
        schema_version: "1.0".to_string(),
        selected_scenarios,
        pass_count,
        fail_count,
        total_duration_ms: matrix_start_instant.elapsed().as_millis(),
        scenarios: summaries,
        failed_scenarios,
        aggregate_exit_rationale,
        start_ts: report::timestamp_to_string(matrix_start_wall),
        end_ts: report::timestamp_to_string(matrix_end_wall),
    };

    let (matrix_summary_json, matrix_summary_txt) =
        report::write_matrix_summary(&matrix_summary, &matrix_artifact_dir)?;

    println!("MATRIX_ARTIFACT_DIR={}", matrix_artifact_dir.display());
    println!("MATRIX_SUMMARY_JSON={}", matrix_summary_json.display());
    println!("MATRIX_SUMMARY_TXT={}", matrix_summary_txt.display());

    Ok(if fail_count == 0 { 0 } else { 1 })
}

fn resolve_selected_scenarios(only: &[String]) -> anyhow::Result<Vec<String>> {
    if only.is_empty() {
        return Ok(scenario::scenario_ids()
            .iter()
            .map(|scenario_id| scenario_id.to_string())
            .collect());
    }

    let mut selected = Vec::new();
    for scenario_id in only {
        if scenario::scenario_template(scenario_id).is_none() {
            anyhow::bail!(
                "unknown scenario id '{}'; valid ids: {}",
                scenario_id,
                scenario::scenario_ids().join(", ")
            );
        }
        selected.push(scenario_id.to_string());
    }

    Ok(selected)
}

async fn run_single_scenario(
    repo_root: &Path,
    artifacts_root: &Path,
    scenario_id: &str,
    cli: &Cli,
    expected_branch: Option<&str>,
) -> anyhow::Result<ScenarioReport> {
    let binary_path = repo_root.join("target").join("debug").join(scenario_id);

    if !binary_path.exists() {
        anyhow::bail!(
            "scenario binary not found: {}. Build with `cargo build -p transport-smoke-suite --bins`",
            binary_path.display()
        );
    }

    let mut command = Command::new(&binary_path);
    command.current_dir(repo_root);
    command.arg("--artifacts-root").arg(artifacts_root);
    command.arg("--send-count").arg(cli.send_count.to_string());
    command
        .arg("--send-interval-ms")
        .arg(cli.send_interval_ms.to_string());
    if let Some(claims_path) = &cli.claims_path {
        command.arg("--claims-path").arg(claims_path);
    }
    command
        .arg("--endpoint-claim-min-count")
        .arg(cli.endpoint_claim_min_count.to_string());
    command
        .arg("--egress-send-attempt-min-count")
        .arg(cli.egress_send_attempt_min_count.to_string());
    command
        .arg("--egress-send-ok-min-count")
        .arg(cli.egress_send_ok_min_count.to_string());
    command
        .arg("--egress-worker-min-count")
        .arg(cli.egress_worker_min_count.to_string());

    if cli.skip_build {
        command.arg("--skip-build");
    }
    if cli.no_bootstrap {
        command.arg("--no-bootstrap");
    }
    if let Some(scenario_timeout_secs) = cli.scenario_timeout_secs {
        command
            .arg("--scenario-timeout-secs")
            .arg(scenario_timeout_secs.to_string());
    }
    if let Some(expected_branch) = expected_branch {
        command.arg("--expected-branch").arg(expected_branch);
    }

    let output = command.output().await.map_err(|error| {
        anyhow::anyhow!("unable to execute scenario '{}': {error}", scenario_id)
    })?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    let report_json_path = parse_scenario_report_path(&stdout)
        .or_else(|| parse_scenario_report_path(&stderr))
        .or_else(|| find_latest_scenario_report(artifacts_root, scenario_id));

    let Some(report_json_path) = report_json_path else {
        anyhow::bail!(
            "scenario '{}' did not emit SCENARIO_REPORT_JSON and no report was found under {}\nstdout:\n{}\nstderr:\n{}",
            scenario_id,
            shell_escape(&artifacts_root.display().to_string()),
            stdout,
            stderr
        );
    };

    let report = report::load_scenario_report(&report_json_path)?;
    if !output.status.success() && report.pass {
        anyhow::bail!(
            "scenario '{}' exited non-zero but report marked PASS\nstdout:\n{}\nstderr:\n{}",
            scenario_id,
            stdout,
            stderr
        );
    }

    Ok(report)
}

fn parse_scenario_report_path(output: &str) -> Option<PathBuf> {
    output
        .lines()
        .find_map(|line| line.strip_prefix("SCENARIO_REPORT_JSON="))
        .map(|value| PathBuf::from(value.trim()))
}

fn find_latest_scenario_report(artifacts_root: &Path, scenario_id: &str) -> Option<PathBuf> {
    let scenario_root = artifacts_root.join(scenario_id);
    if !scenario_root.exists() {
        return None;
    }

    let mut candidates = fs::read_dir(&scenario_root)
        .ok()?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let path = entry.path();
            if !path.is_dir() {
                return None;
            }
            let report_path = path.join("scenario-report.json");
            if !report_path.exists() {
                return None;
            }
            let directory_name = path.file_name()?.to_string_lossy().to_string();
            Some((directory_name, report_path))
        })
        .collect::<Vec<_>>();

    candidates.sort_by(|left, right| left.0.cmp(&right.0));
    candidates.pop().map(|(_, path)| path)
}

fn build_summary_from_report(report: ScenarioReport, duration_ms: u128) -> MatrixScenarioSummary {
    MatrixScenarioSummary {
        scenario_id: report.scenario_id,
        pass: report.pass,
        exit_code: report.exit_code,
        artifact_dir: Some(report.artifact_dir),
        failure_reason: report.failure_reason,
        duration_ms,
    }
}
