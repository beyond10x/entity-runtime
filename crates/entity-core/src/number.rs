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

/// The most decimal places an exact sum's two operands may span, the units place included.
///
/// An exact sum aligns its operands, which materializes every place between the highest and the
/// lowest either occupies, and an exponent lets a six-byte token such as `1e2000` name a
/// two-thousand-digit number. The bound keeps that work proportional to something a definition
/// author can see. It admits the sum of any two finite binary64 values written in at most
/// seventeen significant digits, which span at most 309 integer and 340 fractional places.
pub(crate) const MAX_SUM_PLACES: i64 = 1024;

/// The exact sum of two JSON numbers, spelled positionally with no exponent, no trailing
/// fractional zero and no negative zero: `1.50 + 1` is `2.5`, `1E+2 + 1` is `101`.
///
/// `None` when an exponent does not fit an `i64` or the operands span more than
/// [`MAX_SUM_PLACES`] places, even where they would cancel; the caller refuses by name. Nothing
/// passes through `f64`.
pub(crate) fn add(left: &Number, right: &Number) -> Option<Number> {
    let left = Exact::parse(&left.to_string())?;
    let right = Exact::parse(&right.to_string())?;
    left.plus(&right)?.spelled().parse().ok()
}

/// `digits × 10^exponent`, with no leading or trailing zero digit; zero has no digits.
struct Exact {
    negative: bool,
    /// Most significant first.
    digits: Vec<u8>,
    exponent: i64,
}

impl Exact {
    fn parse(text: &str) -> Option<Self> {
        let negative = text.starts_with('-');
        let unsigned = text.strip_prefix('-').unwrap_or(text);
        let (mantissa, exponent) = unsigned.split_once(['e', 'E']).unwrap_or((unsigned, "0"));
        let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
        let exponent = exponent
            .parse::<i64>()
            .ok()?
            .checked_sub(i64::try_from(fraction.len()).ok()?)?;
        let digits = whole
            .bytes()
            .chain(fraction.bytes())
            .map(|byte| byte.wrapping_sub(b'0'))
            .collect::<Vec<_>>();
        if digits.iter().any(|digit| *digit > 9) {
            return None;
        }
        Self::normalized(negative, digits, exponent)
    }

    /// Strips leading and trailing zero digits, moving the trailing ones into the exponent.
    fn normalized(negative: bool, mut digits: Vec<u8>, exponent: i64) -> Option<Self> {
        let leading = digits.iter().take_while(|digit| **digit == 0).count();
        digits.drain(..leading);
        let trailing = digits.iter().rev().take_while(|digit| **digit == 0).count();
        digits.truncate(digits.len() - trailing);
        if digits.is_empty() {
            return Some(Self {
                negative: false,
                digits,
                exponent: 0,
            });
        }
        Some(Self {
            negative,
            exponent: exponent.checked_add(i64::try_from(trailing).ok()?)?,
            digits,
        })
    }

    /// The highest place a digit occupies; the units place is `0`.
    fn high(&self) -> Option<i64> {
        self.exponent
            .checked_add(i64::try_from(self.digits.len()).ok()?)?
            .checked_sub(1)
    }

    /// The digit at `place`, zero outside the digits.
    fn at(&self, place: i64) -> u8 {
        let Some(from_low) = place.checked_sub(self.exponent) else {
            return 0;
        };
        usize::try_from(from_low)
            .ok()
            .and_then(|from_low| {
                self.digits
                    .len()
                    .checked_sub(1 + from_low)
                    .map(|index| self.digits[index])
            })
            .unwrap_or(0)
    }

    fn plus(&self, other: &Self) -> Option<Self> {
        let operands = [self, other];
        let mut high = 0;
        let mut low = 0;
        for operand in operands.iter().filter(|operand| !operand.digits.is_empty()) {
            high = high.max(operand.high()?);
            low = low.min(operand.exponent);
        }
        if high.checked_sub(low)?.checked_add(1)? > MAX_SUM_PLACES {
            return None;
        }
        // Least significant first, one entry per place from `low` to `high`.
        let magnitude =
            |operand: &Self| -> Vec<u8> { (low..=high).map(|place| operand.at(place)).collect() };
        let (left, right) = (magnitude(self), magnitude(other));
        let (negative, mut sum) = if self.negative == other.negative {
            (self.negative, add_magnitudes(&left, &right))
        } else if left.iter().rev().cmp(right.iter().rev()).is_lt() {
            (other.negative, subtract_magnitudes(&right, &left))
        } else {
            (self.negative, subtract_magnitudes(&left, &right))
        };
        sum.reverse();
        Self::normalized(negative, sum, low)
    }

