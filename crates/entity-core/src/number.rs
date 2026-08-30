//! Exact comparison for JSON numbers without routing integers through `f64`.

use serde_json::Number;
use std::cmp::Ordering;

/// Compares two finite JSON numbers by their decimal value.
pub(crate) fn compare(left: &Number, right: &Number) -> Ordering {
    let left_text = left.to_string();
    let right_text = right.to_string();
    match (Decimal::parse(&left_text), Decimal::parse(&right_text)) {
        (Some(left), Some(right)) => left.compare(&right),
        _ => left_text.cmp(&right_text),
    }
}

#[derive(Debug)]
struct Decimal {
    negative: bool,
    digits: String,
    scale: i64,
}

impl Decimal {
    fn parse(text: &str) -> Option<Self> {
        let (negative, unsigned) = text
            .strip_prefix('-')
            .map_or((false, text), |unsigned| (true, unsigned));
        let (mantissa, exponent) = match unsigned.split_once(['e', 'E']) {
            Some((mantissa, exponent)) => (mantissa, exponent.parse::<i64>().ok()?),
            None => (unsigned, 0),
        };
        let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
        if whole.is_empty()
            || !whole.bytes().all(|byte| byte.is_ascii_digit())
            || !fraction.bytes().all(|byte| byte.is_ascii_digit())
        {
            return None;
        }
        let mut digits = format!("{whole}{fraction}");
        let first = digits.find(|digit| digit != '0').unwrap_or(digits.len());
        digits.drain(..first);
        if digits.is_empty() {
            return Some(Self {
                negative: false,
                digits: "0".to_owned(),
                scale: 0,
            });
        }
        let mut scale = exponent.checked_sub(i64::try_from(fraction.len()).ok()?)?;
        while digits.ends_with('0') {
            digits.pop();
            scale = scale.checked_add(1)?;
        }
        Some(Self {
            negative,
            digits,
            scale,
        })
    }

    fn compare(&self, other: &Self) -> Ordering {
        if self.negative != other.negative {
            return if self.negative {
                Ordering::Less
            } else {
                Ordering::Greater
            };
        }
        let magnitude = self.compare_magnitude(other);
        if self.negative {
            magnitude.reverse()
        } else {
            magnitude
        }
    }

    fn compare_magnitude(&self, other: &Self) -> Ordering {
        let left_places = i64::try_from(self.digits.len())
            .unwrap_or(i64::MAX)
            .saturating_add(self.scale);
        let right_places = i64::try_from(other.digits.len())
            .unwrap_or(i64::MAX)
            .saturating_add(other.scale);
        match left_places.cmp(&right_places) {
            Ordering::Equal => {
                let width = self.digits.len().max(other.digits.len());
                self.digits
                    .bytes()
                    .chain(std::iter::repeat(b'0'))
                    .take(width)
                    .cmp(
                        other
                            .digits
                            .bytes()
                            .chain(std::iter::repeat(b'0'))
                            .take(width),
                    )
            }
            order => order,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::compare;
    use serde_json::Number;
    use std::cmp::Ordering;
    use std::str::FromStr;

    #[test]
    fn integers_and_decimals_compare_without_losing_precision() {
        let number = |value| Number::from_str(value).expect("number");
        assert_eq!(compare(&number("100"), &number("100.0")), Ordering::Equal);
        assert_eq!(
            compare(&number("9007199254740993"), &number("9007199254740992")),
            Ordering::Greater
        );
        assert_eq!(compare(&number("-1.20"), &number("-1.19")), Ordering::Less);
        assert_eq!(
            compare(&number("1e3"), &number("999.999")),
            Ordering::Greater
        );
    }
}
