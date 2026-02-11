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

use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use std::fs;
use std::path::{Path, PathBuf};
use tokio::process::Command;

pub const DEFAULT_SEND_COUNT: u64 = 12;
pub const DEFAULT_SEND_INTERVAL_MS: u64 = 1000;

pub const DEFAULT_ENDPOINT_CLAIM_MIN_COUNT: usize = 4;
pub const DEFAULT_EGRESS_SEND_ATTEMPT_MIN_COUNT: usize = 2;
pub const DEFAULT_EGRESS_SEND_OK_MIN_COUNT: usize = 2;
pub const DEFAULT_EGRESS_WORKER_MIN_COUNT: usize = 1;

pub const BROKER_READY_TIMEOUT_SECS: u64 = 30;
pub const STREAMER_READY_TIMEOUT_SECS: u64 = 60;
pub const PASSIVE_READY_TIMEOUT_SECS: u64 = 30;
pub const MQTT_HARD_TIMEOUT_SECS: u64 = 120;
pub const SOMEIP_HARD_TIMEOUT_SECS: u64 = 180;
pub const SIGINT_GRACE_SECS: u64 = 5;
pub const SIGTERM_GRACE_SECS: u64 = 5;
pub const LOG_POLL_INTERVAL_MS: u64 = 100;

pub const READY_STREAMER_INITIALIZED: &str = "READY streamer_initialized";
pub const READY_LISTENER_REGISTERED: &str = "READY listener_registered";

pub fn repo_root() -> Result<PathBuf> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .ancestors()
        .nth(2)
        .map(Path::to_path_buf)
        .ok_or_else(|| {
            anyhow!(
                "unable to resolve workspace root from {}",
                manifest_dir.display()
            )
        })
}

pub fn resolve_artifacts_root(repo_root: &Path, artifacts_root: Option<PathBuf>) -> PathBuf {
    artifacts_root.unwrap_or_else(|| repo_root.join("target").join("transport-smoke"))
}

pub fn resolve_expected_branch(expected_branch: Option<String>) -> Option<String> {
    expected_branch.or_else(|| {
        std::env::var("SMOKE_EXPECTED_BRANCH")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    })
}

pub fn scenario_timestamp() -> String {
    Utc::now().format("%Y%m%dT%H%M%SZ").to_string()
}

pub async fn enforce_expected_branch(
    repo_root: &Path,
    expected_branch: Option<&str>,
) -> Result<()> {
    let Some(expected_branch) = expected_branch else {
        return Ok(());
    };

    let output = Command::new("git")
        .arg("rev-parse")
        .arg("--abbrev-ref")
        .arg("HEAD")
        .current_dir(repo_root)
        .output()
        .await
        .context("failed to query current git branch")?;

    if !output.status.success() {
        return Err(anyhow!(
            "git branch query failed with status {}",
            output.status
        ));
    }

    let current_branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if current_branch != expected_branch {
        return Err(anyhow!(
            "branch guard mismatch: expected '{}', found '{}'. Pass --expected-branch or unset SMOKE_EXPECTED_BRANCH to control this guard",
            expected_branch,
            current_branch
        ));
    }

    Ok(())
}

pub fn ensure_paths_exist(repo_root: &Path, required_paths: &[&str]) -> Result<()> {
    for relative_path in required_paths {
        let candidate = repo_root.join(relative_path);
        if !candidate.exists() {
            return Err(anyhow!(
                "required path does not exist: {}",
                candidate.display()
            ));
        }
    }
    Ok(())
}

pub fn detect_vsomeip_runtime_lib(repo_root: &Path) -> Result<PathBuf> {
    let build_dir = repo_root.join("target").join("debug").join("build");
    if !build_dir.exists() {
        return Err(anyhow!(
            "missing build output directory for vsomeip lookup: {}",
            build_dir.display()
        ));
    }

    let mut candidates = Vec::new();
    for entry in fs::read_dir(&build_dir)
        .with_context(|| format!("unable to read build directory {}", build_dir.display()))?
    {
        let entry = entry?;
        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();
        if !file_name.starts_with("vsomeip-sys-") {
            continue;
        }

        let candidate = entry
            .path()
            .join("out")
            .join("vsomeip")
            .join("vsomeip-install")
            .join("lib");

        if candidate.exists() {
            let modified = fs::metadata(&candidate)
                .and_then(|metadata| metadata.modified())
                .with_context(|| {
                    format!(
                        "unable to inspect modified time for {}",
                        candidate.display()
                    )
                })?;
            candidates.push((modified, candidate));
        }
    }

    candidates.sort_by_key(|(modified, _)| *modified);
    candidates
        .pop()
        .map(|(_, path)| path)
        .ok_or_else(|| {
            anyhow!(
                "unable to locate bundled vsomeip runtime under target/debug/build/vsomeip-sys-*/out/vsomeip/vsomeip-install/lib"
            )
        })
}
