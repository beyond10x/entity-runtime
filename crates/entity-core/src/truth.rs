//! Three-valued truth, and why two values were not enough.
//!
//! A lifecycle ladder can be governed with `true` and `false`: either the instance is in a state
//! the operation leaves from, or it is not. An **evidence gate** cannot. *No review has been
//! recorded* and *the review says rejected* are different facts about the world, and a rule
//! language that reports both as `false` tells whoever reads the refusal to fix the wrong thing.
//!
//! So a condition evaluates to [`Truth`], a rule holds only when the answer is [`Truth::True`],
//! and a rule that could not be answered is refused by its own error — one that names what nobody
//! observed. See [`CoreError::PreconditionUnobservable`](crate::CoreError::PreconditionUnobservable).
//!
//! The connectives are Kleene's, which is the only three-valued logic that agrees with two-valued
//! logic on every input that has no `Unknown` in it. That property is what makes this change safe
//! for definitions that never touch a missing reference: they evaluate exactly as they did.

use std::fmt;

/// The result of evaluating a condition.
///
/// `Unknown` means *nothing has been observed here*; it is distinct from `False`, which means
/// *something was observed and it contradicts the rule*.
///
/// This mirrors, deliberately and name for name, the `Truth` that `aep` already
/// evaluates its predicates with (`crates/aep-domain/src/predicate.rs`). Two kernels that disagree
/// about what `Unknown` means would disagree about whether a gate passed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Truth {
    /// Observed to hold.
    True,
    /// Observed not to hold.
    False,
    /// Not observable: a reference the condition reads has no value.
    Unknown,
}

impl Truth {
    /// Kleene conjunction: `False` dominates, then `Unknown`.
    ///
    /// A rule that is contradicted by something observed is refused whatever else is missing —
    /// which is why `False` wins over `Unknown` rather than the other way round.
    #[must_use]
    pub fn and(self, other: Self) -> Self {
        match (self, other) {
            (Self::False, _) | (_, Self::False) => Self::False,
            (Self::Unknown, _) | (_, Self::Unknown) => Self::Unknown,
            (Self::True, Self::True) => Self::True,
        }
    }

    /// Kleene disjunction: `True` dominates, then `Unknown`.
    #[must_use]
    pub fn or(self, other: Self) -> Self {
        match (self, other) {
            (Self::True, _) | (_, Self::True) => Self::True,
            (Self::Unknown, _) | (_, Self::Unknown) => Self::Unknown,
            (Self::False, Self::False) => Self::False,
        }
    }

    /// Kleene negation: `Unknown` negates to itself.
    ///
    /// `not` cannot turn *nobody looked* into *it is wrong*. This never surprises a definition
    /// author in practice, because the only conditions that answer `Unknown` are questions about
    /// a value that is not there — and nobody writes `not` around one of those expecting `true`.
    /// [`Condition::Exists`](crate::Condition::Exists) asks about the store instead, is
    /// two-valued, and negates in the ordinary way.
    #[must_use]
    #[allow(clippy::should_implement_trait)]
    pub fn not(self) -> Self {
        match self {
            Self::True => Self::False,
            Self::False => Self::True,
            Self::Unknown => Self::Unknown,
        }
    }

    /// `true` only for [`Truth::True`] — what a rule requires.
    #[must_use]
    pub fn is_satisfied(self) -> bool {
        self == Self::True
    }

    /// The value as it appears in output: `true`, `false`, `unknown`.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::True => "true",
            Self::False => "false",
            Self::Unknown => "unknown",
        }
    }

    /// Builds a truth value from a boolean observation.
    #[must_use]
    pub fn from_bool(value: bool) -> Self {
        if value {
            Self::True
        } else {
            Self::False
        }
    }
}

impl fmt::Display for Truth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::Truth;
    use super::Truth::{False, True, Unknown};

    /// The property that makes this change safe: on inputs with no `Unknown`, Kleene *is* boolean
    /// logic. A definition that never reads a missing reference evaluates exactly as it did.
    #[test]
    fn kleene_agrees_with_boolean_logic_wherever_nothing_is_unknown() {
        for left in [true, false] {
            for right in [true, false] {
                let (l, r) = (Truth::from_bool(left), Truth::from_bool(right));
                assert_eq!(l.and(r), Truth::from_bool(left && right));
                assert_eq!(l.or(r), Truth::from_bool(left || right));
            }
            assert_eq!(Truth::from_bool(left).not(), Truth::from_bool(!left));
        }
    }

    #[test]
    fn false_dominates_conjunction_and_true_dominates_disjunction() {
        assert_eq!(False.and(Unknown), False);
        assert_eq!(Unknown.and(False), False);
        assert_eq!(True.and(Unknown), Unknown);
        assert_eq!(True.or(Unknown), True);
        assert_eq!(Unknown.or(True), True);
        assert_eq!(False.or(Unknown), Unknown);
    }

    #[test]
    fn negation_cannot_turn_nobody_looked_into_it_is_wrong() {
        assert_eq!(Unknown.not(), Unknown);
    }

    #[test]
    fn only_true_satisfies_a_rule() {
        assert!(True.is_satisfied());
        assert!(!False.is_satisfied());
        assert!(!Unknown.is_satisfied());
    }

    /// Order-independence is what lets `all`/`any` evaluate every operand without changing the
    /// answer — which is what makes a complete list of unobserved addresses affordable.
    #[test]
    fn the_connectives_are_commutative_and_associative() {
        let values = [True, False, Unknown];
        for &a in &values {
            for &b in &values {
                assert_eq!(a.and(b), b.and(a));
                assert_eq!(a.or(b), b.or(a));
                for &c in &values {
                    assert_eq!(a.and(b).and(c), a.and(b.and(c)));
                    assert_eq!(a.or(b).or(c), a.or(b.or(c)));
                }
            }
        }
    }
}
