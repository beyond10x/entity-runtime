//! Scalar observations with explicit scale authority; no legacy operator changes.

use std::{cmp::Ordering, collections::BTreeMap};

use serde_json::Value;

use crate::ScalarCompareOp;

pub(crate) fn truthy(value: &Value) -> Option<bool> {
    match value {
        Value::Bool(value) => Some(*value),
        // ESS's bare numeric predicate observes its finite binary64 conversion, including
        // underflow. Exact numeric comparison below deliberately has a different contract.
        Value::Number(value) => value.as_f64().filter(|n| n.is_finite()).map(|n| n != 0.0),
        Value::String(value) => Some(!value.is_empty() && value != "false"),
        _ => None,
    }
}

pub(crate) fn compare(
    left: &Value,
    right: &Value,
    op: ScalarCompareOp,
    scales: &BTreeMap<String, Vec<String>>,
) -> Option<bool> {
    let scalar =
        |value: &Value| matches!(value, Value::Bool(_) | Value::Number(_) | Value::String(_));
    if !scalar(left) || !scalar(right) {
        return None;
    }
    if matches!(op, ScalarCompareOp::Eq | ScalarCompareOp::Ne) {
        let equal = match (left, right) {
            (Value::Number(left), Value::Number(right)) => {
                crate::number::compare(left, right).is_eq()
            }
            _ => left == right,
        };
        return Some(if op == ScalarCompareOp::Eq {
            equal
        } else {
            !equal
        });
    }
    let order = match (left, right) {
        (Value::Number(left), Value::Number(right)) => crate::number::compare(left, right),
        (Value::String(left), Value::String(right)) => scale_order(left, right, scales)?,
        _ => return None,
    };
    Some(match op {
        ScalarCompareOp::Lt => order.is_lt(),
        ScalarCompareOp::Le => !order.is_gt(),
        ScalarCompareOp::Gt => order.is_gt(),
        ScalarCompareOp::Ge => !order.is_lt(),
        ScalarCompareOp::Eq | ScalarCompareOp::Ne => unreachable!("equality handled above"),
    })
}

fn scale_order(
    left: &str,
    right: &str,
    scales: &BTreeMap<String, Vec<String>>,
) -> Option<Ordering> {
    let mut observed = None;
    for values in scales.values() {
        let (Some(left), Some(right)) = (
            values.iter().position(|value| value == left),
            values.iter().position(|value| value == right),
        ) else {
            continue;
        };
        let order = left.cmp(&right);
        if observed.is_some_and(|previous| previous != order) {
            return None;
        }
        observed = Some(order);
    }
    observed
}
