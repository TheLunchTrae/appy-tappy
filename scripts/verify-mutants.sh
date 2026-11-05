#!/bin/sh

set -eu

if [ "${1:-}" = "--self-test" ]; then
  cargo test --locked --all-features --test mutation_report_policy -- \
    --ignored --skip verify_configured_report
  echo "Mutation verifier self-tests passed."
  exit 0
fi

mode="${1:-}"
report_directory="${2:-mutants.out}"
case "$mode" in
  full | diff) ;;
  *)
    echo "Usage: $0 --self-test | <full|diff> [report-directory]" >&2
    exit 2
    ;;
esac

MUTATION_REPORT_MODE="$mode" \
  MUTATION_REPORT_DIRECTORY="$report_directory" \
  cargo test --locked --all-features --test mutation_report_policy \
  verify_configured_report -- --exact --ignored --nocapture
