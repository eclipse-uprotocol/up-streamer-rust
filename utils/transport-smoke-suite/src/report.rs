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

use crate::claims::ClaimOutcome;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const SCENARIO_REPORT_SCHEMA_VERSION: &str = "1.1";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PhaseTiming {
    pub phase: String,
    pub start_ts: String,
    pub end_ts: String,
    pub duration_ms: u128,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProcessMetadata {
    pub name: String,
    pub pid: i32,
    pub process_group_id: i32,
    pub command: String,
    pub workdir: String,
    pub log_file: String,
    pub exit_status: Option<i32>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScenarioReport {
    pub schema_version: String,
    pub scenario_id: String,
    pub transport_family: String,
    pub pass: bool,
    pub exit_code: i32,
    pub phase_timings: Vec<PhaseTiming>,
    pub processes: Vec<ProcessMetadata>,
    pub claim_outcomes: Vec<ClaimOutcome>,
    pub forbidden_claim_outcomes: Vec<ClaimOutcome>,
    pub failure_reason: Option<String>,
    pub repro_command: String,
    pub artifact_dir: String,
    #[serde(default)]
    pub claims_source_path: Option<String>,
    pub start_ts: String,
    pub end_ts: String,
    pub duration_ms: u128,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MatrixScenarioSummary {
    pub scenario_id: String,
    pub pass: bool,
    pub exit_code: i32,
    pub artifact_dir: Option<String>,
    pub failure_reason: Option<String>,
    pub duration_ms: u128,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FailedScenarioSummary {
    pub scenario_id: String,
    pub failure_reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MatrixSummary {
    pub schema_version: String,
    pub selected_scenarios: Vec<String>,
    pub pass_count: usize,
    pub fail_count: usize,
    pub total_duration_ms: u128,
    pub scenarios: Vec<MatrixScenarioSummary>,
    pub failed_scenarios: Vec<FailedScenarioSummary>,
    pub aggregate_exit_rationale: String,
    pub start_ts: String,
    pub end_ts: String,
}

pub fn write_scenario_report(
    report: &ScenarioReport,
    artifacts_dir: &Path,
) -> Result<(PathBuf, PathBuf)> {
    fs::create_dir_all(artifacts_dir)
        .with_context(|| format!("unable to create artifact dir {}", artifacts_dir.display()))?;

    let scenario_report_json = artifacts_dir.join("scenario-report.json");
    let scenario_report_txt = artifacts_dir.join("scenario-report.txt");

    let json_payload = serde_json::to_string_pretty(report).context("serialize scenario report")?;
    fs::write(&scenario_report_json, json_payload).with_context(|| {
        format!(
            "unable to write scenario report JSON {}",
            scenario_report_json.display()
        )
    })?;

    fs::write(&scenario_report_txt, render_scenario_summary_text(report)).with_context(|| {
        format!(
            "unable to write scenario report text {}",
            scenario_report_txt.display()
        )
    })?;

    Ok((scenario_report_json, scenario_report_txt))
}

pub fn write_matrix_summary(
    summary: &MatrixSummary,
    artifacts_dir: &Path,
) -> Result<(PathBuf, PathBuf)> {
    fs::create_dir_all(artifacts_dir)
        .with_context(|| format!("unable to create artifact dir {}", artifacts_dir.display()))?;

    let matrix_summary_json = artifacts_dir.join("matrix-summary.json");
    let matrix_summary_txt = artifacts_dir.join("matrix-summary.txt");

    let json_payload = serde_json::to_string_pretty(summary).context("serialize matrix summary")?;
    fs::write(&matrix_summary_json, json_payload).with_context(|| {
        format!(
            "unable to write matrix summary JSON {}",
            matrix_summary_json.display()
        )
    })?;

    fs::write(&matrix_summary_txt, render_matrix_summary_text(summary)).with_context(|| {
        format!(
            "unable to write matrix summary text {}",
            matrix_summary_txt.display()
        )
    })?;

    Ok((matrix_summary_json, matrix_summary_txt))
}

pub fn load_scenario_report(report_path: &Path) -> Result<ScenarioReport> {
    let payload = fs::read_to_string(report_path)
        .with_context(|| format!("unable to read scenario report {}", report_path.display()))?;
    serde_json::from_str(&payload)
        .with_context(|| format!("unable to parse scenario report {}", report_path.display()))
}

pub fn timestamp_to_string(timestamp: DateTime<Utc>) -> String {
    timestamp.to_rfc3339()
}

fn render_scenario_summary_text(report: &ScenarioReport) -> String {
    let mut lines = vec![
        format!("scenario_id: {}", report.scenario_id),
        format!("transport_family: {}", report.transport_family),
        format!("status: {}", if report.pass { "PASS" } else { "FAIL" }),
        format!("exit_code: {}", report.exit_code),
        format!("artifact_dir: {}", report.artifact_dir),
        format!(
            "claims_source_path: {}",
            report
                .claims_source_path
                .as_deref()
                .unwrap_or("<not available>")
        ),
        format!("duration_ms: {}", report.duration_ms),
        format!("repro_command: {}", report.repro_command),
    ];

    if let Some(reason) = &report.failure_reason {
        lines.push(format!("failure_reason: {reason}"));
    }

    lines.push("phase_timings:".to_string());
    for phase in &report.phase_timings {
        lines.push(format!(
            "  - {} (duration_ms={})",
            phase.phase, phase.duration_ms
        ));
    }

    lines.push("claim_outcomes:".to_string());
    for outcome in report
        .claim_outcomes
        .iter()
        .chain(report.forbidden_claim_outcomes.iter())
    {
        lines.push(format!(
            "  - {}: {} (observed={}, min={}, pattern='{}')",
            outcome.claim_id,
            if outcome.pass { "PASS" } else { "FAIL" },
            outcome.observed_count,
            outcome.min_count,
            outcome.pattern
        ));
    }

    lines.join("\n")
}

fn render_matrix_summary_text(summary: &MatrixSummary) -> String {
    let mut lines = vec![
        format!(
            "selected_scenarios: {}",
            summary.selected_scenarios.join(", ")
        ),
        format!("pass_count: {}", summary.pass_count),
        format!("fail_count: {}", summary.fail_count),
        format!("total_duration_ms: {}", summary.total_duration_ms),
        format!(
            "aggregate_exit_rationale: {}",
            summary.aggregate_exit_rationale
        ),
    ];

    lines.push("scenarios:".to_string());
    for scenario in &summary.scenarios {
        lines.push(format!(
            "  - {}: {} (exit_code={}, duration_ms={}, artifact_dir={})",
            scenario.scenario_id,
            if scenario.pass { "PASS" } else { "FAIL" },
            scenario.exit_code,
            scenario.duration_ms,
            scenario
                .artifact_dir
                .as_deref()
                .unwrap_or("<not available>")
        ));
    }

    lines
        .into_iter()
        .chain(summary.failed_scenarios.iter().map(|failed| {
            format!(
                "failed: {} -> {}",
                failed.scenario_id, failed.failure_reason
            )
        }))
        .collect::<Vec<String>>()
        .join("\n")
}
