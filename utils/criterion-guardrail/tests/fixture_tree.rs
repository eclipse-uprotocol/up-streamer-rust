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

use criterion_guardrail::{evaluate_guardrail, GuardrailInput, REQUIRED_BENCHMARK_IDS};
use std::path::PathBuf;

#[test]
fn checked_in_fixture_tree_supports_direct_and_fallback_layouts() {
    let criterion_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("criterion-sample-tree");

    let report = evaluate_guardrail(&GuardrailInput {
        criterion_root,
        baseline: "ergonomics_baseline".to_string(),
        candidate: "ergonomics_candidate".to_string(),
        throughput_threshold_pct: 3.0,
        latency_threshold_pct: 5.0,
        alloc_proxy_threshold_pct: 5.0,
    })
    .expect("fixture tree should parse");

    assert!(report.pass);
    assert_eq!(report.results.len(), REQUIRED_BENCHMARK_IDS.len());
}
