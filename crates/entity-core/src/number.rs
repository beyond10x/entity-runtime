//! Exact decimal ordering, including arbitrarily large JSON exponents.

use serde_json::Number;
use std::cmp::Ordering;

/// Compares JSON numbers by exact mathematical value, independent of their spelling.
#[must_use]
pub fn compare(left: &Number, right: &Number) -> Ordering {
    let left = Decimal::parse(&left.to_string());
    let right = Decimal::parse(&right.to_string());
    match (left.digits.is_empty(), right.digits.is_empty()) {
        (true, true) => return Ordering::Equal,
        (true, false) => {
            return if right.negative {
                Ordering::Greater
            } else {
                Ordering::Less
            }
        }
        (false, true) => {
            return if left.negative {
                Ordering::Less
            } else {
                Ordering::Greater
            }
        }
        _ => {}
    }
    if left.negative != right.negative {
        return if left.negative {
            Ordering::Less
        } else {
            Ordering::Greater
        };
    }
    let order = left.places.cmp(&right.places).then_with(|| {
        let width = left.digits.len().max(right.digits.len());
        left.digits
            .bytes()
            .chain(std::iter::repeat(b'0'))
            .take(width)
            .cmp(
                right
                    .digits
                    .bytes()
                    .chain(std::iter::repeat(b'0'))
                    .take(width),
            )
    });
    if left.negative {
        order.reverse()
    } else {
        order
    }
}

struct Decimal {
    negative: bool,
    digits: String,
    places: Integer,
}
impl Decimal {
    fn parse(text: &str) -> Self {
        let negative = text.starts_with('-');
        let unsigned = text.strip_prefix('-').unwrap_or(text);
        let (mantissa, exponent) = unsigned.split_once(['e', 'E']).unwrap_or((unsigned, "0"));
        let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
        let raw = format!("{whole}{fraction}");
        let leading = raw.bytes().take_while(|byte| *byte == b'0').count();
        let digits = raw[leading..].trim_end_matches('0').to_owned();
        let mut places = Integer::parse(exponent);
        places.add(Integer::parse(&whole.len().to_string()));
        places.add(Integer::parse(&format!("-{leading}")));
        Self {
            negative,
            digits,
            places,
        }
    }
}

/// Signed decimal arithmetic avoids both exponent overflow and a string-order fallback.
#[derive(Eq, PartialEq)]
struct Integer {
    negative: bool,
    digits: Vec<u8>,
}
impl Integer {
    fn parse(text: &str) -> Self {
        let digits = text
            .trim_start_matches(['-', '+'])
            .trim_start_matches('0')
            .bytes()
            .rev()
            .map(|byte| byte - b'0')
            .collect::<Vec<_>>();
        Self {
            negative: text.starts_with('-') && !digits.is_empty(),
            digits,
        }
    }
    fn magnitude(&self, other: &Self) -> Ordering {
        self.digits
            .len()
            .cmp(&other.digits.len())
            .then_with(|| self.digits.iter().rev().cmp(other.digits.iter().rev()))
    }
    fn add(&mut self, mut other: Self) {
        if self.negative == other.negative {
            let width = self.digits.len().max(other.digits.len());
            self.digits.resize(width, 0);
            let mut carry = 0;
            for (index, digit) in self.digits.iter_mut().enumerate() {
                let sum = *digit + other.digits.get(index).copied().unwrap_or(0) + carry;
                *digit = sum % 10;
                carry = sum / 10;
            }
            if carry != 0 {
                self.digits.push(carry);
            }
        } else {
            if self.magnitude(&other).is_lt() {
                std::mem::swap(self, &mut other);
            }
            let mut borrow = 0i16;
            for (index, digit) in self.digits.iter_mut().enumerate() {
                let value = i16::from(*digit)
                    - i16::from(other.digits.get(index).copied().unwrap_or(0))
                    - borrow;
                borrow = i16::from(value < 0);
                *digit = (value + borrow * 10) as u8;
            }
            while self.digits.last() == Some(&0) {
                self.digits.pop();
            }
            if self.digits.is_empty() {
                self.negative = false;
            }
        }
    }
}
impl Ord for Integer {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.negative.cmp(&other.negative) {
            Ordering::Equal => {
                if self.negative {
                    self.magnitude(other).reverse()
                } else {
                    self.magnitude(other)
                }
            }
            order => order.reverse(),
        }
    }
}
impl PartialOrd for Integer {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn integers_and_decimals_compare_without_losing_precision() {
        for (left, right, expected) in [
            ("100", "100.0", Ordering::Equal),
            ("9007199254740993", "9007199254740992", Ordering::Greater),
            ("-1.20", "-1.19", Ordering::Less),
            ("1e3", "999.999", Ordering::Greater),
            ("0", "0.1", Ordering::Less),
            ("-0.0", "0", Ordering::Equal),
            ("0", "-0.001", Ordering::Greater),
            ("1e9223372036854775808", "2", Ordering::Greater),
            (
                "100e9999999999999999999999999",
                "1e10000000000000000000000001",
                Ordering::Equal,
            ),
            ("1e-9999999999999999999999999", "0", Ordering::Greater),
        ] {
            let l = left.parse().unwrap();
            let r = right.parse().unwrap();
            assert_eq!(compare(&l, &r), expected, "{left} versus {right}");
            assert_eq!(compare(&r, &l), expected.reverse());
        }
    }
}
