//! The storage address a logical identity is stored at.
//!
//! A `service/1` definition holds **two** identity notions and this module keeps them apart:
//!
//! * the **logical identity** is an ordinary declared field of the schema, in its own kind —
//!   `{invoice_id: {type: string}}`, `{seq: {type: integer}}`, `{pair: {type: object, …}}`. It is
//!   what a guard reads, what a view projects and what a relation carrier is typed as.
//! * the **storage address** is [`EntityInstance::id`](crate::EntityInstance): the text a store
//!   keys by. `identity: { field: <name> }` declares which schema field it is derived from.
//!
//! [`address`] is that derivation, and it is the **only** one. A binding calls this function rather
//! than spelling a prefix itself, so the text rule, the numeric canonical spelling and the
//! composite recursion have exactly one implementation and a binding cannot drift from the kernel's
//! own mirror check. Deriving an address is all a binding does with it: the address is `$id`, the
//! storage coordinate, and it is never published as the entity's identifier.
//!
//! # Total, per kind, and collision-free
//!
//! | kind | address |
//! |---|---|
//! | `string`, `enum`, `ref` | `s:` followed by the string's own contents, unquoted and unescaped |
//! | `integer`, `number`, `binary64` | the source-observed canonical text — one function, not three |
//! | `boolean` | `false` or `true` |
//! | `array`, `object`, `map`, `union` | canonical JSON, recursively |
//! | `json` | refused: it types nothing, so no injection over it exists |
//!
//! The `s:` prefix is what makes the text rule **total**: the source admits an empty and a
//! whitespace `String` identity, a storage address may be neither, and `address("")` is `"s:"`.
//! It is injective on the whole `String` domain — `address("s:x")` is `"s:s:x"` — and it separates
//! the rows from one another as well, though two rows can never meet because a kind is fixed per
//! entity type.
//!
//! No `kernel/1` identifier, `ref` value or record byte moves, because a `kernel/1` definition
//! cannot declare `identity` at all.

use serde_json::Value;

use crate::definition::FieldKind;
use crate::observed::Observed;

/// Why a value has no address.
///
/// Every case a registered definition admits is addressable, so each of these names an input that
/// registration already refuses — which is what makes the public function safe to hand an
/// unvalidated value: it answers with a name rather than inventing an address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddressError {
    /// The identity field's kind has no address function. Only `json` is in this position.
    KindNotAddressable {
        /// The kind that was declared.
        kind: &'static str,
    },
    /// The value is not one of the declared kind.
    ValueNotOfKind {
        /// The kind that was declared.
        kind: &'static str,
        /// What arrived instead.
        found: &'static str,
    },
    /// A number outside the source's observation domain, which no admitted field value is.
    NumberOutsideSourceDomain {
        /// The token that could not be observed.
        token: String,
    },
    /// A `null` inside a composite identity. Per-kind validation refuses one, and `json` is not an
    /// admitted identity kind, so there is no hole where it could be admitted as a value.
    NullInComposite,
}

impl std::fmt::Display for AddressError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::KindNotAddressable { kind } => write!(
                f,
                "a {kind} identity has no address: it types nothing, so no address function over \
                 it could be injective"
            ),
            Self::ValueNotOfKind { kind, found } => {
                write!(f, "a {kind} identity cannot be addressed from {found}")
            }
            Self::NumberOutsideSourceDomain { token } => write!(
                f,
                "the number {token} is outside the source observation domain, so it has no \
                 canonical address"
            ),
            Self::NullInComposite => {
                write!(f, "a composite identity cannot carry a null member")
            }
        }
    }
}

impl std::error::Error for AddressError {}

/// The storage address of one logical identity value, by the declared kind of its field.
///
/// # Errors
///
/// [`AddressError`], for each of the inputs a registered `service/1` definition cannot produce:
/// a `json` identity field, a value of the wrong shape, a number the source cannot observe, or a
/// `null` inside a composite.
pub fn address(kind: FieldKind, value: &Value) -> Result<String, AddressError> {
    let wrong = |found: &'static str| AddressError::ValueNotOfKind {
        kind: kind.as_str(),
        found,
    };
    match kind {
        // `s:` and everything after it, unquoted and unescaped. The prefix is what makes the rule
        // total over a `String` domain that includes the empty and whitespace values the source
        // admits, and what keeps a text address from colliding with a boolean or numeric one.
        FieldKind::String | FieldKind::Enum | FieldKind::Ref => match value.as_str() {
            Some(text) => Ok(format!("s:{text}")),
            None => Err(wrong(describe(value))),
        },
        FieldKind::Integer | FieldKind::Number | FieldKind::Binary64 => match value.as_number() {
            Some(number) => numeric_text(number),
            None => Err(wrong(describe(value))),
        },
        FieldKind::Boolean => match value.as_bool() {
            Some(true) => Ok("true".to_owned()),
            Some(false) => Ok("false".to_owned()),
            None => Err(wrong(describe(value))),
        },
        FieldKind::Array => match value {
            Value::Array(_) => canonical_json(value),
            other => Err(wrong(describe(other))),
        },
        FieldKind::Object | FieldKind::Map | FieldKind::Union => match value {
            Value::Object(_) => canonical_json(value),
            other => Err(wrong(describe(other))),
        },
        FieldKind::Json => Err(AddressError::KindNotAddressable {
            kind: kind.as_str(),
        }),
    }
}

