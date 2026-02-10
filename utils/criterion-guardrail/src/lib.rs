use chrono::Utc;
use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub const REQUIRED_BENCHMARK_IDS: [&str; 6] = [
    "routing_lookup/exact_authority",
    "routing_lookup/wildcard_authority",
    "publish_resolution/source_filter_derivation",
    "ingress_registry/register_route",
    "ingress_registry/unregister_route",
    "egress_forwarding/single_route_dispatch",
];

const THROUGHPUT_GROUPS: [&str; 2] = ["egress_forwarding", "ingress_registry"];
const LATENCY_GROUPS: [&str; 2] = ["routing_lookup", "publish_resolution"];
const ALLOC_PROXY_BENCHMARK_IDS: [&str; 2] = [
    "routing_lookup/wildcard_authority",
    "publish_resolution/source_filter_derivation",
];

const REQUIRED_HEADER_COLUMNS: [&str; 2] = ["sample_measured_value", "iteration_count"];

#[derive(Clone, Debug)]
pub struct GuardrailInput {
    pub criterion_root: PathBuf,
    pub baseline: String,
    pub candidate: String,
    pub throughput_threshold_pct: f64,
    pub latency_threshold_pct: f64,
    pub alloc_proxy_threshold_pct: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct ThresholdConfig {
    pub throughput_threshold_pct: f64,
    pub latency_threshold_pct: f64,
    pub alloc_proxy_threshold_pct: f64,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricKind {
    Throughput,
    Latency,
    AllocProxy,
}

impl MetricKind {
    fn as_label(self) -> &'static str {
        match self {
            Self::Throughput => "throughput",
            Self::Latency => "latency",
            Self::AllocProxy => "alloc_proxy",
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct BenchmarkResult {
    pub benchmark_id: String,
    pub metric_kind: MetricKind,
    pub baseline_ns_per_iter: f64,
    pub candidate_ns_per_iter: f64,
    pub delta_pct: f64,
    pub threshold_pct: f64,
    pub pass: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct GuardrailReport {
    pub timestamp_utc: String,
    pub criterion_root: String,
    pub baseline: String,
    pub candidate: String,
    pub thresholds: ThresholdConfig,
    pub pass: bool,
    pub failures: Vec<String>,
    pub results: Vec<BenchmarkResult>,
}

pub fn evaluate_guardrail(input: &GuardrailInput) -> Result<GuardrailReport, String> {
    let baseline_map = discover_named_raw_csvs(&input.criterion_root, &input.baseline)?;
    let candidate_map = discover_named_raw_csvs(&input.criterion_root, &input.candidate)?;

    let mut missing_ids = Vec::new();
    for benchmark_id in REQUIRED_BENCHMARK_IDS {
        if !baseline_map.contains_key(benchmark_id) {
            missing_ids.push(format!("{benchmark_id} (missing baseline CSV)"));
        }
        if !candidate_map.contains_key(benchmark_id) {
            missing_ids.push(format!("{benchmark_id} (missing candidate CSV)"));
        }
    }

    if !missing_ids.is_empty() {
        return Err(format!(
            "missing required benchmark IDs in criterion data: {}. Ensure `{}` and `{}` exist under `{}` using Criterion baseline naming and the required benchmark IDs.",
            missing_ids.join(", "),
            input.baseline,
            input.candidate,
            input.criterion_root.display()
        ));
    }

    let thresholds = ThresholdConfig {
        throughput_threshold_pct: input.throughput_threshold_pct,
        latency_threshold_pct: input.latency_threshold_pct,
        alloc_proxy_threshold_pct: input.alloc_proxy_threshold_pct,
    };

    let mut results = Vec::with_capacity(REQUIRED_BENCHMARK_IDS.len());
    let mut failures = Vec::new();

    for benchmark_id in REQUIRED_BENCHMARK_IDS {
        let baseline_path = baseline_map
            .get(benchmark_id)
            .ok_or_else(|| format!("internal error: baseline missing for `{benchmark_id}`"))?;
        let candidate_path = candidate_map
            .get(benchmark_id)
            .ok_or_else(|| format!("internal error: candidate missing for `{benchmark_id}`"))?;

        let baseline_ns_per_iter = parse_median_ns_per_iter(baseline_path)?;
        let candidate_ns_per_iter = parse_median_ns_per_iter(candidate_path)?;

        let (metric_kind, threshold_pct) = benchmark_threshold(benchmark_id, &thresholds)?;
        let delta_pct = match metric_kind {
            MetricKind::Throughput => {
                throughput_delta_pct(baseline_ns_per_iter, candidate_ns_per_iter)?
            }
            MetricKind::Latency | MetricKind::AllocProxy => {
                latency_delta_pct(baseline_ns_per_iter, candidate_ns_per_iter)?
            }
        };

        let pass = delta_pct <= threshold_pct;
        if !pass {
            failures.push(format!(
                "{benchmark_id}: regression {delta_pct:.3}% exceeds threshold {threshold_pct:.3}%"
            ));
        }

        results.push(BenchmarkResult {
            benchmark_id: benchmark_id.to_string(),
            metric_kind,
            baseline_ns_per_iter,
            candidate_ns_per_iter,
            delta_pct,
            threshold_pct,
            pass,
        });
    }

    let pass = failures.is_empty();
    Ok(GuardrailReport {
        timestamp_utc: Utc::now().to_rfc3339(),
        criterion_root: input.criterion_root.display().to_string(),
        baseline: input.baseline.clone(),
        candidate: input.candidate.clone(),
        thresholds,
        pass,
        failures,
        results,
    })
}

pub fn write_report(report: &GuardrailReport, report_path: &Path) -> Result<(), String> {
    if let Some(parent) = report_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "failed to create report parent directory `{}`: {err}",
                parent.display()
            )
        })?;
    }

    let payload = serde_json::to_string_pretty(report)
        .map_err(|err| format!("failed to serialize guardrail JSON report: {err}"))?;
    fs::write(report_path, payload).map_err(|err| {
        format!(
            "failed to write guardrail JSON report to `{}`: {err}",
            report_path.display()
        )
    })
}

pub fn render_summary_table(report: &GuardrailReport) -> String {
    let mut lines = Vec::with_capacity(report.results.len() + 4);
    lines.push(format!(
        "{:<48} {:>12} {:>12} {:>10} {:>10} {:>10}",
        "benchmark_id", "baseline_ns", "candidate_ns", "delta_%", "threshold", "result"
    ));
    lines.push("-".repeat(108));

    for result in &report.results {
        lines.push(format!(
            "{:<48} {:>12.3} {:>12.3} {:>10.3} {:>10.3} {:>10}",
            format!(
                "{} ({})",
                result.benchmark_id,
                result.metric_kind.as_label()
            ),
            result.baseline_ns_per_iter,
            result.candidate_ns_per_iter,
            result.delta_pct,
            result.threshold_pct,
            if result.pass { "PASS" } else { "FAIL" }
        ));
    }

    lines.join("\n")
}

fn benchmark_threshold(
    benchmark_id: &str,
    thresholds: &ThresholdConfig,
) -> Result<(MetricKind, f64), String> {
    if ALLOC_PROXY_BENCHMARK_IDS.contains(&benchmark_id) {
        return Ok((MetricKind::AllocProxy, thresholds.alloc_proxy_threshold_pct));
    }

    let group = benchmark_id
        .split('/')
        .next()
        .ok_or_else(|| format!("invalid benchmark id `{benchmark_id}`"))?;

    if THROUGHPUT_GROUPS.contains(&group) {
        return Ok((MetricKind::Throughput, thresholds.throughput_threshold_pct));
    }

    if LATENCY_GROUPS.contains(&group) {
        return Ok((MetricKind::Latency, thresholds.latency_threshold_pct));
    }

    Err(format!(
        "benchmark group `{group}` is not mapped to a threshold for `{benchmark_id}`"
    ))
}

fn latency_delta_pct(baseline_ns_per_iter: f64, candidate_ns_per_iter: f64) -> Result<f64, String> {
    if baseline_ns_per_iter <= 0.0 {
        return Err("baseline ns/iter must be > 0 for latency delta".to_string());
    }
    Ok(((candidate_ns_per_iter - baseline_ns_per_iter) / baseline_ns_per_iter) * 100.0)
}

fn throughput_delta_pct(
    baseline_ns_per_iter: f64,
    candidate_ns_per_iter: f64,
) -> Result<f64, String> {
    if baseline_ns_per_iter <= 0.0 || candidate_ns_per_iter <= 0.0 {
        return Err("baseline and candidate ns/iter must be > 0 for throughput delta".to_string());
    }

    let baseline_ops = 1_000_000_000.0 / baseline_ns_per_iter;
    let candidate_ops = 1_000_000_000.0 / candidate_ns_per_iter;
    Ok(((baseline_ops - candidate_ops) / baseline_ops) * 100.0)
}

fn parse_median_ns_per_iter(csv_path: &Path) -> Result<f64, String> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_path(csv_path)
        .map_err(|err| format!("failed to open `{}`: {err}", csv_path.display()))?;

