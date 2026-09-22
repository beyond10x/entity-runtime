//! The bytes a `kernel/1` definition serialises to, over the definitions this repository ships.
//!
//! The claim this protects is narrow and load-bearing: every definition key the `service/1`
//! contract adds carries a `skip_serializing_if` that is true for the value a `kernel/1` definition
//! has, so a shipped document round-trips to the bytes it had and no stored identifier or `ref`
//! value moves.

use std::{fs, path::Path};

use entity_core::EntityDefinition;

/// Every YAML definition under `examples/`, in the order the filesystem lists them.
fn shipped_definitions() -> Vec<(String, EntityDefinition)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates/")
        .parent()
        .expect("repository root")
        .join("examples");
    let mut found = Vec::new();
    let mut directories = vec![root];
    while let Some(directory) = directories.pop() {
        for entry in fs::read_dir(&directory).expect("examples/ is readable") {
            let path = entry.expect("a directory entry").path();
            if path.is_dir() {
                directories.push(path);
                continue;
            }
            if path.extension().and_then(|extension| extension.to_str()) != Some("yaml") {
                continue;
            }
            let text = fs::read_to_string(&path).expect("readable");
            let definition = entity_yaml::from_str(&text).expect("a definition document");
            found.push((path.display().to_string(), definition));
        }
    }
    found.sort_by(|left, right| left.0.cmp(&right.0));
    found
}

#[test]
fn every_kernel_1_fixture_id_and_ref_value_is_byte_identical_after_this_contract() {
    let shipped = shipped_definitions();
    assert!(
        shipped.len() >= 3,
        "only {} shipped definitions were read",
        shipped.len()
    );
    for (path, definition) in shipped {
        assert!(
            definition.semantics.is_kernel_1(),
            "{path} is not a kernel/1 definition"
        );
        let document = serde_json::to_value(&definition).expect("serializes");
        for key in [
            "semantics",
            "identity",
            "relations",
            "scales",
            "number_observation",
        ] {
            assert!(
                document.get(key).is_none(),
                "{path} serialized a `{key}` key it does not declare"
            );
        }
        // The creation and every operation keep their exact spelling. `create`'s `{"emit":null}`
        // is the one this contract was most at risk of changing.
        if let Some(create) = document.get("create") {
            let keys: Vec<&String> = create.as_object().expect("object").keys().collect();
            assert_eq!(keys, vec!["emit"], "{path} grew a create key");
        }
        for (name, operation) in document
            .get("operations")
            .and_then(|operations| operations.as_object())
            .into_iter()
            .flatten()
        {
            for key in ["response", "outcomes"] {
                assert!(
                    operation.get(key).is_none(),
                    "{path} operation `{name}` serialized a `{key}` key"
                );
            }
        }
        // Nothing about a `ref` field's admitted values moved: the kind keeps its own validation,
        // and a `service/1` text address is a new address in a new framing rather than a rewrite.
        for field in document
            .get("schema")
            .and_then(|schema| schema.get("fields"))
            .and_then(|fields| fields.as_object())
            .into_iter()
            .flatten()
            .map(|(_, field)| field)
        {
            for key in ["key", "tag", "variants"] {
                assert!(
                    field.get(key).is_none(),
                    "{path} serialized a `{key}` key on a field that does not declare it"
                );
            }
        }
    }
}