    fn spelled(&self) -> String {
        if self.digits.is_empty() {
            return "0".to_owned();
        }
        let digits: String = self
            .digits
            .iter()
            .map(|digit| char::from(b'0' + digit))
            .collect();
        let sign = if self.negative { "-" } else { "" };
        // `plus` bounded the places, so every count below is small.
        let length = i64::try_from(digits.len()).unwrap_or(i64::MAX);
        let zeros = |count: i64| "0".repeat(usize::try_from(count).unwrap_or(0));
        if self.exponent >= 0 {
            format!("{sign}{digits}{}", zeros(self.exponent))
        } else {
            let point = length + self.exponent;
            if point > 0 {
                let (whole, fraction) = digits.split_at(usize::try_from(point).unwrap_or(0));
                format!("{sign}{whole}.{fraction}")
            } else {
                format!("{sign}0.{}{digits}", zeros(-point))
            }
        }
    }
}

/// Two equal-width magnitudes, least significant digit first, added with carry.
fn add_magnitudes(left: &[u8], right: &[u8]) -> Vec<u8> {
    let mut sum = Vec::with_capacity(left.len() + 1);
    let mut carry = 0;
    for (left, right) in left.iter().zip(right) {
        let place = left + right + carry;
        sum.push(place % 10);
        carry = place / 10;
    }
    sum.push(carry);
    sum
}

/// `larger - smaller` for two equal-width magnitudes, least significant digit first.
fn subtract_magnitudes(larger: &[u8], smaller: &[u8]) -> Vec<u8> {
    let mut difference = Vec::with_capacity(larger.len());
    let mut borrow = 0;
    for (larger, smaller) in larger.iter().zip(smaller) {
        let place = i16::from(*larger) - i16::from(*smaller) - borrow;
        borrow = i16::from(place < 0);
        difference.push(u8::try_from(place + borrow * 10).unwrap_or(0));
    }
    difference
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

    fn sum(left: &str, right: &str) -> Option<String> {
        add(&left.parse().unwrap(), &right.parse().unwrap()).map(|sum| sum.to_string())
    }

    #[test]
    fn a_decimal_sum_is_exact_and_spelled_positionally_without_trailing_zeros() {
        for (left, right, expected) in [
            ("0.1", "0.2", "0.3"),
            ("1.50", "1", "2.5"),
            ("1E+2", "1", "101"),
            ("1.5e1", "0.05", "15.05"),
            ("-0.5", "0.5", "0"),
            ("-0.0", "0", "0"),
            ("0", "0", "0"),
            ("1e-5", "0", "0.00001"),
            ("-1.25", "0.25", "-1"),
            ("999", "1", "1000"),
            ("-3", "1", "-2"),
            ("1", "-3", "-2"),
            ("0.001", "-0.01", "-0.009"),
            (
                "123456789012345678901234567890",
                "1",
                "123456789012345678901234567891",
            ),
            ("9007199254740993", "0.5", "9007199254740993.5"),
        ] {
            assert_eq!(
                sum(left, right).as_deref(),
                Some(expected),
                "{left} + {right}"
            );
            assert_eq!(
                sum(right, left).as_deref(),
                Some(expected),
                "{right} + {left}"
            );
        }
    }

    #[test]
    fn a_decimal_sum_past_its_digit_bound_or_the_exponent_range_is_no_sum() {
        // The bound is on the decimal places the two operands span, the units place included, which
        // is what aligning them for an exact sum materializes; it holds even where they cancel.
        assert_eq!(sum("1e1023", "0").map(|s| s.len()), Some(1024));
        assert_eq!(sum("1e1024", "0"), None);
        assert_eq!(sum("1e-1023", "0").map(|s| s.len()), Some(1025));
        assert_eq!(sum("1e-1024", "0"), None);
        assert_eq!(sum("1e2000", "1"), None);
        assert_eq!(sum("1e2000", "-1e2000"), None);
        assert_eq!(sum("1e99999999999999999999", "1"), None);
        assert_eq!(sum("1e-99999999999999999999", "1"), None);
    }
}