    let headers = reader
        .headers()
        .map_err(|err| {
            format!(
                "failed to read CSV headers from `{}`: {err}",
                csv_path.display()
            )
        })?
        .clone();

    for required_column in REQUIRED_HEADER_COLUMNS {
        if !headers.iter().any(|column| column == required_column) {
            return Err(format!(
                "unsupported Criterion raw.csv header layout in `{}`; missing required column `{required_column}`. Expected headers to include `sample_measured_value` and `iteration_count` for the pinned Criterion output.",
                csv_path.display()
            ));
        }
    }

    let measured_index = headers
        .iter()
        .position(|column| column == "sample_measured_value")
        .ok_or_else(|| {
            format!(
                "internal header index error for `sample_measured_value` in `{}`",
                csv_path.display()
            )
        })?;
    let iteration_index = headers
        .iter()
        .position(|column| column == "iteration_count")
        .ok_or_else(|| {
            format!(
                "internal header index error for `iteration_count` in `{}`",
                csv_path.display()
            )
        })?;

    let mut samples_ns_per_iter = Vec::new();
    for record in reader.records() {
        let row = record.map_err(|err| {
            format!(
                "failed to parse CSV row from `{}`: {err}",
                csv_path.display()
            )
        })?;

        let measured_value = row
            .get(measured_index)
            .ok_or_else(|| {
                format!(
                    "CSV row in `{}` is missing `sample_measured_value` at expected index",
                    csv_path.display()
                )
            })?
            .parse::<f64>()
            .map_err(|err| {
                format!(
                    "failed to parse `sample_measured_value` in `{}`: {err}",
                    csv_path.display()
                )
            })?;

        let iteration_count = row
            .get(iteration_index)
            .ok_or_else(|| {
                format!(
                    "CSV row in `{}` is missing `iteration_count` at expected index",
                    csv_path.display()
                )
            })?
            .parse::<f64>()
            .map_err(|err| {
                format!(
                    "failed to parse `iteration_count` in `{}`: {err}",
                    csv_path.display()
                )
            })?;

        if iteration_count <= 0.0 {
            return Err(format!(
                "invalid `iteration_count` <= 0 in `{}`",
                csv_path.display()
            ));
        }

        samples_ns_per_iter.push(measured_value / iteration_count);
    }

