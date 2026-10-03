//! Execute declared ESS suites against the actual ER library implementations.
mod common;
mod core;
mod executor;
mod identity;
#[cfg(feature = "eventlog")]
mod provider;
mod query;
mod shell;
mod store;
mod target;

use clap::{Parser, Subcommand};
use ess_compiler::{resolve::compile, source::SourceMap};
use ess_conformance::{
    coverage::{Origins, Scope},
    coverage_build::{self, CoverageSource},
    AdmittedSuite, Clock, CountReport, CountRun, CountStatus, Ids, Runner, RunnerConfig,
};
use ess_domain::{spec::RawSpecFile, system::Source, Specification};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

// Evidence records when execution happened. Fixture timestamps remain explicit scenario inputs.
// Monotonic elapsed time avoids wall-clock corrections; advancing reads bound runner deadlines.
struct EvidenceClock {
    epoch_ms: u64,
    started: std::time::Instant,
    last: u64,
}

impl EvidenceClock {
    fn new() -> Result<Self> {
        Ok(Self {
            epoch_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_millis()
                .try_into()?,
            started: std::time::Instant::now(),
            last: 0,
        })
    }
}

impl Clock for EvidenceClock {
    fn now(&mut self) -> ess_primitives::time::Timestamp {
        loop {
            let now = self.epoch_ms + self.started.elapsed().as_millis() as u64;
            if now > self.last {
                self.last = now;
                return ess_primitives::time::Timestamp::from_epoch_millis(now);
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
}

#[derive(Parser)]
#[command(about = "Admit and execute ER's exact ESS library contracts")]
struct Args {
    #[arg(long, default_value = ".", global = true)]
    root: PathBuf,
    /// Specification directory relative to the repository root.
    #[arg(long, default_value = "ess", global = true)]
    spec_root: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Validate, detect generated drift and coverage removal, then run every scenario.
    Check {
        #[arg(long, default_value = "target/ess-conformance")]
        reports: PathBuf,
    },
    /// Regenerate the canonical IR and suite; changed coverage requires an explicit review reason.
    Regenerate {
        #[arg(long)]
        coverage_review: Option<String>,
    },
    /// Run an admitted suite, optionally selecting one exact scenario for mutation evidence.
    Run {
        #[arg(long)]
        suite: Option<PathBuf>,
        #[arg(long)]
        scenario: Option<String>,
        #[arg(long, default_value = "target/ess-conformance")]
        reports: PathBuf,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    format: String,
    specification: Vec<String>,
    scenarios: Vec<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Baseline {
    format: String,
    review: String,
    scenarios: Vec<String>,
    #[serde(default)]
    contracts: std::collections::BTreeMap<String, String>,
}

fn read(root: &Path, relative: &str) -> Result<String> {
    let path = Path::new(relative);
    if path.is_absolute()
        || path
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Err(format!("manifest input must be a relative path: {relative}").into());
    }
    Ok(fs::read_to_string(root.join(path))?)
}

fn generate(ess_root: &Path) -> Result<(String, String)> {
    let manifest: Manifest =
        serde_yaml_ng::from_str(&fs::read_to_string(ess_root.join("ess-inputs.yaml"))?)?;
    if manifest.format != "ess-inputs/1" {
        return Err("unsupported manifest".into());
    }
    let mut sources = SourceMap::new();
    let mut files = Vec::new();
    for path in manifest.specification {
        let text = read(ess_root, &path)?;
        let raw = RawSpecFile::parse(&text).map_err(|e| format!("{path}: {e}"))?;
        sources.insert(path.as_str(), text.as_str());
        files.push((Source::new(path), raw));
    }
    let spec = Specification::assemble(files).map_err(|e| e.to_string())?;
    let ir = compile(&spec, &sources).map_err(|e| e.to_string())?;
    #[cfg(feature = "eventlog")]
    provider::check_model(&ir.to_canonical_json())?;
    let authored = manifest
        .scenarios
        .iter()
        .map(|path| Ok(CoverageSource::new(path.clone(), read(ess_root, path)?)?))
        .collect::<Result<Vec<_>>>()?;
    let input =
        coverage_build::build(&ir, &authored, Scope::System, Origins::GeneratedAndAuthored)?;
    let suite = input.selected();
    if suite.suite().is_empty() {
        return Err("empty conformance suite".into());
    }
    let coverage = suite.coverage().ok_or("undeclared coverage")?;
    if !coverage.is_complete() {
        return Err(format!("incomplete coverage: {}", serde_json::to_string(coverage)?).into());
    }
    Ok((ir.to_canonical_json(), suite.original_json().to_owned()))
}

fn exact(path: &Path, expected: &str) -> Result<()> {
    if fs::read_to_string(path)? != expected {
        return Err(format!("generated drift: {}", path.display()).into());
    }
    Ok(())
}

fn scenario_ids(suite: &AdmittedSuite) -> Vec<String> {
    suite
        .suite()
        .scenarios
        .keys()
        .map(ToString::to_string)
        .collect()
}

fn coverage(ess_root: &Path, suite: &AdmittedSuite, review: Option<&str>) -> Result<()> {
    let path = ess_root.join("coverage.json");
    let scenarios = scenario_ids(suite);
    let contracts = suite
        .suite()
        .scenarios
        .iter()
        .map(|(id, scenario)| {
            let digest = Sha256::digest(serde_json::to_vec(scenario)?);
            Ok((id.to_string(), format!("sha256:{digest:x}")))
        })
        .collect::<Result<std::collections::BTreeMap<_, _>>>()?;
    let old = fs::read_to_string(&path)
        .ok()
        .map(|s| serde_json::from_str::<Baseline>(&s))
        .transpose()?;
    if let Some(old) = &old {
        if !matches!(
            old.format.as_str(),
            "er-ess-coverage/1" | "er-ess-coverage/2"
        ) || old.review.trim().is_empty()
        {
            return Err("invalid coverage baseline".into());
        }
        if old.format == "er-ess-coverage/2"
            && old.scenarios == scenarios
            && old.contracts == contracts
        {
            return Ok(());
        }
    }
    let reason = review.filter(|s| !s.trim().is_empty()).ok_or(
        "scenario inventory or assertions changed: explicit --coverage-review <reason> required",
    )?;
    let baseline = Baseline {
        format: "er-ess-coverage/2".into(),
        review: reason.into(),
        scenarios,
        contracts,
    };
    fs::write(
        path,
        format!("{}\n", serde_json::to_string_pretty(&baseline)?),
    )?;
    Ok(())
}

fn run(root: &Path, suite: &AdmittedSuite, reports: &Path) -> Result<()> {
    let source = identity::source_identity(root, |_| {})?;
    if source != env!("ER_ESS_SOURCE_IDENTITY") {
        return Err(
            "checker binary is stale for these sources; rebuild with cargo run --locked".into(),
        );
    }
    let executable = Sha256::digest(fs::read(std::env::current_exe()?)?);
    let implementation = format!("{source};executable-sha256:{executable:x}");
    let target = target::Target::new(implementation.clone());
    let executed = Runner::new(
        RunnerConfig::default(),
        EvidenceClock::new()?,
        Ids::for_suite(suite.suite()),
    )
    .run_admitted(suite, &target);
    let report = CountReport::from_run(&executed, suite)?;
    let details = CountRun::from_run(&executed, suite)?;
    let dir = root.join(reports);
    fs::create_dir_all(&dir)?;
    fs::write(dir.join("suite.json"), suite.original_json())?;
    fs::write(dir.join("report.json"), report.to_canonical_json()?)?;
    fs::write(dir.join("run.json"), details.to_canonical_json()?)?;
    fs::write(
        dir.join("implementation.txt"),
        format!("{implementation}\n"),
    )?;
    fs::write(dir.join("observations.json"), target.observations()?)?;
    println!("{}", serde_json::to_string(report.counts())?);
    if report.execution_status() != CountStatus::Passed
        || report.conformance_status() != CountStatus::Passed
        || report.counts().total == 0
    {
        return Err(format!("conformance failed; see {}", dir.join("run.json").display()).into());
    }
    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();
    let root = args.root.canonicalize()?;
    let ess_root = root.join(args.spec_root).canonicalize()?;
    if !ess_root.starts_with(&root) {
        return Err("specification directory must be inside the repository".into());
    }
    match args.command {
        Command::Regenerate { coverage_review } => {
            let (ir, suite) = generate(&ess_root)?;
            let second = generate(&ess_root)?;
            if (ir.as_str(), suite.as_str()) != (second.0.as_str(), second.1.as_str()) {
                return Err("nondeterministic regeneration".into());
            }
            coverage(
                &ess_root,
                &AdmittedSuite::from_json(&suite)?,
                coverage_review.as_deref(),
            )?;
            fs::create_dir_all(ess_root.join("generated"))?;
            fs::write(ess_root.join("generated/model.json"), ir)?;
            fs::write(ess_root.join("generated/suite.json"), suite)?;
        }
        Command::Check { reports } => {
            let (ir, suite) = generate(&ess_root)?;
            let second = generate(&ess_root)?;
            if (ir.as_str(), suite.as_str()) != (second.0.as_str(), second.1.as_str()) {
                return Err("nondeterministic regeneration".into());
            }
            exact(&ess_root.join("generated/model.json"), &ir)?;
            exact(&ess_root.join("generated/suite.json"), &suite)?;
            let admitted = AdmittedSuite::from_json(&suite)?;
            coverage(&ess_root, &admitted, None)?;
            run(&root, &admitted, &reports)?;
        }
        Command::Run {
            suite,
            scenario,
            reports,
        } => {
            let text = fs::read_to_string(match suite {
                Some(path) => root.join(path),
                None => ess_root.join("generated/suite.json"),
            })?;
            let admitted = AdmittedSuite::from_json(&text)?;
            if let Some(id) = scenario {
                let selected = admitted
                    .suite()
                    .scenarios
                    .keys()
                    .find(|key| key.to_string() == id)
                    .ok_or("unknown exact scenario ID")?
                    .clone();
                let input = ess_conformance::coverage::AdmittedInput::from_suite(admitted)?
                    .select(&[selected])?;
                run(&root, input.selected(), &reports)?;
            } else {
                run(&root, &admitted, &reports)?;
            }
        }
    }
    Ok(())
}
