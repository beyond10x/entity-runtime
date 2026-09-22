//! `source-number/1`: the value a `service/1` predicate reads a stored JSON number as.
//!
//! **Stored and observed are two different things, and both are exact.** A field, an event
//! payload, a response and a [`DecisionRecord`](crate::DecisionRecord) hold the authored token byte
//! for byte — `arbitrary_precision` is what keeps it — and nothing here ever writes one back.
//! What this module supplies is the *reading*: a pure function of the token, used by every
//! `service/1` predicate and by nothing else.
//!
//! Why a reading rule rather than an exact comparison of the token: the source's own two doors
//! observe a decimal binary64 cannot carry as the binary64 it collapses to. With `amount` authored
//! `1.0000000000000000001`, the source observes exactly `1` and answers `ne 1` **false**; a reader
//! comparing the token exactly answers **true** and takes an accepting branch the source does not
//! take. That divergence is not one-directional — a negated or `ne` guard reverses it — so it
//! cannot be bounded by a caveat. Reproducing the reading is what makes a lowered guard mean what
//! it meant.
//!
//! # The two doors
//!
//! They are not one door, and reading both through the wire door would erase a distinction the
//! source draws:
//!
//! * [`Observed::of_number`] — the **wire and field path**: a JSON number as the source's document
//!   reader takes it. Integer first (so `9007199254740993` and `9007199254740992` are two values
//!   with two orderings, never one `f64`), then a correctly rounded finite binary64.
//! * [`Observed::of_literal`] — the **literal path**: a number written inside a condition, as the
//!   source's predicate parser takes it. It keeps the authored decimal only when its scale is zero
//!   or it *is* the canonical decimal of the binary64 the same text parses to.
//!
//! # Why Rust's own parser, and not `serde_json`'s decoding
//!
//! The feature configurations do not read every token alike. At the locked `serde_json` a plain
//! default-feature build reads `946.3702156715110866946` as binary64 bits `408d92f633a24cda`,
//! because it uses its own significand/exponent arithmetic; the same token through
//! arbitrary-precision `Number::as_f64` reads `408d92f633a24cdb`. The actual service-producing
//! source tool resolves `float_roundtrip` and reads `…cdb`. So the fallback below parses the
//! retained token with `str::parse::<f64>`, which is correctly rounded: a Cargo feature change
//! cannot move what an already recorded definition answers.
//!
//! # The one `f64`, and what it is not
//!
//! The source's reading is binary64-mediated in two places — the fallback above and
//! `canonical_decimal`'s shortest-round-trip spelling — so reproducing the *answer* requires
//! reproducing those. Nothing the kernel writes passes through them: this module returns an
//! [`Ordering`], a `bool` and a canonical text, and never a number a field, event, response or
//! record is built from.

use std::cmp::Ordering;

use serde_json::Number;

/// The largest scale an exact decimal may carry before no exact value is read.
const MAX_SCALE: u32 = 255;

/// The value a `service/1` predicate reads a stored JSON number as.
#[derive(Debug, Clone, Copy)]
pub struct Observed(Repr);

/// `units × 10^-scale` beside the binary64 the source writes, or a magnitude no such pair spells.
#[derive(Debug, Clone, Copy)]
enum Repr {
    Exact { units: i128, scale: u8, binary: f64 },
    Binary64(f64),
}

impl Observed {
    /// The wire and field path: a JSON number as the source's document reader takes it.
    ///
    /// `None` for a token outside the source's observation domain — `1e400` is a number this
    /// runtime can hold and the source wire reader refuses. A caller receives the absence and
    /// refuses by name; nothing here panics, saturates or invents a value.
    #[must_use]
    pub fn of_number(value: &Number) -> Option<Self> {
        if let Some(units) = value.as_i64() {
            return Some(Self::exact_integer(i128::from(units)));
        }
        if let Some(units) = value.as_u64() {
            return Some(Self::exact_integer(i128::from(units)));
        }
        let binary: f64 = value.to_string().parse().ok()?;
        binary.is_finite().then(|| Self::of_binary64(binary))
    }

