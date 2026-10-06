//! Adversarial cases against the `x-alphabet` projection (R-162): every place a `string` schema is
//! projected — array items, map values, nested properties, union variants, operation arguments
//! and an event payload read from an argument — and the YAML documents carrying it as text.

use entity_core::EntityDefinition;
use entity_surface::{asyncapi, documentation, openapi};
use serde_json::{json, Value};

fn keypad() -> EntityDefinition {
    serde_json::from_value(json!({
        "entity": "keypad",
        "version": 1,
        "semantics": "service/1",
        "schema": { "fields": {
            "keys": { "type": "array", "items": { "type": "string", "alphabet": "0123456789" } },
            "named": { "type": "map", "key": "string",
                       "items": { "type": "string", "alphabet": "01" } },
            "dial": { "type": "object", "properties": {
                "prefix": { "type": "string", "alphabet": "+0123456789" }
            }},
            "signal": { "type": "union", "tag": "kind", "variants": {
                "tone": { "type": "string", "required": true, "alphabet": "*#" }
            }},
            "on": { "type": "string", "alphabet": "on" },
            "tilde": { "type": "string", "alphabet": "~" },
            "hex": { "type": "string", "alphabet": "0x1F" },
            "float": { "type": "string", "alphabet": "1e5" }
        }},
        "lifecycle": { "initial": "held", "states": ["held"] },
        "operations": { "Dial": {
            "transitions": [{ "from": "held", "to": "held" }],
            "arguments": { "fields": {
                "sequence": { "type": "string", "required": true, "alphabet": "0123456789*#" }
            }},
            "emits": [{ "type": "Dialled", "payload": { "sequence": "$args.sequence" } }]
        }}
    }))
    .expect("the fixture is a definition document")
}

#[test]
fn x_alphabet_reaches_every_place_a_string_schema_is_projected() {
    let definition = keypad();
    let api = openapi(std::slice::from_ref(&definition));
    let fields = &api["components"]["schemas"]["KeypadV1Fields"]["properties"];
    assert_eq!(fields["keys"]["items"]["x-alphabet"], "0123456789");
    assert_eq!(fields["named"]["additionalProperties"]["x-alphabet"], "01");
    assert_eq!(
        fields["dial"]["properties"]["prefix"]["x-alphabet"],
        "+0123456789"
    );
    assert_eq!(
        fields["signal"]["oneOf"][0]["properties"]["value"]["x-alphabet"],
        "*#"
    );
    let arguments = &api["paths"]["/entities/keypad/versions/1/{id}/operations/Dial"]["post"]
        ["requestBody"]["content"]["application/json"]["schema"]["properties"]["arguments"];
    assert_eq!(
        arguments["properties"]["sequence"]["x-alphabet"], "0123456789*#",
        "{arguments}"
    );

    let events = asyncapi(std::slice::from_ref(&definition));
    let payload =
        &events["components"]["messages"]["keypad_Dialled"]["payload"]["properties"]["payload"];
    assert_eq!(
        payload["properties"]["sequence"]["x-alphabet"], "0123456789*#",
        "{payload}"
    );
}

/// An alphabet is arbitrary text, and some texts are another scalar when YAML reads them back.
#[test]
fn the_yaml_contracts_carry_every_alphabet_as_the_text_it_is() {
    let definition = keypad();
    let bundle = documentation(std::slice::from_ref(&definition)).expect("bundle");
    let reread: Value =
        serde_yaml_ng::from_str(&bundle["openapi.yaml"]).expect("the YAML contract parses");
    let fields = &reread["components"]["schemas"]["KeypadV1Fields"]["properties"];
    for (field, alphabet) in [
        ("on", "on"),
        ("tilde", "~"),
        ("hex", "0x1F"),
        ("float", "1e5"),
    ] {
        assert_eq!(
            fields[field]["x-alphabet"],
            Value::String(alphabet.to_owned()),
            "{field}"
        );
    }
}
