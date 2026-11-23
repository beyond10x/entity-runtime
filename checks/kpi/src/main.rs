//! The KPI gate of entity-runtime.
//!
//! Two kinds of KPI, because CI hardware differs from a workstation and a workstation is shared:
//!
//! * **Counters** — bytes read and written, files opened, fsyncs, allocation bytes, history
//!   loads, kernel calls, plan rows. The same tree gives the same number on any machine, so they
//!   are compared with the committed `baseline.json` (`check`).
//! * **Timing** — latency and throughput. Never compared with a number from another machine:
//!   `ab` builds two revisions and runs them interleaved on one runner. `record` keeps the local
//!   numbers in the baseline for the report, with the host they came from.

mod ab;
mod conflict;
mod counters;
mod defs;
mod fixture;
mod pg;
mod probe;
mod stores;
mod timing;

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::ExitCode,
    time::Instant,
};

use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};

use defs::Direction;

/// The repository's KPI gate.
#[derive(Parser)]
#[command(name = "er-kpi", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Print every KPI in `kpis.yaml`: id, unit, direction, gate, finding and scenario.
    List,
    /// Run the scenarios once and write the raw values as JSON.
    Run {
        /// Where to write the values.
        #[arg(long)]
        json: PathBuf,
        /// Only the timing scenarios.
        #[arg(long)]
        timing_only: bool,
        /// Release `entity` binary built with `eventlog-providers`, for the CLI timing KPIs.
        #[arg(long)]
        entity: Option<PathBuf>,
        /// Scratch directory, emptied first [default: $CARGO_TARGET_DIR/kpi-work, else
        /// target/kpi-work].
        #[arg(long)]
        work: Option<PathBuf>,
    },
    /// Measure counters twice (they must agree) and timing over rounds, and write the baseline.
    Record {
        /// Release `entity` binary built with `eventlog-providers`.
        #[arg(long)]
        entity: Option<PathBuf>,
        /// Timing rounds.
        #[arg(long, default_value_t = 3)]
        rounds: usize,
        /// Baseline file.
        #[arg(long, default_value = "checks/kpi/baseline.json")]
        baseline: PathBuf,
        /// Scratch directory, emptied first [default: $CARGO_TARGET_DIR/kpi-work, else
        /// target/kpi-work].
        #[arg(long)]
        work: Option<PathBuf>,
    },
    /// Compare counters with the baseline (the gate); show one timing round beside the local
    /// baseline numbers for information.
    Check {
        /// Release `entity` binary built with `eventlog-providers`.
        #[arg(long)]
        entity: Option<PathBuf>,
        /// Also run one informational timing round (slow on a loaded machine; timing gates only
        /// in `ab`).
        #[arg(long)]
        timing: bool,
        /// Baseline file.
        #[arg(long, default_value = "checks/kpi/baseline.json")]
        baseline: PathBuf,
        /// Scratch directory, emptied first [default: $CARGO_TARGET_DIR/kpi-work, else
        /// target/kpi-work].
        #[arg(long)]
        work: Option<PathBuf>,
    },
    /// Timing A/B: build both revisions with this harness, run them interleaved, compare medians.
    Ab {
        /// Base revision (the merge-base with main, for a pull request).
        #[arg(long)]
        base: String,
        /// Candidate revision; `WORKTREE` measures the working tree as it is.
        #[arg(long, default_value = "HEAD")]
        head: String,
        /// Interleaved rounds per side.
        #[arg(long, default_value_t = 3)]
        rounds: usize,
        /// Rust toolchain for both builds.
        #[arg(long, default_value = "1.91.0")]
        toolchain: String,
        /// Where the trees, builds and scratch live [default: $CARGO_TARGET_DIR/kpi-ab, else
        /// target/kpi-ab].
        #[arg(long)]
        dir: Option<PathBuf>,
    },
}

/// Raw values of one run.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Values {
    /// Counter values by id.
    pub counters: BTreeMap<String, u64>,
    /// Timing values by id.
    pub timing: BTreeMap<String, f64>,
}

/// The committed baseline.
#[derive(Debug, Serialize, Deserialize)]
struct Baseline {
    /// Where and with what the numbers were taken.
    host: BTreeMap<String, String>,
    /// Counter values by id.
    counters: BTreeMap<String, u64>,
    /// Local timing by id, for the report only.
    timing: BTreeMap<String, TimingSummary>,
}

/// One timing KPI over rounds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimingSummary {
    /// Median over rounds.
    pub median: f64,
    /// Smallest round.
    pub min: f64,
    /// Largest round.
    pub max: f64,
}

