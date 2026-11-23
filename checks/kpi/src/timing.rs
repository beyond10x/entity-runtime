//! Timing scenarios. Their numbers are only ever compared with numbers taken on the same machine
//! in the same job (`ab`); the baseline file keeps them for the report, with the host they came
//! from.

use std::{
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use entity_eventlog::EventlogFileStore;

use crate::{
    fixture::{bridge, registry, COUNTER, LIMITS},
    stores::{fresh, postgres_url, Kind, Open},
};

/// One timing value: id, value, and whether higher is better.
pub type Value = (String, f64);

const CREATES: usize = 8;
const PRIOR_SHORT: u64 = 8;
const SHORT_SUBJECTS: usize = 8;
const PRIOR_LONG: u64 = 32;
const LONG_SUBJECTS: usize = 2;
const READS: usize = 8;
const SPAWNS: usize = 5;

/// Every timing scenario once. `entity` is the release binary built with
/// `eventlog-providers`; without it the CLI scenarios are skipped and say so.
pub fn run(work: &Path, entity: Option<&Path>) -> Vec<Value> {
    let mut values = Vec::new();
    for kind in Kind::ALL {
        if kind == Kind::Postgres && postgres_url().is_none() {
            continue;
        }
        let start = Instant::now();
        values.extend(store_pass(work, kind));
        eprintln!(
            "er-kpi: timing {:<40} {:>6.2} s",
            kind.id(),
            start.elapsed().as_secs_f64()
        );
    }
    match entity {
        Some(entity) => {
            let start = Instant::now();
            values.extend(cli(work, entity));
            eprintln!(
                "er-kpi: timing {:<40} {:>6.2} s",
                "cli",
                start.elapsed().as_secs_f64()
            );
        }
        None => eprintln!("er-kpi: no `entity` binary given; the CLI timing KPIs did not run"),
    }
    values
}

fn micros(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1e6
}

fn median(samples: &mut [f64]) -> f64 {
    samples.sort_by(f64::total_cmp);
    let middle = samples.len() / 2;
    if samples.len().is_multiple_of(2) {
        (samples[middle - 1] + samples[middle]) / 2.0
    } else {
        samples[middle]
    }
}

fn p95(samples: &mut [f64]) -> f64 {
    samples.sort_by(f64::total_cmp);
    let rank = ((samples.len() as f64) * 0.95).ceil() as usize;
    samples[rank.clamp(1, samples.len()) - 1]
}

fn timed(operation: impl FnOnce()) -> f64 {
    let start = Instant::now();
    operation();
    micros(start.elapsed())
}

fn latency(values: &mut Vec<Value>, name: String, mut samples: Vec<f64>) {
    values.push((format!("{name}.p50_us"), median(&mut samples)));
    values.push((format!("{name}.p95_us"), p95(&mut samples)));
}

/// Create, execute at 8 and 32 prior records, throughput, ids and one-row page on one provider.
fn store_pass(work: &Path, kind: Kind) -> Vec<Value> {
    let registry = registry(&[COUNTER]);
    let directory = fresh(work, &format!("timing-{}", kind.id()));
    let mut store = Open::provision(kind, &directory, &registry, kind.id());
    let prefix = kind.id();
    let mut values = Vec::new();

    let creates = (0..CREATES)
        .map(|at| timed(|| store.create(&registry, &format!("s{at:03}"))))
        .collect();
    latency(&mut values, format!("{prefix}.create"), creates);

    let commands = SHORT_SUBJECTS as u64 * (PRIOR_SHORT - 1);
    let elapsed = timed(|| {
        for at in 0..SHORT_SUBJECTS {
            for revision in 1..PRIOR_SHORT {
                store.execute(&registry, &format!("s{at:03}"), revision);
            }
        }
    });
    values.push((
        format!("{prefix}.commit.ops_per_s"),
        commands as f64 / (elapsed / 1e6),
    ));

    let short = (0..SHORT_SUBJECTS)
        .map(|at| timed(|| store.execute(&registry, &format!("s{at:03}"), PRIOR_SHORT)))
        .collect();
    latency(&mut values, format!("{prefix}.execute.prior8"), short);

    for at in 0..LONG_SUBJECTS {
        let id = format!("l{at:03}");
        store.create(&registry, &id);
        for revision in 1..PRIOR_LONG {
            store.execute(&registry, &id, revision);
        }
    }
    let long = (0..LONG_SUBJECTS)
        .map(|at| timed(|| store.execute(&registry, &format!("l{at:03}"), PRIOR_LONG)))
        .collect();
    latency(&mut values, format!("{prefix}.execute.prior32"), long);

    let expected = CREATES + LONG_SUBJECTS;
    let ids = (0..READS)
        .map(|_| timed(|| assert_eq!(store.ids("counter").len(), expected)))
        .collect();
    latency(&mut values, format!("{prefix}.ids.n10"), ids);

    if kind.pages() {
        let pages = (0..READS)
            .map(|_| timed(|| assert_eq!(store.query_one(), Some(1))))
            .collect();
        latency(&mut values, format!("{prefix}.query_one_row.n10"), pages);
    }
    store.close();
    values
}

fn spawn(entity: &Path, arguments: &[String]) -> f64 {
    let start = Instant::now();
    let status = Command::new(entity)
        .args(arguments)
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .status()
        .expect("entity runs");
    let elapsed = start.elapsed();
    assert!(status.success(), "entity {arguments:?} failed");
    elapsed.as_secs_f64() * 1e3
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_owned()).collect()
}

