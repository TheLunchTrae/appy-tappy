#!/bin/sh
# Check the project's own rules for a cargo-mutants report in $2 (default
# mutants.out), from a lane named by $1, full or diff. cargo-mutants already
# fails the run for a missed mutant, a timeout, or a failed baseline, and
# holds its own lock on the report, so this checks only what it does not:
#
# - A full run tests at least one mutant that builds.
# - A report in which every mutant is unviable, as a diff shard of a small
#   change can be, is accepted only when each mutant's log shows the
#   compiler rejecting the mutated file. A broken build directory, or a
#   build killed for memory or disk, also leaves every mutant unviable.
set -eu

mode="${1:-}"
report="${2:-mutants.out}"
case "$mode" in
  full | diff) ;;
  *)
    echo "Usage: $0 <full|diff> [report-directory]" >&2
    exit 2
    ;;
esac

# The nonblank lines of the outcome lists named, which cargo-mutants writes
# one mutant per line.
count() {
  for list in "$@"; do
    if [ ! -r "$report/$list.txt" ]; then
      echo "The mutation report has no $list.txt: $report" >&2
      exit 1
    fi
  done
  (cd "$report" && for list in "$@"; do cat "$list.txt"; done) | grep -c . || true
}

tested="$(count caught missed timeout)"
unviable="$(count unviable)"

if [ "$mode" = full ] && [ "$tested" -eq 0 ]; then
  echo "A full mutation run must test at least one mutant that builds: $report" >&2
  exit 1
fi
if [ "$tested" -gt 0 ] || [ "$unviable" -eq 0 ]; then
  exit 0
fi

# Each mutant's log starts with "*** <file>:<line>:<column>: <mutation>",
# and the compiler names the file it rejects in a "--> <file>:" line under
# an error.
for log in "$report"/log/*.log; do
  [ "$(basename "$log")" = baseline.log ] && continue
  if ! awk '
    file == "" && /^\*\*\* / && $2 != "mutation" {
      file = $2
      sub(/:.*/, "", file)
      next
    }
    /^error/ { in_error = 1; next }
    /^[^ ]/ { in_error = 0 }
    in_error && file != "" && index($0, "--> " file ":") { found = 1 }
    END { exit found ? 0 : 1 }
  ' "$log"; then
    echo "Every mutant is unviable, but $log shows no compiler error in the mutated file, so the build directory may be broken." >&2
    exit 1
  fi
done