/// One numeric row for three kinds: the source-observed canonical text.
///
/// `1`, `1.0` and `1e0` are one *value* and so one address; `-0.0` and `0.0` are one value and one
/// address, while the field's own bytes keep the sign.
fn numeric_text(number: &serde_json::Number) -> Result<String, AddressError> {
    Observed::of_number(number)
        .map(Observed::exact_text)
        .ok_or_else(|| AddressError::NumberOutsideSourceDomain {
            token: number.to_string(),
        })
}

/// Canonical JSON, at every depth: objects and maps by sorted key, arrays in index order, string
/// leaves JSON-quoted — which is where the `s:` prefix does **not** appear, because a composite's
/// injectivity comes from the quoting instead — numeric leaves by the source-observed canonical
/// text, and booleans as `false`/`true`.
fn canonical_json(value: &Value) -> Result<String, AddressError> {
    let mut text = String::new();
    write_canonical(value, &mut text)?;
    Ok(text)
}

fn write_canonical(value: &Value, out: &mut String) -> Result<(), AddressError> {
    match value {
        Value::Null => Err(AddressError::NullInComposite),
        Value::Bool(true) => {
            out.push_str("true");
            Ok(())
        }
        Value::Bool(false) => {
            out.push_str("false");
            Ok(())
        }
        Value::Number(number) => {
            out.push_str(&numeric_text(number)?);
            Ok(())
        }
        Value::String(text) => {
            out.push_str(&quoted(text));
            Ok(())
        }
        Value::Array(values) => {
            out.push('[');
            for (index, value) in values.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_canonical(value, out)?;
            }
            out.push(']');
            Ok(())
        }
        Value::Object(members) => {
            // `serde_json::Map` without `preserve_order` is already sorted by key, which is the
            // order this runtime canonicalizes an object into everywhere else.
            let mut keys: Vec<&String> = members.keys().collect();
            keys.sort_unstable();
            out.push('{');
            for (index, key) in keys.into_iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(&quoted(key));
                out.push(':');
                write_canonical(&members[key], out)?;
            }
            out.push('}');
            Ok(())
        }
    }
}

/// A JSON string with the standard escaping, which is what makes the composite rule injective over
/// text leaves without a prefix.
fn quoted(text: &str) -> String {
    Value::String(text.to_owned()).to_string()
}

fn describe(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "a list",
        Value::Object(_) => "a mapping",
    }
}

#[cfg(test)]
mod tests {
    use super::{address, AddressError};
    use crate::definition::FieldKind;
    use serde_json::json;

    /// The row that makes the rule total: an empty and a whitespace identity are values the source
    /// admits, and a storage address may be neither empty nor whitespace.
    #[test]
    fn a_text_address_is_never_empty_or_whitespace_however_empty_its_identity_is() {
        assert_eq!(address(FieldKind::String, &json!("")).unwrap(), "s:");
        assert_eq!(address(FieldKind::String, &json!(" ")).unwrap(), "s: ");
        assert_eq!(address(FieldKind::String, &json!("s:x")).unwrap(), "s:s:x");
        assert_eq!(
            address(FieldKind::String, &json!("INV-1")).unwrap(),
            "s:INV-1"
        );
    }

    #[test]
    fn the_three_numeric_kinds_share_one_canonical_spelling() {
        for kind in [FieldKind::Integer, FieldKind::Number, FieldKind::Binary64] {
            assert_eq!(address(kind, &json!(1)).unwrap(), "1");
        }
        for text in ["1", "1.0", "1e0"] {
            let value: serde_json::Value = serde_json::from_str(text).unwrap();
            assert_eq!(address(FieldKind::Number, &value).unwrap(), "1");
        }
        let negative_zero: serde_json::Value = serde_json::from_str("-0.0").unwrap();
        assert_eq!(address(FieldKind::Binary64, &negative_zero).unwrap(), "0");
        assert_eq!(address(FieldKind::Binary64, &json!(0)).unwrap(), "0");
    }

    #[test]
    fn a_composite_address_sorts_keys_quotes_text_and_normalizes_numbers_at_every_depth() {
        let value: serde_json::Value =
            serde_json::from_str(r#"{"b":1.0,"a":["x",{"d":true,"c":-0.0}]}"#).unwrap();
        assert_eq!(
            address(FieldKind::Object, &value).unwrap(),
            r#"{"a":["x",{"c":0,"d":true}],"b":1}"#
        );
    }

    #[test]
    fn a_json_identity_has_no_address_and_says_so_rather_than_inventing_one() {
        assert_eq!(
            address(FieldKind::Json, &json!("x")),
            Err(AddressError::KindNotAddressable { kind: "json" })
        );
    }

    #[test]
    fn an_unvalidated_value_receives_a_named_error_rather_than_an_invented_address() {
        assert_eq!(
            address(FieldKind::Integer, &json!("x")),
            Err(AddressError::ValueNotOfKind {
                kind: "integer",
                found: "a string"
            })
        );
        // The token named is the one the kernel **received**, not the lexeme the caller wrote:
        // this decoder already normalised `1e400` to `1e+400` before the kernel saw it, and no
        // recovery of a pre-decoding lexeme is promised.
        let huge: serde_json::Value = serde_json::from_str("1e400").unwrap();
        assert_eq!(
            address(FieldKind::Number, &huge),
            Err(AddressError::NumberOutsideSourceDomain {
                token: huge.to_string()
            })
        );
        assert_eq!(
            address(FieldKind::Object, &json!({"a": null})),
            Err(AddressError::NullInComposite)
        );
    }
}
