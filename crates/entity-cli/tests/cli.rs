//! The `entity` command, driven as a user would drive it: files in, stdout and an exit code out.

use std::{
    fs,
    io::Write,
    path::PathBuf,
    process::{Command, Output, Stdio},
};

fn entity() -> Command {
    Command::new(env!("CARGO_BIN_EXE_entity"))
}

fn order_yaml() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/order.yaml")
}

fn run(args: &[&str], stdin: Option<&str>) -> Output {
    let mut child = entity()
        .args(args)
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the entity binary runs");
    if let Some(text) = stdin {
        child
            .stdin
            .take()
            .expect("piped stdin")
            .write_all(text.as_bytes())
            .expect("stdin written");
    }
    child.wait_with_output().expect("the entity binary exits")
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("utf-8 stdout")
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("utf-8 stderr")
}

fn scratch(name: &str, contents: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("entity-cli");
    fs::create_dir_all(&dir).expect("scratch dir");
    let path = dir.join(name);
    fs::write(&path, contents).expect("scratch file");
    path
}

fn refusal_of(output: &Output) -> serde_json::Value {
    serde_json::from_str(&stdout(output)).expect("a JSON refusal on stdout")
}

#[test]
fn validate_accepts_the_example_and_exits_zero() {
    let output = run(&["validate", order_yaml().to_str().unwrap()], None);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(
        stdout(&output).contains("valid (order v1)"),
        "{}",
        stdout(&output)
    );
    assert!(stdout(&output).contains("1 file(s), 0 invalid"));
}

#[test]
fn validate_names_the_defect_and_exits_one() {
    let broken = scratch(
        "broken.yaml",
        "entity: broken\nlifecycle: { initial: nowhere, states: [somewhere] }\nschema: {}\n",
    );
    let output = run(
        &[
            "validate",
            order_yaml().to_str().unwrap(),
            broken.to_str().unwrap(),
        ],
        None,
    );
    assert_eq!(output.status.code(), Some(1));
    let text = stdout(&output);
    assert!(text.contains("valid (order v1)"), "{text}");
    assert!(
        text.contains("invalid: lifecycle initial state 'nowhere' is not declared"),
        "{text}"
    );
    assert!(text.contains("2 file(s), 1 invalid"), "{text}");
}

/// Fixing a definition used to take as many runs of `validate` as it had faults, because each
/// run named one. This is the same file three times over in one pass.
#[test]
fn validate_prints_every_defect_of_a_file_not_only_the_first() {
    let broken = scratch(
        "three-defects.yaml",
        r#"
entity: broken
version: 0
lifecycle: { initial: nowhere, states: [somewhere] }
schema:
  fields:
    count: { type: integer, default: "many" }
operations:
  touch:
    transitions: [ { from: somewhere, to: somewhere } ]
"#,
    );
    let output = run(&["validate", broken.to_str().unwrap()], None);
    assert_eq!(output.status.code(), Some(1));

    let text = stdout(&output);
    assert!(text.contains("invalid: 3 defects"), "{text}");
    assert!(
        text.contains("entity version must be greater than zero"),
        "{text}"
    );
    assert!(
        text.contains("lifecycle initial state 'nowhere' is not declared"),
        "{text}"
    );
    assert!(text.contains("schema.count"), "{text}");
    assert!(text.contains("1 file(s), 1 invalid"), "{text}");
}

#[test]
fn validate_reports_every_file_and_a_broken_one_is_a_finding_not_a_usage_error() {
    // A syntax slip in the first example must not hide a broken lifecycle in the second: both are
    // reported, and the summary counts them.
    let syntax = scratch("syntax.yaml", "entity: [unclosed\n");
    let semantic = scratch(
        "semantic.yaml",
        "entity: sem\nlifecycle: { initial: nowhere, states: [somewhere] }\nschema: {}\n",
    );
    let output = run(
        &[
            "validate",
            syntax.to_str().unwrap(),
            semantic.to_str().unwrap(),
            order_yaml().to_str().unwrap(),
        ],
        None,
    );
    assert_eq!(output.status.code(), Some(1));
    let text = stdout(&output);
    assert!(
        text.contains("syntax.yaml: invalid: invalid entity YAML"),
        "{text}"
    );
    assert!(
        text.contains("semantic.yaml: invalid: lifecycle initial state 'nowhere' is not declared"),
        "{text}"
    );
    assert!(text.contains("valid (order v1)"), "{text}");
    assert!(text.contains("3 file(s), 2 invalid"), "{text}");
    // Only the report — no JSON refusal appended to it, so the output has one shape.
    assert!(
        !text.contains('{'),
        "validate prints lines, not a refusal object: {text}"
    );

    let output = run(&["validate", "does-not-exist.yaml"], None);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stdout(&output).contains("invalid: cannot read"),
        "{}",
        stdout(&output)
    );
}