/// Wall time of one `entity` process: create and execute on the native FileStore and on Eventlog
/// File, and `graph --format text` on the 300-state chain (P15).
fn cli(work: &Path, entity: &Path) -> Vec<Value> {
    let registry = registry(&[COUNTER]);
    let directory = fresh(work, "timing-cli");
    let definition = directory.join("counter.yaml");
    std::fs::write(&definition, COUNTER).expect("definition");
    let graph = directory.join("graph-300.yaml");
    std::fs::write(&graph, crate::fixture::GRAPH_300).expect("graph fixture");
    let definition = definition.to_str().expect("UTF-8").to_owned();
    let mut values = Vec::new();

    let native = directory.join("native");
    {
        let mut store = Open::native_file(&native);
        for at in 0..SPAWNS {
            let id = format!("p{at:03}");
            store.create(&registry, &id);
            for revision in 1..PRIOR_SHORT {
                store.execute(&registry, &id, revision);
            }
        }
    }
    let native = native.to_str().expect("UTF-8").to_owned();
    let common = |record: String| {
        strings(&[
            "--definition",
            &definition,
            "--store",
            &native,
            "--record-id",
            &record,
            "--recorded-at",
            "2026-11-22T00:00:00Z",
            "--no-actor",
            "--format",
            "text",
        ])
    };
    let mut creates = Vec::new();
    let mut executes = Vec::new();
    for at in 0..SPAWNS {
        let mut create = strings(&["create", "--id", &format!("n{at:03}"), "--fields"]);
        create.push(r#"{"name":"counter"}"#.to_owned());
        create.extend(common(format!("n{at:03}-create")));
        creates.push(spawn(entity, &create));
        let mut execute = strings(&[
            "execute",
            "--id",
            &format!("p{at:03}"),
            "--operation",
            "view",
        ]);
        execute.extend(common(format!("p{at:03}-view")));
        executes.push(spawn(entity, &execute));
    }
    values.push(("cli.file.create.wall_ms".to_owned(), median(&mut creates)));
    values.push((
        "cli.file.execute.prior8.wall_ms".to_owned(),
        median(&mut executes),
    ));

    let root = directory.join("eventlog");
    let selection = directory.join("selection.json");
    let provisioned = Command::new(entity)
        .args(strings(&[
            "store",
            "provision-eventlog-file",
            "--root",
            root.to_str().expect("UTF-8"),
            "--scope",
            "er-kpi/cli",
            "--tenant",
            "er-kpi-cli",
            "--max-events",
            "20000",
            "--max-blobs",
            "80000",
            "--max-projection-rows",
            "80000",
            "--max-payload-bytes",
            "268435456",
            "--queue-capacity",
            "8",
            "--subject",
            "er-kpi",
            "--actor",
            "er-kpi",
            "--request-id",
            "provision",
            "--trace-id",
            "trace-provision",
            "--occurred-at",
            "2026-11-22T00:00:00Z",
        ]))
        .output()
        .expect("entity runs");
    assert!(
        provisioned.status.success(),
        "provisioning failed: {}",
        String::from_utf8_lossy(&provisioned.stderr)
    );
    std::fs::write(&selection, &provisioned.stdout).expect("selection");
    let authority: entity_eventlog::Authority = serde_json::from_value(
        serde_json::from_slice::<serde_json::Value>(&provisioned.stdout).expect("selection")
            ["authority"]
            .clone(),
    )
    .expect("authority");
    {
        let mut store = Open::EventlogFile(
            EventlogFileStore::open(&root, registry.clone(), authority, LIMITS, bridge())
                .expect("opened"),
        );
        for at in 0..SPAWNS {
            let id = format!("p{at:03}");
            store.create(&registry, &id);
            for revision in 1..PRIOR_SHORT {
                store.execute(&registry, &id, revision);
            }
        }
        store.close();
    }
    let root = root.to_str().expect("UTF-8").to_owned();
    let selection = selection.to_str().expect("UTF-8").to_owned();
    let common = |record: String| {
        strings(&[
            "--definition",
            &definition,
            "--store",
            &root,
            "--eventlog-config",
            &selection,
            "--record-id",
            &record,
            "--recorded-at",
            "2026-11-22T00:00:00Z",
            "--actor",
            "er-kpi",
            "--format",
            "text",
        ])
    };
    let mut creates = Vec::new();
    let mut executes = Vec::new();
    let mut lists = Vec::new();
    for at in 0..SPAWNS {
        let mut create = strings(&["create", "--id", &format!("n{at:03}"), "--fields"]);
        create.push(r#"{"name":"counter"}"#.to_owned());
        create.extend(common(format!("n{at:03}-create")));
        creates.push(spawn(entity, &create));
        let mut execute = strings(&[
            "execute",
            "--id",
            &format!("p{at:03}"),
            "--operation",
            "view",
        ]);
        execute.extend(common(format!("p{at:03}-view")));
        executes.push(spawn(entity, &execute));
        lists.push(spawn(
            entity,
            &strings(&[
                "list",
                "--store",
                &root,
                "--eventlog-config",
                &selection,
                "--entity",
                "absent",
            ]),
        ));
    }
    values.push((
        "cli.eventlog_file.create.wall_ms".to_owned(),
        median(&mut creates),
    ));
    values.push((
        "cli.eventlog_file.execute.prior8.wall_ms".to_owned(),
        median(&mut executes),
    ));
    values.push((
        "cli.eventlog_file.list_empty.wall_ms".to_owned(),
        median(&mut lists),
    ));

    let graph = graph.to_str().expect("UTF-8").to_owned();
    let mut graphs = (0..SPAWNS)
        .map(|_| spawn(entity, &strings(&["graph", "--format", "text", &graph])))
        .collect::<Vec<_>>();
    values.push((
        "cli.graph_text.chain300.wall_ms".to_owned(),
        median(&mut graphs),
    ));
    values
}
