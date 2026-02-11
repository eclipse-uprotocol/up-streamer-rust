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

use std::path::PathBuf;
use transport_smoke_suite::claims::{evaluate_claims, load_claims_for_scenario, Thresholds};
use transport_smoke_suite::env;
use transport_smoke_suite::scenario;

#[test]
fn pass_fixtures_satisfy_scenario_claims() {
    let repo_root = env::repo_root().expect("resolve workspace root");

    for scenario_id in scenario::scenario_ids() {
        let loaded = load_claims_for_scenario(&repo_root, scenario_id, None, Thresholds::default())
            .unwrap_or_else(|_| panic!("unable to load claims for {}", scenario_id));
        let fixture_dir = fixture_root().join(scenario_id).join("pass");
        let outcomes = evaluate_claims(&fixture_dir, &loaded.claims);

        let failed = outcomes
            .into_iter()
            .filter(|outcome| !outcome.pass)
            .collect::<Vec<_>>();
        assert!(
            failed.is_empty(),
            "pass fixture failed for {} with {:?}",
            scenario_id,
            failed
        );
    }
}

#[test]
fn fail_fixtures_trigger_claim_failures() {
    let repo_root = env::repo_root().expect("resolve workspace root");

    for scenario_id in scenario::scenario_ids() {
        let loaded = load_claims_for_scenario(&repo_root, scenario_id, None, Thresholds::default())
            .unwrap_or_else(|_| panic!("unable to load claims for {}", scenario_id));
        let fixture_dir = fixture_root()
            .join(scenario_id)
            .join("fail-missing-egress-ok");
        let outcomes = evaluate_claims(&fixture_dir, &loaded.claims);

        assert!(
            outcomes.iter().any(|outcome| !outcome.pass),
            "expected at least one failing claim for {}",
            scenario_id
        );
    }
}

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}