#[test]
fn an_unparsable_definition_is_a_usage_error_with_exit_two_where_one_is_expected() {
    let garbage = scratch("garbage.yaml", "entity: [unclosed\n");
    let output = run(&["inspect", garbage.to_str().unwrap()], None);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("invalid entity YAML"),
        "{}",
        stderr(&output)
    );

    let output = run(&["graph", "does-not-exist.yaml"], None);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("cannot read"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn inspect_and_graph_describe_the_definition() {
    let output = run(&["inspect", order_yaml().to_str().unwrap()], None);
    assert_eq!(output.status.code(), Some(0));
    let text = stdout(&output);
    for expected in [
        "entity: order  version: 1",
        "draft (initial)",
        "approve: submitted -> approved",
        "precondition: positive_total — zero-value orders cannot be approved",
        "cancel: draft|submitted -> cancelled",
        "emits: OrderRejected",
    ] {
        assert!(text.contains(expected), "missing {expected:?} in\n{text}");
    }

    let output = run(&["graph", order_yaml().to_str().unwrap()], None);
    assert!(
        stdout(&output).contains("approved --fulfill--> fulfilled"),
        "{}",
        stdout(&output)
    );

    let output = run(
        &["graph", order_yaml().to_str().unwrap(), "--format", "dot"],
        None,
    );
    let dot = stdout(&output);
    // The graph is named for the *version* as well, so two versions of one entity do not
    // produce two files claiming to be the same graph.
    assert!(dot.starts_with("digraph \"order v1\" {"), "{dot}");
    assert!(
        dot.contains("\"submitted\" -> \"approved\" [label=\"approve\"];"),
        "{dot}"
    );
}

#[test]
fn graph_dot_quotes_a_name_that_would_otherwise_close_the_string() {
    // A state called `a"b` used to produce DOT no renderer accepts — and a name carrying
    // `" [label=` could have written attributes nobody asked for.
    let definition = scratch(
        "quoted.yaml",
        "entity: \"q\\\"x\"\nlifecycle: { initial: \"a\\\"b\", states: [\"a\\\"b\"] }\nschema: {}\n",
    );
    let output = run(
        &["graph", definition.to_str().unwrap(), "--format", "dot"],
        None,
    );
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let dot = stdout(&output);
    assert!(dot.starts_with(r#"digraph "q\"x v1" {"#), "{dot}");
    // The label is emitted explicitly now — a node id and its label are separate things in
    // the reference graph, and both have to survive a quote.
    assert!(
        dot.contains(r#""a\"b" [label="a\"b" peripheries=2];"#),
        "{dot}"
    );
}

#[test]
fn create_then_execute_through_a_pipe_and_a_refusal_with_its_typed_reason() {
    let definition = order_yaml();
    let definition = definition.to_str().unwrap();

    let created = run(
        &[
            "create",
            "--definition",
            definition,
            "--id",
            "ord-1",
            "--fields",
            r#"{"customer_id": "c-1", "total_cents": 0}"#,
        ],
        None,
    );
    assert_eq!(created.status.code(), Some(0), "{}", stderr(&created));
    let decision: serde_json::Value =
        serde_json::from_str(&stdout(&created)).expect("a JSON decision");
    assert_eq!(decision["instance"]["lifecycle_state"], "draft");
    assert_eq!(decision["instance"]["revision"], 1);
    assert_eq!(decision["events"][0]["type"], "OrderCreated");
    assert_eq!(
        decision["events"][0]["args"]["customer_id"], "c-1",
        "the printed event carries what it was decided on"
    );

    // The whole Decision goes back in as --instance via stdin; the command takes its instance.
    let submitted = run(
        &[
            "execute",
            "--definition",
            definition,
            "--instance",
            "-",
            "--operation",
            "submit",
            "--arguments",
            r#"{"actor": "alice"}"#,
            "--format",
            "text",
        ],
        Some(&stdout(&created)),
    );
    assert_eq!(submitted.status.code(), Some(0), "{}", stderr(&submitted));
    assert_eq!(
        stdout(&submitted),
        "order ord-1 is submitted (revision 2); events: OrderSubmitted\n"
    );

    // Now the JSON form via a file, and a precondition refusal: a zero-value order cannot be
    // approved.
    let submitted_json = run(
        &[
            "execute",
            "--definition",
            definition,
            "--instance",
            "-",
            "--operation",
            "submit",
            "--arguments",
            r#"{"actor": "alice"}"#,
        ],
        Some(&stdout(&created)),
    );
    let state = scratch("submitted.json", &stdout(&submitted_json));
    let refused = run(
        &[
            "execute",
            "--definition",
            definition,
            "--instance",
            &format!("@{}", state.display()),
            "--operation",
            "approve",
            "--arguments",
            r#"{"actor": "bob"}"#,
        ],
        None,
    );
    assert_eq!(refused.status.code(), Some(1));
    let refusal = refusal_of(&refused);
    assert_eq!(refusal["kind"], "precondition_failed");
    assert_eq!(refusal["operation"], "approve");
    assert_eq!(refusal["rule"], "positive_total");
    assert!(
        stderr(&refused).contains("refused: precondition 'positive_total' failed"),
        "{}",
        stderr(&refused)
    );

    // And a lifecycle refusal, with the state named.
    let refused = run(
        &[
            "execute",
            "--definition",
            definition,
            "--instance",
            &format!("@{}", state.display()),
            "--operation",
            "fulfill",
            "--arguments",
            r#"{"tracking_number": "T-1"}"#,
        ],
        None,
    );
    assert_eq!(refused.status.code(), Some(1));
    let refusal = refusal_of(&refused);
    assert_eq!(refusal["kind"], "invalid_transition");
    assert_eq!(refusal["state"], "submitted");
}

/// The operator-facing half of three-valued rules. A refusal that says *go and observe* without
/// saying what to observe is the prose rule this kernel replaced, printed by a program.
#[test]
fn an_unobservable_refusal_names_every_address_nobody_observed() {
    let path = scratch(
        "gate.yaml",
        r#"
entity: claim
version: 1
schema:
  fields:
    title: { type: string, required: true }
    review: { type: string }
    score: { type: integer }
lifecycle:
  initial: draft
  states: [draft, accepted]
operations:
  accept:
    preconditions:
      - name: evidenced
        assert:
          all:
            - eq: [$fields.review, approved]
            - gte: [$fields.score, 4]
        message: an accepted claim carries an approved review scoring at least four
    transitions:
      - from: draft
        to: accepted
"#,
    );
    let definition = path.to_str().unwrap();

    let created = run(
        &[
            "create",
            "--definition",
            definition,
            "--id",
            "c-1",
            "--fields",
            r#"{"title": "a claim"}"#,
        ],
        None,
    );
    assert_eq!(created.status.code(), Some(0), "{}", stderr(&created));

    let refused = run(
        &[
            "execute",
            "--definition",
            definition,
            "--instance",
            "-",
            "--operation",
            "accept",
            "--arguments",
            "{}",
        ],
        Some(&stdout(&created)),
    );
    assert_eq!(refused.status.code(), Some(1), "{}", stderr(&refused));

    let refusal = refusal_of(&refused);
    assert_eq!(refusal["kind"], "precondition_unobservable");
    assert_eq!(refusal["operation"], "accept");
    assert_eq!(refusal["rule"], "evidenced");
    // Both, not the first: one refusal the operator can act on once.
    assert_eq!(
        refusal["unresolved"],
        serde_json::json!(["$fields.review", "$fields.score"])
    );
    assert!(
        stderr(&refused).contains("nothing was observed at $fields.review, $fields.score"),
        "{}",
        stderr(&refused)
    );
}

#[test]
fn create_refuses_to_guess_between_two_definitions_and_says_how_to_choose() {
    let second = scratch(
        "second.yaml",
        "entity: other\nlifecycle: { initial: a, states: [a] }\nschema: {}\n",
    );
    let output = run(
        &[
            "create",
            "--definition",
            order_yaml().to_str().unwrap(),
            "--definition",
            second.to_str().unwrap(),
            "--id",
            "x",
        ],
        None,
    );
    // Still a refusal rather than a guess — but several definitions are ordinary now, because a
    // definition that declares a `ref` needs the type it points at registered beside it. So the
    // refusal names the way through instead of naming a restriction that no longer holds.
    assert_eq!(output.status.code(), Some(2));
    let message = stderr(&output);
    assert!(message.contains("--entity"), "{message}");
    assert!(message.contains("order"), "{message}");
    assert!(message.contains("other"), "{message}");

    // And naming it works.
    let chosen = run(
        &[
            "create",
            "--definition",
            order_yaml().to_str().unwrap(),
            "--definition",
            second.to_str().unwrap(),
            "--entity",
            "other",
            "--id",
            "x",
        ],
        None,
    );
    assert_eq!(chosen.status.code(), Some(0), "{}", stderr(&chosen));
    assert!(
        stdout(&chosen).contains("\"entity\": \"other\""),
        "{}",
        stdout(&chosen)
    );
}

#[test]
fn a_validation_refusal_lists_every_error() {
    let output = run(
        &[
            "create",
            "--definition",
            order_yaml().to_str().unwrap(),
            "--id",
            "ord-1",
            "--fields",
            r#"{"total_cents": -5, "priority": "urgent"}"#,
        ],
        None,
    );
    assert_eq!(output.status.code(), Some(1));
    let refusal = refusal_of(&output);
    assert_eq!(refusal["kind"], "validation");
    let paths: Vec<&str> = refusal["errors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["path"].as_str().unwrap())
        .collect();
    assert_eq!(
        paths,
        [
            "fields.customer_id",
            "fields.priority",
            "fields.total_cents"
        ]
    );
}

#[test]
fn two_flags_cannot_both_read_standard_input() {
    // Reading stdin twice used to hand the second flag an empty document, so a caller's arguments
    // were consumed as the instance and the kernel refused for a reason that was not the truth.
    let created = run(
        &[
            "create",
            "--definition",
            order_yaml().to_str().unwrap(),
            "--id",
            "ord-1",
            "--fields",
            r#"{"customer_id": "c-1", "total_cents": 10}"#,
        ],
        None,
    );
    assert_eq!(created.status.code(), Some(0));

    let output = run(
        &[
            "execute",
            "--definition",
            order_yaml().to_str().unwrap(),
            "--instance",
            "-",
            "--operation",
            "submit",
            "--arguments",
            "-",
        ],
        Some(&stdout(&created)),
    );
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("already reads standard input"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn json_escapes_that_yaml_rejects_are_read_as_json() {
    // `json.dumps` and `jq -a` emit non-BMP characters as surrogate pairs, which a YAML 1.1
    // parser refuses. Trying JSON first is what makes the promised "inline JSON" true.
    let output = run(
        &[
            "create",
            "--definition",
            order_yaml().to_str().unwrap(),
            "--id",
            "ord-1",
            "--fields",
            r#"{"customer_id": "😀", "total_cents": 5}"#,
        ],
        None,
    );
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let decision: serde_json::Value = serde_json::from_str(&stdout(&output)).expect("a decision");
    assert_eq!(
        decision["instance"]["fields"]["customer_id"].as_str(),
        Some("\u{1f600}")
    );
}

#[test]
fn an_instance_carrying_a_state_the_definition_does_not_declare_is_refused_by_name() {
    let forged = scratch(
        "forged.json",
        r#"{"entity":"order","version":1,"id":"ghost","lifecycle_state":"limbo","revision":9,
            "fields":{"customer_id":"c","total_cents":1,"priority":"normal","tags":[]}}"#,
    );
    let output = run(
        &[
            "execute",
            "--definition",
            order_yaml().to_str().unwrap(),
            "--instance",
            &format!("@{}", forged.display()),
            "--operation",
            "submit",
            "--arguments",
            r#"{"actor":"a"}"#,
        ],
        None,
    );
    assert_eq!(output.status.code(), Some(1));
    let refusal = refusal_of(&output);
    assert_eq!(refusal["kind"], "unknown_state");
    assert_eq!(refusal["state"], "limbo");
}

#[test]
fn two_definitions_of_the_same_type_and_version_are_refused() {
    let copy = scratch(
        "order-copy.yaml",
        &fs::read_to_string(order_yaml()).expect("read the example"),
    );
    let output = run(
        &[
            "execute",
            "--definition",
            order_yaml().to_str().unwrap(),
            "--definition",
            copy.to_str().unwrap(),
            "--instance",
            r#"{"entity":"order","version":1,"id":"o","lifecycle_state":"draft","revision":1,"fields":{"customer_id":"c","total_cents":1,"priority":"normal","tags":[]}}"#,
            "--operation",
            "submit",
            "--arguments",
            r#"{"actor":"a"}"#,
        ],
        None,
    );
    assert_eq!(output.status.code(), Some(1));
    let refusal = refusal_of(&output);
    assert_eq!(refusal["kind"], "definition");
    assert_eq!(refusal["defect"], "duplicate_definition");
}

/// A store carries the instance between commands, so `execute` needs no `--instance`.
///
/// This is the seam R-80 describes, end to end: the kernel decided, the shell kept it, and the next
/// command found it. Before a store existed the caller had to catch the `Decision` and hand it back
/// on the next command line, which works and is not something anybody would build a workflow on.
#[test]
fn a_store_carries_the_instance_from_create_to_execute() {
    let root = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("cli-store");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("scratch root");
    let definition = root.join("ticket.yaml");
    std::fs::write(
        &definition,
        "entity: ticket\nversion: 1\nschema:\n  fields:\n    title: { type: string, required: true }\nlifecycle:\n  initial: open\n  states: [open, closed]\noperations:\n  close:\n    transitions:\n      - from: open\n        to: closed\n    emits:\n      - type: TicketClosed\n        payload: { ticket: $id }\n",
    )
    .expect("a definition");
    let store = root.join("store");

    let created = entity()
        .args(["create", "--definition"])
        .arg(&definition)
        .args([
            "--id",
            "one",
            "--fields",
            r#"{"title":"A ticket"}"#,
            "--store",
        ])
        .arg(&store)
        .output()
        .expect("runs");
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );

    // No `--instance`: the store is where it comes from.
    let closed = entity()
        .args(["execute", "--definition"])
        .arg(&definition)
        .args(["--store"])
        .arg(&store)
        .args(["--id", "one", "--operation", "close"])
        .output()
        .expect("runs");
    assert!(
        closed.status.success(),
        "{}",
        String::from_utf8_lossy(&closed.stderr)
    );

    let decision: serde_json::Value =
        serde_json::from_slice(&closed.stdout).expect("a decision on stdout");
    assert_eq!(decision["instance"]["lifecycle_state"], "closed");
    assert_eq!(decision["instance"]["revision"], 2);
    assert_eq!(decision["events"].as_array().expect("events").len(), 1);

    // And it landed: state and events are both on disk.
    assert!(store.join("ticket/one.json").is_file());
    assert!(store.join("ticket/one.events.jsonl").is_file());
}

/// Creating twice under one identity is the store's refusal, not the kernel's, and says so.
#[test]
fn a_second_creation_of_one_identity_is_refused_by_the_store() {
    let root = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("cli-store-twice");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("scratch root");
    let definition = root.join("ticket.yaml");
    std::fs::write(
        &definition,
        "entity: ticket\nversion: 1\nschema:\n  fields:\n    title: { type: string, required: true }\nlifecycle:\n  initial: open\n  states: [open, closed]\noperations: {}\n",
    )
    .expect("a definition");
    let store = root.join("store");

    let create = || {
        entity()
            .args(["create", "--definition"])
            .arg(&definition)
            .args([
                "--id",
                "one",
                "--fields",
                r#"{"title":"A ticket"}"#,
                "--store",
            ])
            .arg(&store)
            .output()
            .expect("runs")
    };

    assert!(create().status.success());
    let again = create();
    assert_eq!(
        again.status.code(),
        Some(1),
        "a store refusal exits 1, beside the kernel's"
    );

    let refusal: serde_json::Value =
        serde_json::from_slice(&again.stdout).expect("the refusal is JSON");
    assert_eq!(refusal["by"], "store", "it says which side said no");
    assert!(
        refusal["detail"]
            .as_str()
            .expect("a detail")
            .contains("expected absent, found revision 1"),
        "the refusal says what was expected and what was found: {refusal}"
    );
}

#[test]
fn list_says_what_a_store_holds_and_nothing_for_a_type_nobody_stored() {
    let root = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("cli-list");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("scratch root");
    let definition = root.join("ticket.yaml");
    std::fs::write(
        &definition,
        "entity: ticket\nversion: 1\nschema:\n  fields:\n    title: { type: string, required: true }\nlifecycle:\n  initial: open\n  states: [open, closed]\noperations:\n  close:\n    transitions:\n      - from: open\n        to: closed\n",
    )
    .expect("a definition");
    let store = root.join("store");

    // Created in the order a sort would not produce.
    for id in ["two", "one"] {
        let created = entity()
            .args(["create", "--definition"])
            .arg(&definition)
            .args(["--id", id, "--fields", r#"{"title":"A ticket"}"#, "--store"])
            .arg(&store)
            .output()
            .expect("runs");
        assert!(
            created.status.success(),
            "{}",
            String::from_utf8_lossy(&created.stderr)
        );
    }

    let listed = entity()
        .args(["list", "--store"])
        .arg(&store)
        .args(["--entity", "ticket"])
        .output()
        .expect("runs");
    assert!(
        listed.status.success(),
        "{}",
        String::from_utf8_lossy(&listed.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&listed.stdout),
        "one\ntwo\n",
        "one per line, sorted"
    );

    let json = entity()
        .args(["list", "--store"])
        .arg(&store)
        .args(["--entity", "ticket", "--format", "json"])
        .output()
        .expect("runs");
    let parsed: serde_json::Value = serde_json::from_slice(&json.stdout).expect("JSON on stdout");
    assert_eq!(parsed, serde_json::json!(["one", "two"]));

    let nobody = entity()
        .args(["list", "--store"])
        .arg(&store)
        .args(["--entity", "nobody"])
        .output()
        .expect("runs");
    assert!(
        nobody.status.success(),
        "a type nobody stored under is an answer, not a failure: {}",
        String::from_utf8_lossy(&nobody.stderr)
    );
    assert!(nobody.stdout.is_empty(), "and the answer is nothing");
}

#[test]
fn implement_records_the_evidence_it_was_decided_on() {
    // The adopter's own ladder: `implement` costs a test result, and the printed event says which
    // count it was decided on — so *what made this done* is in the log, not only in the shell.
    let story = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/aep/story.yaml");
    let story = story.to_str().expect("a printable path");
    let mut instance = stdout(&run(
        &[
            "create",
            "--definition",
            story,
            "--id",
            "s-1",
            "--fields",
            r#"{"title": "One"}"#,
        ],
        None,
    ));
    for operation in ["propose", "activate"] {
        let moved = run(
            &[
                "execute",
                "--definition",
                story,
                "--instance",
                "-",
                "--operation",
                operation,
                "--arguments",
                r#"{"actor": "timo"}"#,
            ],
            Some(&instance),
        );
        assert_eq!(moved.status.code(), Some(0), "{}", stderr(&moved));
        instance = stdout(&moved);
    }
    let implemented = run(
        &[
            "execute",
            "--definition",
            story,
            "--instance",
            "-",
            "--operation",
            "implement",
            "--arguments",
            r#"{"actor": "timo", "evidence": {"test_result": 1}}"#,
        ],
        Some(&instance),
    );
    assert_eq!(
        implemented.status.code(),
        Some(0),
        "{}",
        stderr(&implemented)
    );
    let decision: serde_json::Value =
        serde_json::from_str(&stdout(&implemented)).expect("a JSON decision");
    assert_eq!(decision["instance"]["lifecycle_state"], "implemented");
    assert_eq!(decision["events"][0]["args"]["evidence"]["test_result"], 1);
    assert_eq!(decision["events"][0]["args"]["actor"], "timo");
}