    /// An exact integer at scale zero, beside the binary64 the source carries it as.
    ///
    /// The carried binary64 is lossy past 2^53 and is used only by [`Observed::is_zero`] and by a
    /// comparison against a value no exact pair spells; two exact values always compare exactly.
    fn exact_integer(units: i128) -> Self {
        #[allow(clippy::cast_precision_loss)]
        let binary = units as f64;
        Self(Repr::Exact {
            units,
            scale: 0,
            binary,
        })
    }

    /// The literal path: a number written inside a condition, as the source's predicate parser
    /// takes it.
    ///
    /// `None` for text that is not a finite number, which is what refuses an unobservable literal
    /// or schema bound at registration rather than at every later evaluation.
    #[must_use]
    pub fn of_literal(text: &str) -> Option<Self> {
        let binary: f64 = text.parse().ok()?;
        if !binary.is_finite() {
            return None;
        }
        if let Some((units, scale)) = exact_of_decimal_text(text) {
            // The authored decimal survives only where the source keeps it: an exact integer, or a
            // decimal that *is* the canonical decimal of the binary64 the same text parses to.
            if scale == 0 || canonical_decimal(binary) == Some((units, scale)) {
                return Some(Self(Repr::Exact {
                    units,
                    scale,
                    binary,
                }));
            }
        }
        Some(Self::of_binary64(binary))
    }

    /// The exact value, and the binary64 only where there is no exact value.
    ///
    /// Two exactly-carried values compare by their normalised `(units, scale)`; anything else
    /// compares the carried binaries by `total_cmp`. A carried-exactly value and a binary64-carried
    /// one never compare equal, because the representation is a function of the carried binary64.
    ///
    /// Not `Ord`: `Ord` requires `Eq`, and an `Eq` on this type would invite `==` on a value whose
    /// equality *is* this ordering being `Equal` — the one thing every caller here has to go
    /// through, because two tokens that are not byte-identical are routinely one value.
    #[must_use]
    #[allow(clippy::should_implement_trait)]
    pub fn cmp(self, other: Self) -> Ordering {
        match (self.0, other.0) {
            (
                Repr::Exact {
                    units: left,
                    scale: left_scale,
                    ..
                },
                Repr::Exact {
                    units: right,
                    scale: right_scale,
                    ..
                },
            ) => exact_cmp(left, left_scale, right, right_scale),
            _ => self.binary().total_cmp(&other.binary()),
        }
    }

    /// Whether the value is zero, which is the carried binary64's own test.
    ///
    /// The **carried binary64**, not the exact value, and the difference is the underflow class: a
    /// token such as `1e-400` is exactly nonzero and its carried binary64 is `0.0`, so an exact test
    /// would answer *true* where the source answers *false*. The token is still stored unrounded.
    #[must_use]
    pub fn is_zero(self) -> bool {
        self.binary() == 0.0
    }

    /// The canonical spelling of this value: one text per value, and the text an identity addresses
    /// by.
    ///
    /// For a value carried exactly, `units × 10^-scale` with a sign, no exponent and no trailing
    /// fractional zero; for a value no `(i128, u8)` spells, the shortest decimal that round-trips
    /// its binary64.
    #[must_use]
    pub fn exact_text(self) -> String {
        match self.0 {
            Repr::Exact { units, scale, .. } => exact_text(units, scale),
            Repr::Binary64(value) => format!("{value}"),
        }
    }

    fn of_binary64(value: f64) -> Self {
        match canonical_decimal(value) {
            Some((units, scale)) => Self(Repr::Exact {
                units,
                scale,
                binary: value,
            }),
            None => Self(Repr::Binary64(value)),
        }
    }

    fn binary(self) -> f64 {
        match self.0 {
            Repr::Exact { binary, .. } | Repr::Binary64(binary) => binary,
        }
    }
}

