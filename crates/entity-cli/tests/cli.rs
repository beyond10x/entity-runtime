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

#[test]
fn an_unreadable_or_unparsable_file_is_a_usage_error_with_exit_two() {
    let output = run(&["validate", "does-not-exist.yaml"], None);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("cannot read does-not-exist.yaml"),
        "{}",
        stderr(&output)
    );

    let garbage = scratch("garbage.yaml", "entity: [unclosed\n");
    let output = run(&["inspect", garbage.to_str().unwrap()], None);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("invalid entity YAML"),
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
    assert!(dot.starts_with("digraph \"order\" {"), "{dot}");
    assert!(
        dot.contains("\"submitted\" -> \"approved\" [label=\"approve\"];"),
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

    // Now the JSON form via a file, and a precondition refusal: a zero-value order cannot be approved.
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
    let refusal: serde_json::Value =
        serde_json::from_str(&stdout(&refused)).expect("a JSON refusal");
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
    let refusal: serde_json::Value =
        serde_json::from_str(&stdout(&refused)).expect("a JSON refusal");
    assert_eq!(refusal["kind"], "invalid_transition");
    assert_eq!(refusal["state"], "submitted");
}

#[test]
fn create_refuses_to_guess_between_two_definitions() {
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
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("exactly one --definition"),
        "{}",
        stderr(&output)
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
    let refusal: serde_json::Value =
        serde_json::from_str(&stdout(&output)).expect("a JSON refusal");
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
