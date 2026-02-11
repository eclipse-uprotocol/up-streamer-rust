use crate::env;
use anyhow::{anyhow, Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

pub const DEFAULT_CLAIMS_DIRECTORY: &str = "utils/transport-smoke-suite/claims";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClaimsPathKind {
    File,
    Directory,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimKind {
    MustMatch,
    MustNotMatch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimCategory {
    EndpointCommunication,
    StreamerEgress,
    ForbiddenSignature,
    Readiness,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThresholdSelector {
    EndpointCommunication,
    EgressSendAttempt,
    EgressSendOk,
    EgressWorkerCreateOrReuse,
    Fixed(usize),
}

#[derive(Clone, Copy, Debug)]
pub struct ClaimTemplate {
    pub claim_id: &'static str,
    pub category: ClaimCategory,
    pub kind: ClaimKind,
    pub file: &'static str,
    pub pattern: &'static str,
    pub threshold: ThresholdSelector,
}

impl ClaimTemplate {
    pub const fn must_match(
        claim_id: &'static str,
        category: ClaimCategory,
        file: &'static str,
        pattern: &'static str,
        threshold: ThresholdSelector,
    ) -> Self {
        Self {
            claim_id,
            category,
            kind: ClaimKind::MustMatch,
            file,
            pattern,
            threshold,
        }
    }

    pub const fn must_not_match(
        claim_id: &'static str,
        category: ClaimCategory,
        file: &'static str,
        pattern: &'static str,
    ) -> Self {
        Self {
            claim_id,
            category,
            kind: ClaimKind::MustNotMatch,
            file,
            pattern,
            threshold: ThresholdSelector::Fixed(0),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Thresholds {
    pub endpoint_communication_min_count: usize,
    pub egress_send_attempt_min_count: usize,
    pub egress_send_ok_min_count: usize,
    pub egress_worker_create_or_reuse_min_count: usize,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            endpoint_communication_min_count: env::DEFAULT_ENDPOINT_CLAIM_MIN_COUNT,
            egress_send_attempt_min_count: env::DEFAULT_EGRESS_SEND_ATTEMPT_MIN_COUNT,
            egress_send_ok_min_count: env::DEFAULT_EGRESS_SEND_OK_MIN_COUNT,
            egress_worker_create_or_reuse_min_count: env::DEFAULT_EGRESS_WORKER_MIN_COUNT,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ClaimSpec {
    pub claim_id: String,
    pub category: ClaimCategory,
    pub kind: ClaimKind,
    pub file: String,
    pub pattern: String,
    pub min_count: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClaimOutcome {
    pub claim_id: String,
    pub category: ClaimCategory,
    pub kind: ClaimKind,
    pub file: String,
    pub pattern: String,
    pub min_count: usize,
    pub observed_count: usize,
    pub pass: bool,
    pub first_match: Option<String>,
    pub error: Option<String>,
}

#[derive(Clone, Debug)]
pub struct LoadedClaims {
    pub source_path: PathBuf,
    pub schema_version: String,
    pub claims: Vec<ClaimSpec>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ClaimsFile {
    schema_version: String,
    scenario_id: String,
    claims: Vec<ClaimsFileEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ClaimsFileEntry {
    claim_id: String,
    category: ClaimCategory,
    kind: ClaimKind,
    file: String,
    pattern: String,
    #[serde(default)]
    threshold: Option<ThresholdValue>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum ThresholdValue {
    Selector(ThresholdSelectorKey),
    Fixed(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ThresholdSelectorKey {
    EndpointCommunication,
    EgressSendAttempt,
    EgressSendOk,
    EgressWorkerCreateOrReuse,
}

pub fn materialize_claims(
    claim_templates: &[ClaimTemplate],
    thresholds: Thresholds,
) -> Vec<ClaimSpec> {
    claim_templates
        .iter()
        .map(|claim_template| ClaimSpec {
            claim_id: claim_template.claim_id.to_string(),
            category: claim_template.category,
            kind: claim_template.kind,
            file: claim_template.file.to_string(),
            pattern: claim_template.pattern.to_string(),
            min_count: resolve_threshold(claim_template.threshold, thresholds),
        })
        .collect()
}

fn resolve_threshold(selector: ThresholdSelector, thresholds: Thresholds) -> usize {
    match selector {
        ThresholdSelector::EndpointCommunication => thresholds.endpoint_communication_min_count,
        ThresholdSelector::EgressSendAttempt => thresholds.egress_send_attempt_min_count,
        ThresholdSelector::EgressSendOk => thresholds.egress_send_ok_min_count,
        ThresholdSelector::EgressWorkerCreateOrReuse => {
            thresholds.egress_worker_create_or_reuse_min_count
        }
        ThresholdSelector::Fixed(value) => value,
    }
}

pub fn default_claims_file_path(repo_root: &Path, scenario_id: &str) -> PathBuf {
    repo_root
        .join(DEFAULT_CLAIMS_DIRECTORY)
        .join(format!("{scenario_id}.json"))
}

pub fn claims_override_kind(repo_root: &Path, claims_path: &Path) -> Result<ClaimsPathKind> {
    let candidate = resolve_override_path(repo_root, claims_path);
    let metadata = fs::metadata(&candidate).with_context(|| {
        format!(
            "claims path does not exist or is not accessible: {}",
            candidate.display()
        )
    })?;

    if metadata.is_dir() {
        Ok(ClaimsPathKind::Directory)
    } else if metadata.is_file() {
        Ok(ClaimsPathKind::File)
    } else {
        Err(anyhow!(
            "claims path must be a file or directory: {}",
            candidate.display()
        ))
    }
}

pub fn resolve_claims_file_path(
    repo_root: &Path,
    scenario_id: &str,
    claims_path_override: Option<&Path>,
) -> Result<PathBuf> {
    let candidate = match claims_path_override {
        None => default_claims_file_path(repo_root, scenario_id),
        Some(claims_path_override) => {
            let claims_path_override = resolve_override_path(repo_root, claims_path_override);
            let metadata = fs::metadata(&claims_path_override).with_context(|| {
                format!(
                    "claims path does not exist or is not accessible: {}",
                    claims_path_override.display()
                )
            })?;

            if metadata.is_dir() {
                claims_path_override.join(format!("{scenario_id}.json"))
            } else if metadata.is_file() {
                claims_path_override
            } else {
                return Err(anyhow!(
                    "claims path must be a file or directory: {}",
                    claims_path_override.display()
                ));
            }
        }
    };

    let metadata = fs::metadata(&candidate).with_context(|| {
        format!(
            "resolved claims file does not exist or is not accessible: {}",
            candidate.display()
        )
    })?;

    if !metadata.is_file() {
        return Err(anyhow!(
            "resolved claims path is not a file: {}",
            candidate.display()
        ));
    }

    Ok(candidate)
}

pub fn load_claims_for_scenario(
    repo_root: &Path,
    scenario_id: &str,
    claims_path_override: Option<&Path>,
    thresholds: Thresholds,
) -> Result<LoadedClaims> {
    let source_path = resolve_claims_file_path(repo_root, scenario_id, claims_path_override)?;
    let payload = fs::read_to_string(&source_path)
        .with_context(|| format!("unable to read claims file {}", source_path.display()))?;

    let claims_file: ClaimsFile = serde_json::from_str(&payload)
        .with_context(|| format!("unable to parse claims file {}", source_path.display()))?;

    if claims_file.schema_version.trim().is_empty() {
        return Err(anyhow!(
            "claims file {} has an empty schema_version",
            source_path.display()
        ));
    }

    if claims_file.scenario_id != scenario_id {
        return Err(anyhow!(
            "claims file {} targets scenario '{}' but requested scenario is '{}'",
            source_path.display(),
            claims_file.scenario_id,
            scenario_id
        ));
    }

    if claims_file.claims.is_empty() {
        return Err(anyhow!(
            "claims file {} contains no claims",
            source_path.display()
        ));
    }

    let claims = materialize_claim_specs_from_file(&claims_file, thresholds, &source_path)?;

    Ok(LoadedClaims {
        source_path,
        schema_version: claims_file.schema_version,
        claims,
    })
}

fn materialize_claim_specs_from_file(
    claims_file: &ClaimsFile,
    thresholds: Thresholds,
    source_path: &Path,
) -> Result<Vec<ClaimSpec>> {
    let mut claim_ids = HashSet::new();
    let mut claims = Vec::with_capacity(claims_file.claims.len());

    for claim in &claims_file.claims {
        validate_claim_entry(claim, source_path)?;

        if !claim_ids.insert(claim.claim_id.clone()) {
            return Err(anyhow!(
                "claims file {} contains duplicate claim_id '{}'",
                source_path.display(),
                claim.claim_id
            ));
        }

        let min_count = resolve_file_claim_threshold(claim, thresholds, source_path)?;
        claims.push(ClaimSpec {
            claim_id: claim.claim_id.clone(),
            category: claim.category,
            kind: claim.kind,
            file: claim.file.clone(),
            pattern: claim.pattern.clone(),
            min_count,
        });
    }

    Ok(claims)
}

fn validate_claim_entry(claim: &ClaimsFileEntry, source_path: &Path) -> Result<()> {
    if claim.claim_id.trim().is_empty() {
        return Err(anyhow!(
            "claims file {} has a claim with empty claim_id",
            source_path.display()
        ));
    }
    if claim.file.trim().is_empty() {
        return Err(anyhow!(
            "claim '{}' in {} has empty file",
            claim.claim_id,
            source_path.display()
        ));
    }
    if claim.pattern.trim().is_empty() {
        return Err(anyhow!(
            "claim '{}' in {} has empty pattern",
            claim.claim_id,
            source_path.display()
        ));
    }

    Regex::new(&claim.pattern).with_context(|| {
        format!(
            "claim '{}' in {} has invalid regex '{}'",
            claim.claim_id,
            source_path.display(),
            claim.pattern
        )
    })?;

    Ok(())
}

fn resolve_file_claim_threshold(
    claim: &ClaimsFileEntry,
    thresholds: Thresholds,
    source_path: &Path,
) -> Result<usize> {
    match claim.kind {
        ClaimKind::MustMatch => match claim.threshold {
            Some(ThresholdValue::Selector(selector_key)) => {
                Ok(resolve_selector_key(selector_key, thresholds))
            }
            Some(ThresholdValue::Fixed(value)) => Ok(value),
            None => Err(anyhow!(
                "claim '{}' in {} is must_match and requires a threshold",
                claim.claim_id,
                source_path.display()
            )),
        },
        ClaimKind::MustNotMatch => {
            if claim.threshold.is_some() {
                return Err(anyhow!(
                    "claim '{}' in {} is must_not_match and must not define a threshold",
                    claim.claim_id,
                    source_path.display()
                ));
            }

            Ok(0)
        }
    }
}

fn resolve_selector_key(selector_key: ThresholdSelectorKey, thresholds: Thresholds) -> usize {
    match selector_key {
        ThresholdSelectorKey::EndpointCommunication => thresholds.endpoint_communication_min_count,
        ThresholdSelectorKey::EgressSendAttempt => thresholds.egress_send_attempt_min_count,
        ThresholdSelectorKey::EgressSendOk => thresholds.egress_send_ok_min_count,
        ThresholdSelectorKey::EgressWorkerCreateOrReuse => {
            thresholds.egress_worker_create_or_reuse_min_count
        }
    }
}

fn resolve_override_path(repo_root: &Path, claims_path: &Path) -> PathBuf {
    if claims_path.is_absolute() {
        claims_path.to_path_buf()
    } else {
        repo_root.join(claims_path)
    }
}

pub fn evaluate_claims(artifacts_dir: &Path, claims: &[ClaimSpec]) -> Vec<ClaimOutcome> {
    claims
        .iter()
        .map(|claim| evaluate_claim(artifacts_dir, claim))
        .collect()
}

fn evaluate_claim(artifacts_dir: &Path, claim: &ClaimSpec) -> ClaimOutcome {
    let file_path = artifacts_dir.join(&claim.file);
    let file_content = match fs::read_to_string(&file_path) {
        Ok(file_content) => file_content,
        Err(error) => {
            return ClaimOutcome {
                claim_id: claim.claim_id.clone(),
                category: claim.category,
                kind: claim.kind,
                file: claim.file.clone(),
                pattern: claim.pattern.clone(),
                min_count: claim.min_count,
                observed_count: 0,
                pass: false,
                first_match: None,
                error: Some(format!("unable to read {}: {error}", file_path.display())),
            }
        }
    };

    let regex = match Regex::new(&claim.pattern) {
        Ok(regex) => regex,
        Err(error) => {
            return ClaimOutcome {
                claim_id: claim.claim_id.clone(),
                category: claim.category,
                kind: claim.kind,
                file: claim.file.clone(),
                pattern: claim.pattern.clone(),
                min_count: claim.min_count,
                observed_count: 0,
                pass: false,
                first_match: None,
                error: Some(format!("invalid regex '{}': {error}", claim.pattern)),
            }
        }
    };

    let mut match_iter = regex.find_iter(&file_content);
    let first_match = match_iter
        .next()
        .map(|first_match| first_match.as_str().to_string());
    let observed_count = first_match
        .as_ref()
        .map(|_| 1 + match_iter.count())
        .unwrap_or(0);

    let pass = match claim.kind {
        ClaimKind::MustMatch => observed_count >= claim.min_count,
        ClaimKind::MustNotMatch => observed_count == 0,
    };

    ClaimOutcome {
        claim_id: claim.claim_id.clone(),
        category: claim.category,
        kind: claim.kind,
        file: claim.file.clone(),
        pattern: claim.pattern.clone(),
        min_count: claim.min_count,
        observed_count,
        pass,
        first_match,
        error: None,
    }
}

pub fn split_claim_outcomes(
    outcomes: Vec<ClaimOutcome>,
) -> (Vec<ClaimOutcome>, Vec<ClaimOutcome>, Option<String>) {
    let mut must_outcomes = Vec::new();
    let mut forbidden_outcomes = Vec::new();
    let mut first_failure_reason = None;

    for outcome in outcomes {
        if !outcome.pass && first_failure_reason.is_none() {
            first_failure_reason = Some(format!(
                "claim '{}' failed (file='{}', observed={}, min={}, pattern='{}')",
                outcome.claim_id,
                outcome.file,
                outcome.observed_count,
                outcome.min_count,
                outcome.pattern
            ));
        }

        if outcome.kind == ClaimKind::MustNotMatch {
            forbidden_outcomes.push(outcome);
        } else {
            must_outcomes.push(outcome);
        }
    }

    (must_outcomes, forbidden_outcomes, first_failure_reason)
}

#[cfg(test)]
mod tests {
    use super::{
        claims_override_kind, default_claims_file_path, evaluate_claims, load_claims_for_scenario,
        materialize_claims, resolve_claims_file_path, split_claim_outcomes, ClaimCategory,
        ClaimKind, ClaimTemplate, ClaimsPathKind, ThresholdSelector, Thresholds,
        DEFAULT_CLAIMS_DIRECTORY,
    };
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;

    #[test]
    fn must_match_counts_non_overlapping_regex_matches() {
        let temp_dir = TempDir::new().expect("create temp dir");
        fs::write(
            temp_dir.path().join("streamer.log"),
            "event=egress_send_attempt event=egress_send_attempt\n",
        )
        .expect("write fixture");

        let claims = materialize_claims(
            &[ClaimTemplate::must_match(
                "attempt",
                ClaimCategory::StreamerEgress,
                "streamer.log",
                "event=egress_send_attempt",
                ThresholdSelector::Fixed(2),
            )],
            Thresholds::default(),
        );

        let outcomes = evaluate_claims(temp_dir.path(), &claims);
        assert_eq!(outcomes[0].observed_count, 2);
        assert!(outcomes[0].pass);
    }

    #[test]
    fn must_not_match_fails_when_forbidden_pattern_exists() {
        let temp_dir = TempDir::new().expect("create temp dir");
        fs::write(
            temp_dir.path().join("streamer.log"),
            "thread panicked at boom\n",
        )
        .expect("write fixture");

        let claims = materialize_claims(
            &[ClaimTemplate::must_not_match(
                "no_panic",
                ClaimCategory::ForbiddenSignature,
                "streamer.log",
                "panicked at",
            )],
            Thresholds::default(),
        );

        let outcomes = evaluate_claims(temp_dir.path(), &claims);
        assert_eq!(outcomes[0].kind, ClaimKind::MustNotMatch);
        assert_eq!(outcomes[0].observed_count, 1);
        assert!(!outcomes[0].pass);
    }

    #[test]
    fn missing_file_is_hard_claim_failure() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let claims = materialize_claims(
            &[ClaimTemplate::must_match(
                "missing",
                ClaimCategory::EndpointCommunication,
                "service.log",
                "Sending Response message",
                ThresholdSelector::EndpointCommunication,
            )],
            Thresholds::default(),
        );

        let outcomes = evaluate_claims(temp_dir.path(), &claims);
        assert!(!outcomes[0].pass);
        assert!(outcomes[0].error.is_some());
    }

    #[test]
    fn malformed_regex_is_hard_claim_failure() {
        let temp_dir = TempDir::new().expect("create temp dir");
        fs::write(temp_dir.path().join("client.log"), "any content\n").expect("write fixture");

        let claims = materialize_claims(
            &[ClaimTemplate::must_match(
                "bad_regex",
                ClaimCategory::EndpointCommunication,
                "client.log",
                "(unclosed",
                ThresholdSelector::Fixed(1),
            )],
            Thresholds::default(),
        );

        let outcomes = evaluate_claims(temp_dir.path(), &claims);
        assert!(!outcomes[0].pass);
        assert!(outcomes[0].error.is_some());
    }

    #[test]
    fn split_claim_outcomes_separates_forbidden_claims() {
        let temp_dir = TempDir::new().expect("create temp dir");
        fs::write(
            temp_dir.path().join("streamer.log"),
            "event=egress_send_ok\n",
        )
        .expect("write fixture");

        let claims = materialize_claims(
            &[
                ClaimTemplate::must_match(
                    "send_ok",
                    ClaimCategory::StreamerEgress,
                    "streamer.log",
                    "event=egress_send_ok",
                    ThresholdSelector::Fixed(1),
                ),
                ClaimTemplate::must_not_match(
                    "no_panic",
                    ClaimCategory::ForbiddenSignature,
                    "streamer.log",
                    "panicked at",
                ),
            ],
            Thresholds::default(),
        );

        let outcomes = evaluate_claims(temp_dir.path(), &claims);
        let (must_outcomes, forbidden_outcomes, first_failure) = split_claim_outcomes(outcomes);

        assert_eq!(must_outcomes.len(), 1);
        assert_eq!(forbidden_outcomes.len(), 1);
        assert!(first_failure.is_none());
    }

    #[test]
    fn resolve_claims_file_path_uses_default_directory() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let repo_root = temp_dir.path();
        let scenario_id = "smoke-example";

        write_claims_file(
            repo_root,
            scenario_id,
            valid_claims_payload(scenario_id, "endpoint_communication"),
        );

        let resolved = resolve_claims_file_path(repo_root, scenario_id, None)
            .expect("resolve claims file path");
        assert_eq!(resolved, default_claims_file_path(repo_root, scenario_id));
    }

    #[test]
    fn claims_override_kind_detects_file_and_directory() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let repo_root = temp_dir.path();
        let scenario_id = "smoke-example";

        let claims_file = write_claims_file(
            repo_root,
            scenario_id,
            valid_claims_payload(scenario_id, "endpoint_communication"),
        );

        let claims_dir = claims_file.parent().expect("claims dir");
        assert_eq!(
            claims_override_kind(repo_root, claims_dir).expect("directory kind"),
            ClaimsPathKind::Directory
        );
        assert_eq!(
            claims_override_kind(repo_root, &claims_file).expect("file kind"),
            ClaimsPathKind::File
        );
    }

    #[test]
    fn load_claims_for_scenario_resolves_selector_thresholds() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let repo_root = temp_dir.path();
        let scenario_id = "smoke-example";

        write_claims_file(
            repo_root,
            scenario_id,
            valid_claims_payload(scenario_id, "egress_send_ok"),
        );

        let loaded = load_claims_for_scenario(
            repo_root,
            scenario_id,
            None,
            Thresholds {
                endpoint_communication_min_count: 4,
                egress_send_attempt_min_count: 2,
                egress_send_ok_min_count: 7,
                egress_worker_create_or_reuse_min_count: 1,
            },
        )
        .expect("load claims");

        assert_eq!(loaded.schema_version, "1.0");
        assert_eq!(loaded.claims.len(), 2);
        assert_eq!(loaded.claims[0].min_count, 7);
        assert_eq!(loaded.claims[1].min_count, 0);
    }

    #[test]
    fn load_claims_for_scenario_supports_directory_override() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let repo_root = temp_dir.path();
        let scenario_id = "smoke-example";

        let claims_dir = repo_root.join("custom-claims");
        let claims_file = claims_dir.join(format!("{scenario_id}.json"));
        write_claims_file_at(
            &claims_file,
            valid_claims_payload(scenario_id, "endpoint_communication"),
        );

        let loaded = load_claims_for_scenario(
            repo_root,
            scenario_id,
            Some(claims_dir.as_path()),
            Thresholds::default(),
        )
        .expect("load claims from directory override");

        assert_eq!(loaded.source_path, claims_file);
        assert_eq!(loaded.claims.len(), 2);
    }

    #[test]
    fn load_claims_for_scenario_supports_file_override() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let repo_root = temp_dir.path();
        let scenario_id = "smoke-example";

        let claims_file = repo_root.join("claims-one-off.json");
        write_claims_file_at(
            &claims_file,
            valid_claims_payload(scenario_id, "endpoint_communication"),
        );

        let loaded = load_claims_for_scenario(
            repo_root,
            scenario_id,
            Some(claims_file.as_path()),
            Thresholds::default(),
        )
        .expect("load claims from file override");

        assert_eq!(loaded.source_path, claims_file);
        assert_eq!(loaded.claims.len(), 2);
    }

    #[test]
    fn load_claims_for_scenario_rejects_missing_file() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let repo_root = temp_dir.path();

        let error =
            load_claims_for_scenario(repo_root, "smoke-missing", None, Thresholds::default())
                .expect_err("expected missing file failure");
        assert!(error
            .to_string()
            .contains("resolved claims file does not exist or is not accessible"));
    }

    #[test]
    fn load_claims_for_scenario_rejects_malformed_json() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let repo_root = temp_dir.path();
        let scenario_id = "smoke-example";

        write_claims_file(
            repo_root,
            scenario_id,
            "{\n  \"schema_version\": \"1.0\",\n  \"scenario_id\": \"smoke-example\",\n  \"claims\": [\n"
                .to_string(),
        );

        let error = load_claims_for_scenario(repo_root, scenario_id, None, Thresholds::default())
            .expect_err("expected malformed json failure");
        assert!(error.to_string().contains("unable to parse claims file"));
    }

    #[test]
    fn load_claims_for_scenario_rejects_scenario_id_mismatch() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let repo_root = temp_dir.path();

        write_claims_file(
            repo_root,
            "smoke-a",
            valid_claims_payload("smoke-b", "endpoint_communication"),
        );

        let error = load_claims_for_scenario(repo_root, "smoke-a", None, Thresholds::default())
            .expect_err("expected scenario id mismatch");
        assert!(error.to_string().contains("targets scenario 'smoke-b'"));
    }

    #[test]
    fn load_claims_for_scenario_rejects_duplicate_claim_ids() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let repo_root = temp_dir.path();
        let scenario_id = "smoke-example";

        write_claims_file(
            repo_root,
            scenario_id,
            format!(
                "{{\n  \"schema_version\": \"1.0\",\n  \"scenario_id\": \"{scenario_id}\",\n  \"claims\": [\n    {{\n      \"claim_id\": \"dup\",\n      \"category\": \"endpoint_communication\",\n      \"kind\": \"must_match\",\n      \"file\": \"client.log\",\n      \"pattern\": \"ok\",\n      \"threshold\": \"endpoint_communication\"\n    }},\n    {{\n      \"claim_id\": \"dup\",\n      \"category\": \"forbidden_signature\",\n      \"kind\": \"must_not_match\",\n      \"file\": \"streamer.log\",\n      \"pattern\": \"panicked at\"\n    }}\n  ]\n}}"
            ),
        );

        let error = load_claims_for_scenario(repo_root, scenario_id, None, Thresholds::default())
            .expect_err("expected duplicate claim id failure");
        assert!(error.to_string().contains("duplicate claim_id"));
    }

    #[test]
    fn load_claims_for_scenario_rejects_threshold_on_must_not_match() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let repo_root = temp_dir.path();
        let scenario_id = "smoke-example";

        write_claims_file(
            repo_root,
            scenario_id,
            format!(
                "{{\n  \"schema_version\": \"1.0\",\n  \"scenario_id\": \"{scenario_id}\",\n  \"claims\": [\n    {{\n      \"claim_id\": \"no_panic\",\n      \"category\": \"forbidden_signature\",\n      \"kind\": \"must_not_match\",\n      \"file\": \"streamer.log\",\n      \"pattern\": \"panicked at\",\n      \"threshold\": 1\n    }}\n  ]\n}}"
            ),
        );

        let error = load_claims_for_scenario(repo_root, scenario_id, None, Thresholds::default())
            .expect_err("expected threshold validation failure");
        assert!(error
            .to_string()
            .contains("must_not_match and must not define a threshold"));
    }

    #[test]
    fn load_claims_for_scenario_rejects_invalid_threshold_descriptor() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let repo_root = temp_dir.path();
        let scenario_id = "smoke-example";

        write_claims_file(
            repo_root,
            scenario_id,
            valid_claims_payload(scenario_id, "not_a_threshold_selector"),
        );

        let error = load_claims_for_scenario(repo_root, scenario_id, None, Thresholds::default())
            .expect_err("expected invalid threshold descriptor failure");
        assert!(error.to_string().contains("unable to parse claims file"));
    }

    fn write_claims_file(
        repo_root: &Path,
        scenario_id: &str,
        payload: String,
    ) -> std::path::PathBuf {
        let claims_path = repo_root
            .join(DEFAULT_CLAIMS_DIRECTORY)
            .join(format!("{scenario_id}.json"));
        write_claims_file_at(&claims_path, payload);
        claims_path
    }

    fn write_claims_file_at(claims_path: &Path, payload: String) {
        fs::create_dir_all(claims_path.parent().expect("claims parent"))
            .expect("create claims dir");
        fs::write(claims_path, payload).expect("write claims file");
    }

    fn valid_claims_payload(scenario_id: &str, selector: &str) -> String {
        format!(
            "{{\n  \"schema_version\": \"1.0\",\n  \"scenario_id\": \"{scenario_id}\",\n  \"claims\": [\n    {{\n      \"claim_id\": \"endpoint\",\n      \"category\": \"endpoint_communication\",\n      \"kind\": \"must_match\",\n      \"file\": \"client.log\",\n      \"pattern\": \"ok\",\n      \"threshold\": \"{selector}\"\n    }},\n    {{\n      \"claim_id\": \"no_panic\",\n      \"category\": \"forbidden_signature\",\n      \"kind\": \"must_not_match\",\n      \"file\": \"streamer.log\",\n      \"pattern\": \"panicked at\"\n    }}\n  ]\n}}"
        )
    }
}
