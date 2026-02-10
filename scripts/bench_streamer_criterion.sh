#!/usr/bin/env bash

set -euo pipefail

readonly DEFAULT_CRITERION_ARGS="--sample-size 60 --warm-up-time 3 --measurement-time 12 --noise-threshold 0.02"

CRITERION_ARGS="${CRITERION_ARGS:-$DEFAULT_CRITERION_ARGS}"
BENCH_PIN_PREFIX="${BENCH_PIN_PREFIX:-}"

run_bench() {
    if [[ -n "$BENCH_PIN_PREFIX" ]]; then
        read -r -a pin_parts <<<"$BENCH_PIN_PREFIX"
        "${pin_parts[@]}" cargo bench -p up-streamer --bench streamer_criterion -- $CRITERION_ARGS "$@"
    else
        cargo bench -p up-streamer --bench streamer_criterion -- $CRITERION_ARGS "$@"
    fi
}

usage() {
    cat <<'USAGE'
Usage:
  scripts/bench_streamer_criterion.sh baseline
  scripts/bench_streamer_criterion.sh candidate <phase_candidate>
  scripts/bench_streamer_criterion.sh guardrail <phase_candidate> <report_path>
  scripts/bench_streamer_criterion.sh export
USAGE
}

if [[ $# -lt 1 ]]; then
    usage
    exit 1
fi

subcommand="$1"
shift

case "$subcommand" in
baseline)
    run_bench --save-baseline ergonomics_baseline
    ;;
candidate)
    if [[ $# -ne 1 ]]; then
        usage
        exit 1
    fi
    run_bench --save-baseline "$1"
    ;;
guardrail)
    if [[ $# -ne 2 ]]; then
        usage
        exit 1
    fi
    cargo run -p criterion-guardrail -- \
      --criterion-root target/criterion \
      --baseline ergonomics_baseline \
      --candidate "$1" \
      --throughput-threshold-pct 3 \
      --latency-threshold-pct 5 \
      --alloc-proxy-threshold-pct 5 \
      --report "$2"
    ;;
export)
    : "${OPENCODE_CONFIG_DIR:?OPENCODE_CONFIG_DIR must be set for export output path}"
    report_path="$OPENCODE_CONFIG_DIR/reports/ergonomics-perf/bench-data/criterion-compare-bencher.txt"
    mkdir -p "$(dirname "$report_path")"
    run_bench --baseline ergonomics_baseline --output-format bencher | tee "$report_path"
    ;;
*)
    usage
    exit 1
    ;;
esac