impl TimingSummary {
    fn of(mut samples: Vec<f64>) -> Self {
        samples.sort_by(f64::total_cmp);
        let middle = samples.len() / 2;
        let median = if samples.len().is_multiple_of(2) {
            (samples[middle - 1] + samples[middle]) / 2.0
        } else {
            samples[middle]
        };
        Self {
            median: round(median),
            min: round(samples[0]),
            max: round(samples[samples.len() - 1]),
        }
    }

    /// (max - min) / median.
    pub fn spread(&self) -> f64 {
        if self.median == 0.0 {
            0.0
        } else {
            (self.max - self.min) / self.median
        }
    }
}

fn round(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

fn measure(work: &Path, timing_only: bool, entity: Option<&Path>) -> Values {
    if work.exists() {
        std::fs::remove_dir_all(work).expect("old scratch removed");
    }
    std::fs::create_dir_all(work).expect("scratch");
    let mut values = Values::default();
    if !timing_only {
        let start = Instant::now();
        for (id, value) in counters::run(work) {
            let previous = values.counters.insert(id.clone(), value);
            assert!(previous.is_none(), "counter {id} measured twice");
        }
        eprintln!(
            "er-kpi: counters took {:.1} s",
            start.elapsed().as_secs_f64()
        );
    }
    let start = Instant::now();
    for (id, value) in timing::run(work, entity) {
        values.timing.insert(id, round(value));
    }
    eprintln!("er-kpi: timing took {:.1} s", start.elapsed().as_secs_f64());
    values
}

/// Fails when a measured id has no definition, or a defined id (that should run here) was not
/// measured: the file and the scenarios cannot drift apart.
fn agree(definitions: &defs::Expanded, values: &Values, timing: bool) -> Vec<String> {
    let mut problems = Vec::new();
    let postgres = stores::postgres_url().is_some();
    for id in values.counters.keys() {
        if !definitions.counters.contains_key(id) {
            problems.push(format!(
                "counter {id} is measured but not defined in kpis.yaml"
            ));
        }
    }
    for (id, counter) in &definitions.counters {
        let expected =
            (postgres || !counter.postgres) && (cfg!(feature = "counters") || !counter.hooked);
        if expected && !values.counters.contains_key(id) {
            problems.push(format!(
                "counter {id} is defined in kpis.yaml but was not measured"
            ));
        }
    }
    if timing {
        for id in values.timing.keys() {
            if !definitions.timing.contains_key(id) {
                problems.push(format!(
                    "timing {id} is measured but not defined in kpis.yaml"
                ));
            }
        }
    }
    problems
}

fn host(entity: Option<&Path>) -> BTreeMap<String, String> {
    let read = |path: &str| std::fs::read_to_string(path).unwrap_or_default();
    let cpu = read("/proc/cpuinfo")
        .lines()
        .find(|line| line.starts_with("model name"))
        .and_then(|line| line.split(':').nth(1))
        .unwrap_or("unknown")
        .trim()
        .to_owned();
    // The toolchain this harness was built with (rustup names it to rustc), not the default one.
    let rustc = option_env!("RUSTUP_TOOLCHAIN")
        .unwrap_or("unknown")
        .to_owned();
    let revision = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .unwrap_or_default();
    let dirty = std::process::Command::new("git")
        .args(["status", "--porcelain", "--untracked-files=no"])
        .output()
        .is_ok_and(|output| !output.stdout.is_empty());
    let revision = if dirty {
        format!("{revision} plus uncommitted changes")
    } else {
        revision
    };
    let mut host = BTreeMap::new();
    host.insert(
        "hostname".into(),
        read("/proc/sys/kernel/hostname").trim().to_owned(),
    );
    host.insert("cpu".into(), cpu);
    host.insert(
        "cpus".into(),
        std::thread::available_parallelism()
            .map_or(0, usize::from)
            .to_string(),
    );
    host.insert("loadavg".into(), read("/proc/loadavg").trim().to_owned());
    host.insert("rustc".into(), rustc);
    host.insert("revision".into(), revision);
    host.insert(
        "postgres".into(),
        if stores::postgres_url().is_some() {
            "ENTITY_POSTGRES_URL set".into()
        } else {
            "not measured".into()
        },
    );
    host.insert(
        "entity_binary".into(),
        entity.map_or("not given".into(), |path| {
            path.file_name()
                .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
        }),
    );
    host
}

/// Allowed absolute change of a counter.
fn allowance(counter: &defs::Counter, base: u64) -> u64 {
    ((base as f64) * counter.tolerance).ceil() as u64 + counter.slack
}

fn counter_verdict(counter: &defs::Counter, base: u64, now: u64) -> (&'static str, bool) {
    let allowed = allowance(counter, base);
    let worse = match counter.direction {
        Direction::Lower => now > base + allowed,
        Direction::Higher => now + allowed < base,
        Direction::None => false,
    };
    let better = match counter.direction {
        Direction::Lower => now + allowed < base,
        Direction::Higher => now > base + allowed,
        Direction::None => false,
    };
    if worse {
        ("REGRESSION", true)
    } else if better {
        ("improved: re-record", false)
    } else if counter.direction == Direction::None && now.abs_diff(base) > allowed {
        ("changed: re-record", false)
    } else {
        ("ok", false)
    }
}

fn delta(base: f64, now: f64) -> String {
    if base == 0.0 {
        if now == 0.0 {
            "0".into()
        } else {
            "new".into()
        }
    } else {
        format!("{:+.1}%", (now - base) / base * 100.0)
    }
}

fn record(entity: Option<&Path>, rounds: usize, baseline: &Path, work: &Path) -> ExitCode {
    let definitions = defs::load();
    let first = measure(work, false, entity);
    let mut problems = agree(&definitions, &first, true);
    let second = measure(work, true, entity);
    let again = {
        let mut values = Values::default();
        let start = Instant::now();
        for (id, value) in counters::run(work) {
            values.counters.insert(id, value);
        }
        eprintln!(
            "er-kpi: counters (second run) took {:.1} s",
            start.elapsed().as_secs_f64()
        );
        values
    };
    for (id, value) in &first.counters {
        let counter = &definitions.counters[id];
        let other = again.counters.get(id).copied().unwrap_or(0);
        if value.abs_diff(other) > allowance(counter, *value) {
            problems.push(format!(
                "counter {id} is not deterministic within its tolerance: {value} then {other}"
            ));
        }
    }
    let mut samples: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    for values in [&first, &second] {
        for (id, value) in &values.timing {
            samples.entry(id.clone()).or_default().push(*value);
        }
    }
    for _ in 2..rounds {
        for (id, value) in measure(work, true, entity).timing {
            samples.entry(id).or_default().push(value);
        }
    }
    if !problems.is_empty() {
        for problem in &problems {
            eprintln!("er-kpi: {problem}");
        }
        return ExitCode::FAILURE;
    }
    let baseline_value = Baseline {
        host: host(entity),
        counters: first.counters,
        timing: samples
            .into_iter()
            .map(|(id, values)| (id, TimingSummary::of(values)))
            .collect(),
    };
    let mut text = serde_json::to_string_pretty(&baseline_value).expect("baseline serializes");
    text.push('\n');
    std::fs::write(baseline, text).expect("baseline written");
    println!(
        "er-kpi: wrote {} ({} counters, {} timing KPIs)",
        baseline.display(),
        baseline_value.counters.len(),
        baseline_value.timing.len()
    );
    for (id, summary) in &baseline_value.timing {
        println!(
            "  {id:<52} median {:>12.2}  spread {:>5.1}%",
            summary.median,
            summary.spread() * 100.0
        );
    }
    ExitCode::SUCCESS
}

fn check(entity: Option<&Path>, timing: bool, baseline: &Path, work: &Path) -> ExitCode {
    let started = Instant::now();
    let definitions = defs::load();
    let baseline: Baseline = serde_json::from_str(
        &std::fs::read_to_string(baseline).expect("baseline readable; run `record` first"),
    )
    .expect("baseline parses");
    let values = if !timing {
        let mut values = Values::default();
        if work.exists() {
            std::fs::remove_dir_all(work).expect("old scratch removed");
        }
        std::fs::create_dir_all(work).expect("scratch");
        values.counters = counters::run(work).into_iter().collect();
        values
    } else {
        measure(work, false, entity)
    };
    let mut problems = agree(&definitions, &values, timing);
    let postgres = stores::postgres_url().is_some();
    println!(
        "{:<56} {:>14} {:>14} {:>9}  verdict",
        "counter", "baseline", "now", "delta"
    );
    for (id, counter) in &definitions.counters {
        let Some(now) = values.counters.get(id).copied() else {
            let reason = if counter.postgres && !postgres {
                "skipped: ENTITY_POSTGRES_URL unset"
            } else if counter.hooked && !cfg!(feature = "counters") {
                "skipped: built without the counters feature"
            } else {
                "missing"
            };
            println!("{id:<56} {:>14} {:>14} {:>9}  {reason}", "", "", "");
            continue;
        };
        let Some(base) = baseline.counters.get(id).copied() else {
            println!(
                "{id:<56} {:>14} {now:>14} {:>9}  no baseline: re-record",
                "-", "new"
            );
            problems.push(format!(
                "counter {id} has no baseline value; run `task kpi-record`"
            ));
            continue;
        };
        let (verdict, failed) = counter_verdict(counter, base, now);
        println!(
            "{id:<56} {base:>14} {now:>14} {:>9}  {verdict}",
            delta(base as f64, now as f64)
        );
        if failed {
            problems.push(format!(
                "counter {id} regressed: {base} -> {now} {} (allowed {})",
                counter.unit,
                allowance(counter, base)
            ));
        }
    }
    if timing {
        println!();
        println!(
            "{:<56} {:>14} {:>14} {:>9}  (information: timing gates only in `ab`)",
            "timing, this run vs local baseline", "baseline", "now", "delta"
        );
        for (id, now) in &values.timing {
            let base = baseline.timing.get(id).map(|summary| summary.median);
            println!(
                "{id:<56} {:>14} {now:>14.2} {:>9}",
                base.map_or("-".into(), |base| format!("{base:.2}")),
                base.map_or("-".into(), |base| delta(base, *now))
            );
        }
    }
    println!(
        "er-kpi: check took {:.1} s",
        started.elapsed().as_secs_f64()
    );
    if problems.is_empty() {
        println!("er-kpi: no counter regressed");
        ExitCode::SUCCESS
    } else {
        for problem in &problems {
            eprintln!("er-kpi: {problem}");
        }
        ExitCode::FAILURE
    }
}

/// A scratch directory under the build directory, never under `/tmp`.
fn scratch(given: Option<PathBuf>, name: &str) -> PathBuf {
    given.unwrap_or_else(|| {
        std::env::var_os("CARGO_TARGET_DIR")
            .map_or_else(|| PathBuf::from("target"), PathBuf::from)
            .join(name)
    })
}

fn direction(direction: defs::Direction) -> &'static str {
    match direction {
        defs::Direction::Lower => "lower",
        defs::Direction::Higher => "higher",
        defs::Direction::None => "none",
    }
}