    if samples_ns_per_iter.is_empty() {
        return Err(format!(
            "no benchmark samples found in `{}`",
            csv_path.display()
        ));
    }

    samples_ns_per_iter.sort_by(|left, right| left.total_cmp(right));
    Ok(median(&samples_ns_per_iter))
}

fn median(sorted_values: &[f64]) -> f64 {
    let len = sorted_values.len();
    if len % 2 == 1 {
        sorted_values[len / 2]
    } else {
        (sorted_values[(len / 2) - 1] + sorted_values[len / 2]) / 2.0
    }
}

fn discover_named_raw_csvs(
    criterion_root: &Path,
    comparison_name: &str,
) -> Result<HashMap<String, PathBuf>, String> {
    if !criterion_root.is_dir() {
        return Err(format!(
            "criterion root `{}` does not exist or is not a directory",
            criterion_root.display()
        ));
    }

    let mut discovered = HashMap::new();
    for entry in WalkDir::new(criterion_root) {
        let entry = entry.map_err(|err| {
            format!(
                "failed to walk criterion directory `{}`: {err}",
                criterion_root.display()
            )
        })?;

        if !entry.file_type().is_file() || entry.file_name() != "raw.csv" {
            continue;
        }

        let Some((benchmark_id, csv_path, is_direct_layout)) =
            classify_named_raw_csv_path(criterion_root, entry.path(), comparison_name)?
        else {
            continue;
        };

        match discovered.get(&benchmark_id) {
            None => {
                discovered.insert(benchmark_id, csv_path);
            }
            Some(existing_path) => {
                if is_direct_layout && existing_path.to_string_lossy().contains("/new/raw.csv") {
                    discovered.insert(benchmark_id, csv_path);
                }
            }
        }
    }

    Ok(discovered)
}

