use clap::Parser;
use criterion_guardrail::{evaluate_guardrail, render_summary_table, write_report, GuardrailInput};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "criterion-guardrail")]
#[command(about = "Evaluate Criterion baseline/candidate thresholds")]
struct Cli {
    #[arg(long)]
    criterion_root: PathBuf,
    #[arg(long)]
    baseline: String,
    #[arg(long)]
    candidate: String,
    #[arg(long)]
    throughput_threshold_pct: f64,
    #[arg(long)]
    latency_threshold_pct: f64,
    #[arg(long)]
    alloc_proxy_threshold_pct: f64,
    #[arg(long)]
    report: PathBuf,
}

fn main() {
    let cli = Cli::parse();
    let input = GuardrailInput {
        criterion_root: cli.criterion_root,
        baseline: cli.baseline,
        candidate: cli.candidate,
        throughput_threshold_pct: cli.throughput_threshold_pct,
        latency_threshold_pct: cli.latency_threshold_pct,
        alloc_proxy_threshold_pct: cli.alloc_proxy_threshold_pct,
    };

    let report = match evaluate_guardrail(&input) {
        Ok(report) => report,
        Err(err) => {
            eprintln!("criterion-guardrail: {err}");
            std::process::exit(1);
        }
    };

    println!("{}", render_summary_table(&report));

    if let Err(err) = write_report(&report, &cli.report) {
        eprintln!("criterion-guardrail: {err}");
        std::process::exit(1);
    }

    println!("JSON report: {}", cli.report.display());

    if report.pass {
        println!("criterion-guardrail: PASS");
        std::process::exit(0);
    }

    eprintln!("criterion-guardrail: FAIL (threshold breach)");
    for failure in report.failures {
        eprintln!("- {failure}");
    }
    std::process::exit(2);
}