/// The exact decimal a binary64 canonically is, or `None` where no `(i128, u8)` spells it.
///
/// An integral value inside the `i64` span is that integer at scale zero — which is what makes
/// `-0.0` and `0.0` **one value**, both `(0, 0)`, exactly as the source requires of a guard reading
/// `amount == 0`. The sign of the zero survives where the model promises it: in the bytes.
fn canonical_decimal(value: f64) -> Option<(i128, u8)> {
    const LIMIT: f64 = 9_223_372_036_854_775_808.0;
    if value.fract() == 0.0 && (-LIMIT..=LIMIT).contains(&value) {
        #[allow(clippy::cast_possible_truncation)]
        return Some((value as i128, 0));
    }
    // The shortest decimal that round-trips, read back through the authored-decimal grammar.
    exact_of_decimal_text(&format!("{value}"))
}

/// Reads an authored decimal as `units × 10^-scale`, normalised so one value has one pair.
///
/// `None` when the text is not `[+-]?digits[.digits][eE[+-]?digits]`, when the digits do not fit an
/// `i128`, or when the scale exceeds 255.
fn exact_of_decimal_text(text: &str) -> Option<(i128, u8)> {
    let (negative, rest) = match *text.as_bytes().first()? {
        b'+' => (false, &text[1..]),
        b'-' => (true, &text[1..]),
        _ => (false, text),
    };

    let (mantissa, exponent) = match rest.split_once(['e', 'E']) {
        Some((mantissa, exponent)) => (mantissa, Some(exponent)),
        None => (rest, None),
    };
    let (whole, fraction) = match mantissa.split_once('.') {
        Some((whole, fraction)) => {
            // `digits[.digits]`: a trailing point with nothing after it is not that grammar.
            if fraction.is_empty() {
                return None;
            }
            (whole, fraction)
        }
        None => (mantissa, ""),
    };
    // The fractional group is optional, and where it is absent there is nothing to check. Reading
    // an absent group as a malformed one would refuse every integer written without a point, which
    // is the whole of the scale-zero branch `of_literal` keeps an authored value on.
    if !is_digits(whole) || (!fraction.is_empty() && !is_digits(fraction)) {
        return None;
    }

    let exponent = match exponent {
        None => 0_i64,
        Some(text) => {
            let (negative, digits) = match *text.as_bytes().first()? {
                b'+' => (false, &text[1..]),
                b'-' => (true, &text[1..]),
                _ => (false, text),
            };
            if digits.is_empty() || !is_digits(digits) {
                return None;
            }
            let magnitude: i64 = digits.parse().ok()?;
            if negative {
                -magnitude
            } else {
                magnitude
            }
        }
    };

    let mut units: i128 = format!("{whole}{fraction}").parse().ok()?;
    // `fraction.len()` is a count of characters and `exponent` is bounded by the parse above, so
    // this subtraction is taken in `i64` where neither can overflow.
    let mut scale = i64::try_from(fraction.len()).ok()? - exponent;
    while scale < 0 {
        units = units.checked_mul(10)?;
        scale += 1;
    }
    // Trailing zeroes normalised away, so one value has one pair and `exact_text` is injective.
    while scale > 0 && units % 10 == 0 {
        units /= 10;
        scale -= 1;
    }
    if units == 0 {
        scale = 0;
    }
    if scale > i64::from(MAX_SCALE) {
        return None;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let scale = scale as u8;
    Some((if negative { -units } else { units }, scale))
}

fn is_digits(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit())
}

/// `units × 10^-scale`, written with a sign, no exponent and no trailing fractional zero.
fn exact_text(units: i128, scale: u8) -> String {
    if scale == 0 {
        return units.to_string();
    }
    let digits = units.unsigned_abs().to_string();
    let scale = usize::from(scale);
    let (whole, fraction) = if digits.len() > scale {
        let split = digits.len() - scale;
        (digits[..split].to_owned(), digits[split..].to_owned())
    } else {
        ("0".to_owned(), format!("{digits:0>scale$}"))
    };
    let sign = if units < 0 { "-" } else { "" };
    format!("{sign}{whole}.{fraction}")
}

