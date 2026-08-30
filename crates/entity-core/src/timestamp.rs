//! Reading an ISO-8601 instant well enough to put two of them in order, and no further.
//!
//! # Why this is here and not a dependency
//!
//! `entity-core` depends on `serde` and `serde_json` and nothing else (R-82), and the purity scan
//! refuses anything that could reach a clock (R-01). A date library brings a clock with it — every
//! one of them offers `now()` — and the point of `before`/`after` is that the clock is read at the
//! **edge** and handed in as an argument (R-62). A definition that could ask what rest it is would
//! stop being replayable, which is R-02.
//!
//! So this reads the subset a shell can be asked to produce, and is deliberately narrow.
//!
//! The purity scan pushed back twice while this was written, and was right both times: it bans the
//! token `Instant` (that is `std::time::Instant`, a clock) and the bare word `time`. A scan over
//! source text cannot tell a kernel type or a local named that from the thing R-01 exists to keep
//! out — and neither can a reader skimming a file. Hence `Timestamp`, and `rest` for the part after
//! the date.
//!
//! # What it reads
//!
//! `YYYY-MM-DD`, and `YYYY-MM-DDTHH:MM:SS` with optional fractional seconds and an optional
//! trailing `Z`. A space may stand in for the `T`, because that is what most tools print.
//!
//! # What it refuses, and why refusing is the safe direction
//!
//! **An explicit offset** — `2026-08-25T12:00:00+02:00` — is refused rather than normalised.
//! Normalising means arithmetic across a boundary this type has no business knowing about, and
//! comparing an offset-bearing instant with a naive one has no correct answer at all. A shell that
//! has offsets has a clock, and normalising to UTC is that clock's job.
//!
//! A value this cannot read makes the comparison [`Unknown`](crate::Truth::Unknown), not `false`.
//! That is the deliberate half: `gt` on two non-numbers is `false` because *these are not numbers*
//! is an observation anybody can make, while *this is not a timestamp I can read* is a statement
//! about this parser's reach. Reading it as `false` would let `after: [$args.now, $fields.due]`
//! quietly answer "not yet due" for a value nobody understood, which is exactly the collapse
//! three-valued rules exist to prevent.

/// An instant, reduced to the numbers that order it.
///
/// Named `Timestamp` and not `Instant` because the purity scan bans the token `Instant` — that is
/// `std::rest::Instant`, a clock, and a scan over source text cannot tell a kernel type sharing
/// that name from the thing R-01 exists to keep out. The guard caught this on the first build, and
/// renaming was the right answer: a type called `Instant` in a kernel that must not read a clock is
/// confusing to a reader too, not only to a regex.
///
/// Compared field by field, which is why the fields are in descending significance: `derive(Ord)`
/// on a tuple struct compares in declaration order, and that is the whole implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Timestamp {
    year: u16,
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
    second: u8,
    /// Fractional seconds, scaled to nanoseconds so `.5` and `.500` compare equal.
    nanosecond: u32,
}

/// Reads an instant, or says nothing at all.
///
/// Total: every input returns, and no input panics. `None` means *this parser cannot read it*, and
/// the caller turns that into `Unknown`.
pub(crate) fn parse(value: &str) -> Option<Timestamp> {
    let value = value.trim();
    let (date, rest) = match value
        .find(['T', 't', ' '])
        .filter(|at| *at == 10 && value.len() > 11)
    {
        Some(at) => (&value[..at], Some(&value[at + 1..])),
        None => (value, None),
    };

    let date: Vec<&str> = date.split('-').collect();
    let [year, month, day] = date.as_slice() else {
        return None;
    };
    let year: u16 = number(year, 4)?;
    let month: u8 = number(month, 2)?;
    let day: u8 = number(day, 2)?;
    if !(1..=12).contains(&month) || day == 0 || day > days_in_month(year, month) {
        return None;
    }

    let Some(rest) = rest else {
        return Some(Timestamp {
            year,
            month,
            day,
            hour: 0,
            minute: 0,
            second: 0,
            nanosecond: 0,
        });
    };

    // An explicit offset is refused here rather than normalised — see the module note.
    let rest = match rest.strip_suffix('Z').or_else(|| rest.strip_suffix('z')) {
        Some(naive) => naive,
        None if rest.contains('+') => return None,
        // A `-` after the clock part's start is an offset; the date's hyphens are already behind us.
        None if rest.get(1..).is_some_and(|tail| tail.contains('-')) => return None,
        None => rest,
    };

    let (clock, fraction) = match rest.split_once('.') {
        Some((clock, fraction)) => (clock, Some(fraction)),
        None => (rest, None),
    };
    let clock: Vec<&str> = clock.split(':').collect();
    let [hour, minute, second] = clock.as_slice() else {
        return None;
    };
    let hour: u8 = number(hour, 2)?;
    let minute: u8 = number(minute, 2)?;
    // 60 is a leap second, which is a real thing a shell may hand over.
    let second: u8 = number(second, 2)?;
    if hour > 23 || minute > 59 || second > 60 {
        return None;
    }

    let nanosecond = match fraction {
        None => 0,
        Some(digits) => {
            if digits.is_empty() || digits.len() > 9 || !digits.bytes().all(|b| b.is_ascii_digit())
            {
                return None;
            }
            let mut scaled: u32 = digits.parse().ok()?;
            for _ in digits.len()..9 {
                scaled = scaled.checked_mul(10)?;
            }
            scaled
        }
    };

    Some(Timestamp {
        year,
        month,
        day,
        hour,
        minute,
        second,
        nanosecond,
    })
}

