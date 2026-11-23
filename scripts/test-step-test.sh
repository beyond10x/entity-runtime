#!/bin/sh
# What scripts/test-step.sh promises, against a stand-in `cargo`: the ten slowest tests of a report,
# slowest first; a run over its budget fails and says so; a test failure keeps its exit status; a
# report the run did not write is never printed as its own. And the runner bound the step rests
# on: .config/nextest.toml ends a test at 60 s. Run by `task test-step-check` and by CI.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
step="$here/test-step.sh"
failures=0
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

check() {
    name=$1
    expected=$2
    actual=$3
    if [ "$actual" = "$expected" ]; then
        echo "ok   $name"
    else
        echo "FAIL $name: expected [$expected], got [$actual]"
        failures=$((failures + 1))
    fi
}

# A report of twelve cases over two binaries, one of them killed at the bound.
report="$work/report.xml"
{
    echo '<?xml version="1.0" encoding="UTF-8"?>'
    echo '<testsuites name="nextest-run" tests="12" failures="1" errors="0" time="99.0">'
    echo '    <testsuite name="crate-a::alpha" tests="7" errors="0" failures="1">'
    for case in t01:1.500 t02:0.009 t03:12.250 t04:3.000 t06:9.900 t07:0.100; do
        echo "        <testcase name=\"${case%%:*}\" classname=\"crate-a::alpha\" timestamp=\"2026-11-23T00:00:00+01:00\" time=\"${case#*:}\"/>"
    done
    echo '        <testcase name="t05" classname="crate-a::alpha" timestamp="2026-11-23T00:00:00+01:00" time="61.042">'
    echo '            <failure type="test failure">killed</failure>'
    echo '        </testcase>'
    echo '    </testsuite>'
    echo '    <testsuite name="crate-a" tests="5" errors="0" failures="0">'
    for case in u08:10.000 u09:2.000 u10:0.500 u11:0.010 u12:4.000; do
        echo "        <testcase name=\"module::${case%%:*}\" classname=\"crate-a\" timestamp=\"2026-11-23T00:00:00+01:00\" time=\"${case#*:}\"/>"
    done
    echo '    </testsuite>'
    echo '</testsuites>'
} >"$report"

printed=$(sh "$step" slowest "$report")
check "the slowest of a report are ten" "10" "$(printf '%s\n' "$printed" | wc -l | tr -d ' ')"
check "the slowest come first, a killed test among them" \
    "test-step:   61.042 s  crate-a::alpha t05" "$(printf '%s\n' "$printed" | head -n 1)"
check "the slowest are ordered by time, not by text" \
    "t05 t03 module::u08 t06 module::u12 t04 module::u09 t01 module::u10 t07" \
    "$(printf '%s\n' "$printed" | awk '{ printf "%s%s", sep, $NF; sep = " " }')"

# A copy of the step in a workspace of its own, and a stand-in `cargo`: `--no-run` builds nothing;
# a run writes the report where nextest writes it, under the workspace's `target/nextest` whatever
# `CARGO_TARGET_DIR` says, unless told not to, then sleeps and exits as told.
mkdir -p "$work/bin" "$work/workspace/scripts"
cp "$step" "$work/workspace/scripts/test-step.sh"
{
    echo '#!/bin/sh'
    echo 'for argument in "$@"; do [ "$argument" = --no-run ] && exit 0; done'
    echo 'if [ -z "${FAKE_NO_REPORT:-}" ]; then'
    echo '    mkdir -p "$FAKE_WORKSPACE/target/nextest/default"'
    echo '    cp "$FAKE_REPORT" "$FAKE_WORKSPACE/target/nextest/default/junit.xml"'
    echo 'fi'
    echo 'sleep "${FAKE_SLEEP:-0}"'
    echo 'exit "${FAKE_EXIT:-0}"'
} >"$work/bin/cargo"
chmod +x "$work/bin/cargo"

run() {
    PATH="$work/bin:$PATH" CARGO_TARGET_DIR="$work/elsewhere" FAKE_REPORT="$report" \
        FAKE_WORKSPACE="$work/workspace" sh "$work/workspace/scripts/test-step.sh" "$@" \
        -- nextest run -p fixture >"$work/out" 2>&1 && echo 0 || echo $?
}

check "a green run inside its budget exits 0" "0" "$(run --budget 30)"
check "a green run names its ten slowest" "10" "$(grep -c '^test-step: .* s  ' "$work/out")"

check "a run over its budget fails" "124" "$(FAKE_SLEEP=5 run --budget 1)"
check "a run over its budget says so" "1" \
    "$(grep -c 'ran over the step budget of 1 s' "$work/out")"

check "a test failure keeps nextest's exit status" "100" "$(FAKE_EXIT=100 run --budget 30)"
check "a failed run still names its slowest" "1" \
    "$(grep -c 'test-step:   61.042 s  crate-a::alpha t05' "$work/out")"

mkdir -p "$work/workspace/target/nextest/default"
cp "$report" "$work/workspace/target/nextest/default/junit.xml"
check "a run that wrote no report exits as the run did" "0" "$(FAKE_NO_REPORT=1 run --budget 30)"
check "an earlier run's report is not printed as this run's" "0" \
    "$(grep -c ' s  crate-a' "$work/out" || true)"
check "a run that wrote no report says so" "1" "$(grep -c 'wrote no test report' "$work/out")"

config="$here/../.config/nextest.toml"
check "nextest ends a test at 60 s" "1" \
    "$(grep -c '^slow-timeout = { period = "60s", terminate-after = 1,' "$config" || true)"
check "nextest writes the report the step reads" "1" \
    "$(grep -c '^path = "junit.xml"' "$config" || true)"

# The test builds the step times: no debug information, optimised dependencies and hot crates.
profile=$(awk '/^\[/ { section = $0; next } NF { print section " " $0 }' "$here/../.cargo/config.toml")
check "test builds carry no debug information" "1" \
    "$(printf '%s\n' "$profile" | grep -c '^\[profile.dev\] debug = false$' || true)"
check "test builds optimise every dependency" "1" \
    "$(printf '%s\n' "$profile" | grep -c '^\[profile.dev.package."\*"\] opt-level = 2$' || true)"
for crate in entity-core entity-store entity-eventlog; do
    check "test builds optimise $crate" "1" \
        "$(printf '%s\n' "$profile" | grep -c "^\[profile.dev.package.$crate\] opt-level = 1$" || true)"
done

if [ "$failures" -ne 0 ]; then
    echo "$failures check(s) failed"
    exit 1
fi
echo "test-step: every check passed"