fn list() {
    let definitions = defs::load();
    println!(
        "{:<56} {:<10} {:<7} {:<16} {:<10} measures; scenario",
        "counter", "unit", "better", "gate", "finding"
    );
    for (id, counter) in &definitions.counters {
        let gate = match (counter.postgres, counter.hooked) {
            (true, _) => format!("±{} +{} pg", counter.tolerance, counter.slack),
            (_, true) => format!("±{} +{} hook", counter.tolerance, counter.slack),
            _ => format!("±{} +{}", counter.tolerance, counter.slack),
        };
        println!(
            "{:<56} {:<10} {:<7} {:<16} {:<10} {}; {}",
            id,
            counter.unit,
            direction(counter.direction),
            gate,
            counter.finding.as_deref().unwrap_or("-"),
            counter.measures,
            counter.scenario
        );
    }
    println!();
    println!(
        "{:<56} {:<10} {:<7} {:<16} {:<10} measures; scenario",
        "timing", "unit", "better", "ab gate", "findings"
    );
    for (id, timing) in &definitions.timing {
        let gate = if timing.gate {
            format!("ratio {}", timing.max_ratio)
        } else {
            "report".to_owned()
        };
        let findings = if timing.findings.is_empty() {
            "-".to_owned()
        } else {
            timing.findings.join(",")
        };
        println!(
            "{:<56} {:<10} {:<7} {:<16} {:<10} {}; {}",
            id,
            timing.unit,
            direction(timing.direction),
            gate,
            findings,
            timing.measures,
            timing.scenario
        );
    }
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::List => {
            list();
            ExitCode::SUCCESS
        }
        Command::Run {
            json,
            timing_only,
            entity,
            work,
        } => {
            let work = scratch(work, "kpi-work");
            let values = measure(&work, timing_only, entity.as_deref());
            std::fs::write(
                &json,
                serde_json::to_string_pretty(&values).expect("values serialize"),
            )
            .expect("values written");
            ExitCode::SUCCESS
        }
        Command::Record {
            entity,
            rounds,
            baseline,
            work,
        } => record(
            entity.as_deref(),
            rounds.max(2),
            &baseline,
            &scratch(work, "kpi-work"),
        ),
        Command::Check {
            entity,
            timing,
            baseline,
            work,
        } => check(
            entity.as_deref(),
            timing,
            &baseline,
            &scratch(work, "kpi-work"),
        ),
        Command::Ab {
            base,
            head,
            rounds,
            toolchain,
            dir,
        } => ab::run(
            &base,
            &head,
            rounds.max(1),
            &toolchain,
            &scratch(dir, "kpi-ab"),
        ),
    }
}