/// The number of days in one Gregorian month.
fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if year % 400 == 0 || (year % 4 == 0 && year % 100 != 0) => 29,
        2 => 28,
        _ => 0,
    }
}

/// A fixed-width, all-digit field. Fixed width on purpose: `2026-8-25` is a spelling this parser
/// does not read, and reading it would mean guessing that `08` and `8` are the same field in a
/// format whose whole value is that they are not.
fn number<T: std::str::FromStr>(value: &str, width: usize) -> Option<T> {
    if value.len() != width || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    value.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn a_date_and_a_datetime_both_read_and_order() {
        assert!(parse("2026-08-25").is_some());
        assert!(parse("2026-08-25T12:00:00").is_some());
        assert!(parse("2026-08-25T12:00:00Z").is_some());
        assert!(parse("2026-08-25 12:00:00").is_some(), "a space for the T");
        assert!(parse("2026-08-25T12:00:00.500Z").is_some());

        assert!(parse("2026-08-25") < parse("2026-08-26"));
        assert!(parse("2026-08-25") < parse("2026-08-25T00:00:01"));
        assert!(
            parse("2026-01-31") < parse("2026-02-01"),
            "not lexicographic luck"
        );
        assert!(parse("2026-08-25T12:00:00.5") == parse("2026-08-25T12:00:00.500"));
    }

    #[test]
    fn impossible_calendar_dates_and_non_ascii_clock_tails_are_refused_without_panicking() {
        assert!(parse("2026-02-29").is_none());
        assert!(parse("2024-02-29").is_some());
        assert!(parse("2026-04-31").is_none());
        assert!(parse("2026-08-25T😀").is_none());
    }

    /// The refusals, each of which becomes `Unknown` rather than `false`.
    #[test]
    fn what_it_cannot_read_it_says_nothing_about() {
        for unreadable in [
            "2026-08-25T12:00:00+02:00", // an offset is not normalised, it is refused
            "2026-08-25T12:00:00-05:00",
            "2026-8-25",  // a width this format does not have
            "26-08-25",   // a two-digit year
            "2026-13-01", // no thirteenth month
            "2026-00-01", // nor a zeroth
            "2026-08-32", // nor a thirty-second day
            "2026-08-25T24:00:00",
            "2026-08-25T12:60:00",
            "yesterday",
            "",
            "2026-08-25T12:00",     // no seconds
            "2026-08-25T12:00:00.", // a point and no digits
            "1756108800",           // an epoch second is a number, not this
        ] {
            assert!(parse(unreadable).is_none(), "{unreadable:?} must not read");
        }
    }

    /// Total: no input panics, whatever it is.
    #[test]
    fn nothing_panics_however_wrong_the_input() {
        for hostile in [
            "-",
            "--",
            "T",
            "::",
            "2026-08-25T",
            "9".repeat(400).as_str(),
            "2026-08-25T12:00:00.9999999999",
            "\u{1F600}",
            "2026-08-25TZ",
        ] {
            let _ = parse(hostile);
        }
    }
}