/// Orders two exact decimals without ever scaling one into an overflow.
fn exact_cmp(left: i128, left_scale: u8, right: i128, right_scale: u8) -> Ordering {
    match (left.signum(), right.signum()) {
        (0, 0) => Ordering::Equal,
        (0, right) => 0.cmp(&right),
        (left, 0) => left.cmp(&0),
        (left, right) if left != right => left.cmp(&right),
        (sign, _) => {
            let order = magnitude_cmp(
                &left.unsigned_abs().to_string(),
                left_scale,
                &right.unsigned_abs().to_string(),
                right_scale,
            );
            if sign < 0 {
                order.reverse()
            } else {
                order
            }
        }
    }
}

/// Compares two non-zero magnitudes written without leading or trailing zeroes.
///
/// The exponent of the most significant digit orders them first; only where those agree do the
/// digit strings decide. Nothing is multiplied, so a scale of 255 beside an `i128` cannot overflow.
fn magnitude_cmp(left: &str, left_scale: u8, right: &str, right_scale: u8) -> Ordering {
    let left_exponent = left.len() as i64 - 1 - i64::from(left_scale);
    let right_exponent = right.len() as i64 - 1 - i64::from(right_scale);
    match left_exponent.cmp(&right_exponent) {
        Ordering::Equal => {
            let width = left.len().max(right.len());
            left.bytes()
                .chain(std::iter::repeat(b'0'))
                .take(width)
                .cmp(right.bytes().chain(std::iter::repeat(b'0')).take(width))
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::{Observed, Repr};
    use std::cmp::Ordering;

    fn number(text: &str) -> serde_json::Number {
        text.parse().expect("a JSON number")
    }

    fn bits(value: Observed) -> u64 {
        value.binary().to_bits()
    }

    /// The measured profile of the actual service-producing source tool, and the one this rule
    /// fixes. The default-feature library profile reads the same token as `408d92f633a24cda`; the
    /// two are recorded as different profiles rather than claimed equivalent, and this asserts the
    /// one the contract names.
    #[test]
    fn the_wire_door_reads_the_witness_token_as_the_source_cli_profile() {
        let observed =
            Observed::of_number(&number("946.3702156715110866946")).expect("inside the domain");
        assert_eq!(bits(observed), 0x408d_92f6_33a2_4cdb);
        assert_ne!(bits(observed), 0x408d_92f6_33a2_4cda);
        assert_eq!(observed.exact_text(), "946.3702156715111");
    }

    /// Integer first, so two adjacent integers past 2^53 stay two values.
    #[test]
    fn the_wire_door_reads_an_integer_token_exactly_and_never_through_a_binary64() {
        let high = Observed::of_number(&number("9007199254740993")).expect("inside the domain");
        let low = Observed::of_number(&number("9007199254740992")).expect("inside the domain");
        assert_eq!(high.cmp(low), Ordering::Greater);
        assert!(matches!(
            high.0,
            Repr::Exact {
                units: 9007199254740993,
                scale: 0,
                ..
            }
        ));
    }

    /// A token this runtime holds and the source's wire reader refuses is an absence, not a panic
    /// and not a saturated value.
    #[test]
    fn a_token_outside_the_source_domain_is_absent_rather_than_saturated() {
        assert!(Observed::of_number(&number("1e400")).is_none());
        assert!(Observed::of_number(&number("-1e400")).is_none());
        assert!(Observed::of_literal("1e400").is_none());
        assert!(Observed::of_literal("nonsense").is_none());
    }

    /// The scale-zero branch, which is what an authored integer literal past 2^53 depends on: it is
    /// kept exactly rather than collapsed to the binary64 the same text parses to.
    #[test]
    fn the_literal_door_keeps_an_authored_integer_past_the_binary64_span() {
        assert!(matches!(
            Observed::of_literal("9007199254740993").expect("finite").0,
            Repr::Exact {
                units: 9007199254740993,
                scale: 0,
                ..
            }
        ));
        assert_eq!(
            Observed::of_literal("9007199254740993")
                .expect("finite")
                .cmp(Observed::of_literal("9007199254740992").expect("finite")),
            Ordering::Greater
        );
        assert_eq!(
            Observed::of_literal("9007199254740993")
                .expect("finite")
                .exact_text(),
            "9007199254740993"
        );
    }

    /// The two doors are separate rules, and the literal door keeps an authored decimal the wire
    /// door would have collapsed only where the source keeps it.
    #[test]
    fn the_literal_door_keeps_an_authored_decimal_only_where_the_source_keeps_it() {
        // Scale zero: kept exactly.
        assert!(matches!(
            Observed::of_literal("1").expect("finite").0,
            Repr::Exact {
                units: 1,
                scale: 0,
                ..
            }
        ));
        // Equals the canonical decimal of its own binary64: kept exactly.
        assert!(matches!(
            Observed::of_literal("0.5").expect("finite").0,
            Repr::Exact {
                units: 5,
                scale: 1,
                ..
            }
        ));
        // Binary64 does not carry it, so it collapses to the binary64 — which is exactly `1`.
        let collapsed = Observed::of_literal("1.0000000000000000001").expect("finite");
        assert_eq!(
            collapsed.cmp(Observed::of_literal("1").expect("finite")),
            Ordering::Equal
        );
    }

    /// `-0.0` and `0.0` are one value, which is what a guard `amount == 0` has to mean.
    #[test]
    fn negative_zero_and_zero_are_one_observed_value_with_one_canonical_text() {
        let negative = Observed::of_number(&number("-0.0")).expect("finite");
        let positive = Observed::of_number(&number("0")).expect("finite");
        assert_eq!(negative.cmp(positive), Ordering::Equal);
        assert_eq!(negative.exact_text(), "0");
        assert_eq!(positive.exact_text(), "0");
        assert!(negative.is_zero());
    }

    /// The underflow class: exactly nonzero, and falsy, because the carried binary64 is what the
    /// source's truthiness arm tests.
    #[test]
    fn an_underflowing_token_is_falsy_because_the_carried_binary_is_zero() {
        let observed = Observed::of_number(&number("1e-400")).expect("finite");
        assert!(observed.is_zero());
    }

    /// One spelling per value: three ways of writing one number address alike.
    #[test]
    fn one_value_has_one_canonical_text_however_it_was_spelled() {
        for text in ["1", "1.0", "1e0", "1.000", "0.1e1"] {
            let observed = Observed::of_number(&number(text)).expect("finite");
            assert_eq!(observed.exact_text(), "1", "{text}");
        }
        assert_eq!(
            Observed::of_number(&number("-1.500"))
                .expect("finite")
                .exact_text(),
            "-1.5"
        );
        assert_eq!(
            Observed::of_number(&number("0.0001"))
                .expect("finite")
                .exact_text(),
            "0.0001"
        );
    }

    /// Ordering across scales never scales one operand into an overflow.
    #[test]
    fn exact_ordering_holds_across_scales_and_signs() {
        for (left, right, expected) in [
            ("1e3", "999.999", Ordering::Greater),
            ("-1.20", "-1.19", Ordering::Less),
            ("0", "0.1", Ordering::Less),
            ("0", "-0.001", Ordering::Greater),
            ("100", "100.0", Ordering::Equal),
            ("-0.0", "0", Ordering::Equal),
        ] {
            let l = Observed::of_number(&number(left)).expect("finite");
            let r = Observed::of_number(&number(right)).expect("finite");
            assert_eq!(l.cmp(r), expected, "{left} versus {right}");
            assert_eq!(r.cmp(l), expected.reverse(), "{right} versus {left}");
        }
    }
}
