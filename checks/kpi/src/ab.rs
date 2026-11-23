//! Timing A/B on one machine: two source trees, one harness, interleaved rounds.
//!
//! Each revision is exported with `git archive` (no worktree is registered), the harness of the
//! tree running `ab` is copied over its `checks/kpi`, and both are built into one shared target
//! directory, so dependencies compile once. Rounds alternate the order (base first, then head
//! first) so drift in the machine's load falls on both sides.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::{Command, ExitCode, Stdio},
};

use crate::{defs, defs::Direction, TimingSummary, Values};

fn git(arguments: &[&str]) -> String {
    let output = Command::new("git")
        .args(arguments)
        .output()
        .expect("git runs");
    assert!(
        output.status.success(),
        "git {arguments:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

fn run_checked(command: &mut Command, what: &str) {
    let status = command
        .status()
        .unwrap_or_else(|error| panic!("{what}: {error}"));
    assert!(status.success(), "{what} failed");
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("directory");
    for entry in std::fs::read_dir(from).expect("readable") {
        let entry = entry.expect("entry");
        let name = entry.file_name();
        if name == "target" {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            copy_tree(&path, &to.join(&name));
        } else {
            std::fs::copy(&path, to.join(&name)).expect("copied");
        }
    }
}

/// One side: a source tree and the two binaries built from it.
struct Side {
    label: String,
    kpi: PathBuf,
    entity: PathBuf,
}

fn prepare(repository: &Path, revision: &str, label: &str, toolchain: &str, dir: &Path) -> Side {
    let source = dir.join(format!("src-{label}"));
    if source.exists() {
        std::fs::remove_dir_all(&source).expect("old tree removed");
    }
    std::fs::create_dir_all(&source).expect("tree directory");
    if revision == "WORKTREE" {
        let files = git(&["ls-files", "--cached", "--others", "--exclude-standard"]);
        for file in files.lines() {
            let from = repository.join(file);
            if from.is_file() {
                let to = source.join(file);
                std::fs::create_dir_all(to.parent().expect("parent")).expect("directory");
                std::fs::copy(&from, &to).expect("copied");
            }
        }
    } else {
        let commit = git(&["rev-parse", &format!("{revision}^{{commit}}")]);
        let mut archive = Command::new("git")
            .args(["archive", "--format=tar", &commit])
            .current_dir(repository)
            .stdout(Stdio::piped())
            .spawn()
            .expect("git archive runs");
        let stdout = archive.stdout.take().expect("archive stream");
        run_checked(
            Command::new("tar")
                .args(["-x", "-C"])
                .arg(&source)
                .stdin(stdout),
            "tar -x",
        );
        assert!(archive.wait().expect("git archive").success());
    }
    let harness = source.join("checks/kpi");
    if harness.exists() {
        std::fs::remove_dir_all(&harness).expect("old harness removed");
    }
    copy_tree(&repository.join("checks/kpi"), &harness);
    // A revision that predates the `kpi-counters` hooks has no such feature in its crates; the
    // timing build never turns `counters` on, so the overlay drops what it would name.
    let manifest = harness.join("Cargo.toml");
    let text = std::fs::read_to_string(&manifest).expect("harness manifest");
    let start = text.find("counters = [").expect("counters feature");
    let end = start + text[start..].find(']').expect("feature list end") + 1;
    std::fs::write(
        &manifest,
        format!("{}counters = []{}", &text[..start], &text[end..]),
    )
    .expect("harness manifest written");

    // One target directory per side. Cargo names a path package's artifacts by its path relative
    // to the workspace root, so two trees sharing a target directory share `entity-core` and the
    // rest: a side could run the other side's build. Separate directories also keep any other
    // build from replacing `release/entity` before the copy below.
    let target = dir.join(format!("target-{label}"));
    let cargo = |arguments: &[&str], what: &str| {
        run_checked(
            Command::new("cargo")
                .arg(format!("+{toolchain}"))
                .args(arguments)
                .env("CARGO_TARGET_DIR", &target)
                .current_dir(&source),
            what,
        );
    };
    cargo(
        &[
            "build",
            "--release",
            "--manifest-path",
            "checks/kpi/Cargo.toml",
            "--no-default-features",
        ],
        &format!("building the harness for {label}"),
    );
    cargo(
        &[
            "build",
            "--release",
            "--locked",
            "-p",
            "entity-cli",
            "--features",
            "eventlog-providers",
        ],
        &format!("building entity for {label}"),
    );
    let bin = dir.join(format!("bin-{label}"));
    std::fs::create_dir_all(&bin).expect("bin directory");
    let kpi = bin.join("er-kpi");
    let entity = bin.join("entity");
    std::fs::copy(target.join("release/er-kpi"), &kpi).expect("harness copied");
    std::fs::copy(target.join("release/entity"), &entity).expect("entity copied");
    Side {
        label: label.to_owned(),
        kpi,
        entity,
    }
}

fn round(side: &Side, dir: &Path, round: usize) -> Values {
    let json = dir.join(format!("round-{}-{round}.json", side.label));
    run_checked(
        Command::new(&side.kpi)
            .arg("run")
            .arg("--timing-only")
            .arg("--json")
            .arg(&json)
            .arg("--entity")
            .arg(&side.entity)
            .arg("--work")
            .arg(dir.join(format!("work-{}", side.label))),
        &format!("timing round {round} of {}", side.label),
    );
    serde_json::from_str(&std::fs::read_to_string(&json).expect("round output")).expect("round")
}

/// Builds both sides, runs `rounds` interleaved rounds, prints the comparison, fails above a
/// KPI's `max_ratio`.
pub fn run(base: &str, head: &str, rounds: usize, toolchain: &str, dir: &Path) -> ExitCode {
    let repository = PathBuf::from(git(&["rev-parse", "--show-toplevel"]));
    let dir = if dir.is_absolute() {
        dir.to_path_buf()
    } else {
        repository.join(dir)
    };
    std::fs::create_dir_all(&dir).expect("ab directory");
    let definitions = defs::load();
    let base_side = prepare(&repository, base, "base", toolchain, &dir);
    let head_side = prepare(&repository, head, "head", toolchain, &dir);
    let mut samples: BTreeMap<String, (Vec<f64>, Vec<f64>)> = BTreeMap::new();
    for number in 0..rounds {
        let order = if number % 2 == 0 {
            [&base_side, &head_side]
        } else {
            [&head_side, &base_side]
        };
        for side in order {
            let values = round(side, &dir, number);
            for (id, value) in values.timing {
                let entry = samples.entry(id).or_default();
                if side.label == "base" {
                    entry.0.push(value);
                } else {
                    entry.1.push(value);
                }
            }
        }
    }
    println!(
        "timing A/B: base {base} vs head {head}, {rounds} interleaved rounds, best round per side (min time, max throughput)"
    );
    println!(
        "{:<52} {:>12} {:>7} {:>12} {:>7} {:>7} {:>6}  verdict",
        "kpi", "base", "spread", "head", "spread", "ratio", "max"
    );
    let mut failures = Vec::new();
    for (id, (base_values, head_values)) in samples {
        if base_values.is_empty() || head_values.is_empty() {
            println!("{id:<52} measured on one side only");
            continue;
        }
        let base_summary = TimingSummary::of(base_values);
        let head_summary = TimingSummary::of(head_values);
        let definition = definitions.timing.get(&id);
        let (direction, gate, max_ratio) = definition
            .map_or((Direction::None, false, 0.0), |kpi| {
                (kpi.direction, kpi.gate, kpi.max_ratio)
            });
        // Load on a shared machine only ever adds time, so the best round of each side is the
        // steadiest estimate: an A/A run at load 20-200 on 20 CPUs showed best-of-rounds within
        // 1.43x for 9 in 10 gated KPIs, medians of rounds within 1.66x.
        let (base_best, head_best, worse_ratio) = match direction {
            Direction::Lower => (
                base_summary.min,
                head_summary.min,
                head_summary.min / base_summary.min.max(f64::EPSILON),
            ),
            Direction::Higher => (
                base_summary.max,
                head_summary.max,
                base_summary.max / head_summary.max.max(f64::EPSILON),
            ),
            Direction::None => (base_summary.median, head_summary.median, 1.0),
        };
        let verdict = if definition.is_none() {
            "undefined"
        } else if !gate {
            "info"
        } else if worse_ratio > max_ratio {
            failures.push(format!(
                "{id}: {:.2} -> {:.2} is {worse_ratio:.2}x worse (max {max_ratio:.2}x)",
                base_best, head_best
            ));
            "REGRESSION"
        } else if worse_ratio < 1.0 / max_ratio {
            "improved"
        } else {
            "ok"
        };
        println!(
            "{id:<52} {:>12.2} {:>6.1}% {:>12.2} {:>6.1}% {worse_ratio:>7.2} {:>6}  {verdict}",
            base_best,
            base_summary.spread() * 100.0,
            head_best,
            head_summary.spread() * 100.0,
            if gate {
                format!("{max_ratio:.2}")
            } else {
                "-".into()
            },
        );
    }
    if failures.is_empty() {
        println!("er-kpi ab: no gated timing KPI regressed");
        ExitCode::SUCCESS
    } else {
        for failure in &failures {
            eprintln!("er-kpi ab: {failure}");
        }
        ExitCode::FAILURE
    }
}
