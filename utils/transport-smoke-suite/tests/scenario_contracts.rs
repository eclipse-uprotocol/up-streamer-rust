use transport_smoke_suite::claims::{
    default_claims_file_path, load_claims_for_scenario, ClaimCategory, Thresholds,
};
use transport_smoke_suite::env;
use transport_smoke_suite::scenario;

#[test]
fn scenario_registry_contains_exactly_eight_ids() {
    let scenario_ids = scenario::scenario_ids();
    assert_eq!(scenario_ids.len(), 8);
}

#[test]
fn each_scenario_has_claims_file_with_required_category_coverage() {
    let repo_root = env::repo_root().expect("resolve workspace root");

    for scenario_id in scenario::scenario_ids() {
        let claims_path = default_claims_file_path(&repo_root, scenario_id);
        assert!(
            claims_path.is_file(),
            "missing claims file for {} at {}",
            scenario_id,
            claims_path.display()
        );

        let loaded = load_claims_for_scenario(&repo_root, scenario_id, None, Thresholds::default())
            .unwrap_or_else(|_| panic!("unable to load claims for {}", scenario_id));

        assert!(
            loaded
                .claims
                .iter()
                .any(|claim| claim.category == ClaimCategory::EndpointCommunication),
            "{} claims file must include endpoint communication coverage",
            claims_path.display()
        );
        assert!(
            loaded
                .claims
                .iter()
                .any(|claim| claim.category == ClaimCategory::StreamerEgress),
            "{} claims file must include streamer egress coverage",
            claims_path.display()
        );
        assert!(
            loaded
                .claims
                .iter()
                .any(|claim| claim.category == ClaimCategory::ForbiddenSignature),
            "{} claims file must include forbidden signature coverage",
            claims_path.display()
        );
    }
}
