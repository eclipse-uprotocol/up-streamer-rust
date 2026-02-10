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
