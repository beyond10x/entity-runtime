#!/bin/sh
# One bounded test step: build the tests, run them under a wall-clock budget, name the ten slowest.
#
# Usage:
#   test-step.sh [--budget SECONDS] -- CARGO_ARGUMENTS...
#   test-step.sh slowest REPORT        print the ten slowest test cases of a nextest JUnit report
#
# CARGO_ARGUMENTS is a `nextest run` command line without `cargo`, for example
# `+1.91.0 nextest run -p entity-eventlog --all-features --locked`. The step first builds the test
# binaries (`--no-run`, outside the budget: compiling is not testing), then runs the tests under
# `timeout`, so the step fails once the tests run longer than the budget (default 120 s, the
# folder's test budget). `.config/nextest.toml` ends any single test at 60 s. Whatever the outcome,
# the step prints the run's ten slowest tests from nextest's JUnit report, slowest first, so a test
# that grows towards the bound is seen before it reaches it. Nextest writes that report under the
# workspace's own `target/nextest`, whatever `CARGO_TARGET_DIR` says; `TEST_STEP_REPORT` names
# another path. Tested by scripts/test-step-test.sh.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)

# The `count` longest `<testcase>` entries of a JUnit report, as `seconds  binary test`.
slowest() {
    awk -v count="$2" '
        /<testcase / {
            name = attribute($0, "name")
            binary = attribute($0, "classname")
            seconds = attribute($0, "time")
            if (seconds != "") printf "%.3f\t%s %s\n", seconds, binary, name
        }
        function attribute(line, key,    at, rest) {
            at = index(line, " " key "=\"")
            if (at == 0) return ""
            rest = substr(line, at + length(key) + 3)
            return substr(rest, 1, index(rest, "\"") - 1)
        }
    ' "$1" | sort -t "$(printf '\t')" -k1,1 -g -r | head -n "$2" |
        awk -F '\t' '{ printf "test-step: %8.3f s  %s\n", $1, $2 }'
}

if [ "${1:-}" = slowest ]; then
    [ $# -eq 2 ] || { echo "usage: test-step.sh slowest REPORT" >&2; exit 2; }
    slowest "$2" 10
    exit 0
fi

budget=120
while [ $# -gt 0 ]; do
    case $1 in
        --budget) budget=$2; shift 2 ;;
        --) shift; break ;;
        *) echo "test-step: unknown argument $1" >&2; exit 2 ;;
    esac
done
[ $# -gt 0 ] || { echo "usage: test-step.sh [--budget SECONDS] -- CARGO_ARGUMENTS..." >&2; exit 2; }

report=${TEST_STEP_REPORT:-$root/target/nextest/default/junit.xml}
cargo "$@" --no-run
# A report this run did not write is an earlier run's, and is not passed off as this one's.
rm -f "$report"
started=$(date +%s)
status=0
timeout --kill-after=10 "$budget" cargo "$@" || status=$?
took=$(($(date +%s) - started))
if [ "$status" -eq 124 ] || [ "$status" -eq 137 ]; then
    echo "test-step: the tests ran over the step budget of $budget s and were stopped" >&2
fi
echo "test-step: the tests took $took s of a $budget s budget"
if [ -f "$report" ]; then
    echo "test-step: the ten slowest tests"
    slowest "$report" 10
else
    echo "test-step: this run wrote no test report at $report" >&2
fi
exit "$status"