fn classify_named_raw_csv_path(
    criterion_root: &Path,
    csv_path: &Path,
    comparison_name: &str,
) -> Result<Option<(String, PathBuf, bool)>, String> {
    let Some(parent) = csv_path.parent() else {
        return Ok(None);
    };

    let mut benchmark_dir = None;
    let mut is_direct_layout = false;

    if parent
        .file_name()
        .is_some_and(|component| component == comparison_name)
    {
        benchmark_dir = parent.parent();
        is_direct_layout = true;
    } else if parent
        .file_name()
        .is_some_and(|component| component == "new")
        && parent
            .parent()
            .and_then(|path| path.file_name())
            .is_some_and(|component| component == comparison_name)
    {
        benchmark_dir = parent.parent().and_then(|path| path.parent());
    }

    let Some(benchmark_dir) = benchmark_dir else {
        return Ok(None);
    };

    let relative = benchmark_dir.strip_prefix(criterion_root).map_err(|err| {
        format!(
            "failed to derive benchmark id from `{}` under criterion root `{}`: {err}",
            benchmark_dir.display(),
            criterion_root.display()
        )
    })?;

    let benchmark_id = relative
        .iter()
        .map(|component| component.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("/");

    if benchmark_id.is_empty() {
        return Ok(None);
    }

    Ok(Some((
        benchmark_id,
        csv_path.to_path_buf(),
        is_direct_layout,
    )))
}

#[cfg(test)]
mod tests {
    use super::{
        evaluate_guardrail, parse_median_ns_per_iter, GuardrailInput, REQUIRED_BENCHMARK_IDS,
    };
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;

    const HEADER: &str = "group,function,value,sample_measured_value,unit,iteration_count\n";

    fn write_raw_csv(path: &Path, measured_values: &[u64], iteration_count: u64) {
        let mut payload = String::from(HEADER);
        for measured_value in measured_values {
            payload.push_str(&format!(
                "routing,bench,id,{measured_value},ns,{iteration_count}\n"
            ));
        }

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create fixture parent directories");
        }
        fs::write(path, payload).expect("write fixture csv");
    }

    fn write_all_required_benchmark_ids(
        criterion_root: &Path,
        baseline: &str,
        candidate: &str,
        candidate_multiplier_pct: f64,
    ) {
        for benchmark_id in REQUIRED_BENCHMARK_IDS {
            let baseline_path = criterion_root
                .join(benchmark_id)
                .join(baseline)
                .join("raw.csv");
            let candidate_path = criterion_root
                .join(benchmark_id)
                .join(candidate)
                .join("raw.csv");

            write_raw_csv(&baseline_path, &[1000, 1020, 980], 10);
            let candidate_sample = (1000.0 * candidate_multiplier_pct) as u64;
            write_raw_csv(
                &candidate_path,
                &[
                    candidate_sample,
                    candidate_sample + 5,
                    candidate_sample + 10,
                ],
                10,
            );
        }
    }

    #[test]
    fn parse_median_uses_sample_over_iteration_count_ratio() {
        let tempdir = TempDir::new().expect("tempdir");
        let csv_path = tempdir.path().join("ratio/raw.csv");

        write_raw_csv(&csv_path, &[1000, 1200, 1100], 10);

        let median = parse_median_ns_per_iter(&csv_path).expect("csv should parse");
        assert!((median - 110.0).abs() < f64::EPSILON);
    }

    #[test]
    fn parse_median_rejects_schema_without_required_headers() {
        let tempdir = TempDir::new().expect("tempdir");
        let csv_path = tempdir.path().join("invalid/raw.csv");

        if let Some(parent) = csv_path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        fs::write(
            &csv_path,
            "group,function,value,sample_measured_value,unit\nrouting,bench,id,1000,ns\n",
        )
        .expect("write invalid csv");

        let parse_err = parse_median_ns_per_iter(&csv_path).expect_err("should fail on schema");
        assert!(parse_err.contains("unsupported Criterion raw.csv header layout"));
    }

    #[test]
    fn evaluate_guardrail_returns_error_for_missing_required_benchmark_id() {
        let tempdir = TempDir::new().expect("tempdir");
        let criterion_root = tempdir.path().join("criterion");
        let baseline = "ergonomics_baseline";
        let candidate = "ergonomics_candidate";

        let benchmark_id = REQUIRED_BENCHMARK_IDS[0];
        write_raw_csv(
            &criterion_root
                .join(benchmark_id)
                .join(baseline)
                .join("raw.csv"),
            &[1000, 1010, 1020],
            10,
        );
        write_raw_csv(
            &criterion_root
                .join(benchmark_id)
                .join(candidate)
                .join("raw.csv"),
            &[1000, 1010, 1020],
            10,
        );

        let err = evaluate_guardrail(&GuardrailInput {
            criterion_root,
            baseline: baseline.to_string(),
            candidate: candidate.to_string(),
            throughput_threshold_pct: 3.0,
            latency_threshold_pct: 5.0,
            alloc_proxy_threshold_pct: 5.0,
        })
        .expect_err("missing benchmark IDs should fail");

        assert!(err.contains("missing required benchmark IDs"));
    }

    #[test]
    fn evaluate_guardrail_reports_threshold_breach_without_parse_failure() {
        let tempdir = TempDir::new().expect("tempdir");
        let criterion_root = tempdir.path().join("criterion");
        let baseline = "ergonomics_baseline";
        let candidate = "ergonomics_candidate";

        write_all_required_benchmark_ids(&criterion_root, baseline, candidate, 1.20);

        let report = evaluate_guardrail(&GuardrailInput {
            criterion_root,
            baseline: baseline.to_string(),
            candidate: candidate.to_string(),
            throughput_threshold_pct: 3.0,
            latency_threshold_pct: 5.0,
            alloc_proxy_threshold_pct: 5.0,
        })
        .expect("schema is valid");

        assert!(!report.pass, "20% regression must fail thresholds");
        assert!(!report.failures.is_empty());
        assert!(report.results.iter().any(|result| !result.pass));
    }

    #[test]
    fn evaluate_guardrail_passes_when_candidate_is_within_thresholds() {
        let tempdir = TempDir::new().expect("tempdir");
        let criterion_root = tempdir.path().join("criterion");
        let baseline = "ergonomics_baseline";
        let candidate = "ergonomics_candidate";

        write_all_required_benchmark_ids(&criterion_root, baseline, candidate, 1.01);

        let report = evaluate_guardrail(&GuardrailInput {
            criterion_root,
            baseline: baseline.to_string(),
            candidate: candidate.to_string(),
            throughput_threshold_pct: 3.0,
            latency_threshold_pct: 5.0,
            alloc_proxy_threshold_pct: 5.0,
        })
        .expect("schema is valid");

        assert!(
            report.pass,
            "1% regression should pass configured thresholds"
        );
        assert!(report.failures.is_empty());
    }
}
