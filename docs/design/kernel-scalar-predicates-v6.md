# Scalar predicates, outcome profile 6

R-131. `story:scalar-predicate-semantics` owns this extension. Definition and record
formats `entity-outcome-definition/6` and `entity-outcome-record/6` retain profiles
1–5 and admit `truthy` and `scalar_compare` conditions. Existing `exists`, equality,
numeric ordering, timestamp and collection operators retain their meaning.

`truthy` takes one scalar operand using the existing checked reference/literal
grammar. Booleans use their value; a numeric value converted to a finite binary64
is true iff that conversion is nonzero; text is true iff nonempty and not exactly
`false`. This numeric truthiness deliberately follows ESS `FactValue::is_truthy`,
including signed zero and underflow, rather than inventing exact-number truthiness.
Out-of-range conversion, missing/null or non-scalar observations are Unknown.

`scalar_compare` has `left`, `right`, `op` (`eq`, `ne`, `lt`, `le`, `gt`, `ge`)
and optional `scales`, a map of names to lowest-first text lists. Numbers compare
exactly using the existing kernel comparator. Scalar equality is type-sensitive;
text equality ignores scales. Ordering text consults every scale containing both
values, using the first occurrence of each value. No such scale, or disagreement
between their orders, is Unknown. Equal text still requires a containing scale for
ordering. Empty scales and duplicate values retain the existing ESS scale meaning;
they are not silently normalized. Boolean or mixed-type ordering is Unknown.

Both operands are resolved before deciding, and unreadable references remain in
the diagnostic set. Unknown remains Unknown under negation and cannot select an
otherwise outcome. Rule locations and lexical scopes follow the existing kernel
contract, including fixed local value bindings. Literals must be scalar; known
container references are refused at registration. Dynamically typed legacy JSON
observations remain checked at evaluation and cannot create scalar values.

These operators inspect supplied values, not wire codecs. Decimal-text fields
remain text until an explicit typed numeric operand codec exists; the future ESS
lowerer must preserve ESS's admitted/canonical numeric value before comparison.
This change does not broaden ESS number admission, implement a lowerer, authenticate
the first replay definition, or prove application adoption. Replay retains and
re-evaluates the complete predicate and named scales. Old profiles refuse the new
operators; old readers refuse profile 6 and old-profile injections. No dependency,
clock, external callback or ambient scale registry is introduced.
