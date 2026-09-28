# Provider-neutral query contract

Scope: `entity-query`, exercised through its `MemoryStore` implementation and public ordered-input
API. The ESS declaration is `ess/domains/query.yaml`; the component is `ess/components/query.yaml`;
scenarios are in `ess/scenarios/query/`. The adapter in `checks/ess-conformance/src/query.rs` reads
actual query results. R-119 and `docs/design/provider-query-v0.1.md` §1 remain the traceability and
rationale for recursive containment and byte-ordered keyset pages.

| R-119 contract | Public/API implementation evidence | Named scenarios |
| --- | --- | --- |
| Object containment requires every requested member recursively | `query_ordered_instances` and `contains` in `crates/entity-query/src/lib.rs`; existing `nested_document_matching_has_the_same_containment_meaning_as_jsonb` | `recursive-containment` |
| Array containment ignores order and duplicate requested membership and recurses into objects | array branch of `contains` | `recursive-containment` |
| Exact numeric equality across spellings, without large-integer collision or exponent overflow | numeric branch of `contains`; `entity_core::compare_numbers` in `crates/entity-core/src/number.rs`; existing `containment_compares_numeric_values_across_json_representations` | `numeric-values-remain-exact`, `nested-numeric-containment` |
| Missing is different from present null; every top-level predicate must match | `.get(...).is_some_and(...)` at top-level and nested object boundaries | `recursive-containment` |
| Default limit 100, inclusive bounds 1 through 1000 | `DocumentQuery::effective_limit`, `DEFAULT_LIMIT`, `MAX_LIMIT`; existing `page_limits_are_bounded` | `page-limit-boundaries` |
| Memory pages are byte-identity ordered; traversal of a fixed dataset is complete | `DocumentQueryProvider for MemoryStore`, `query_ordered_instances`, `DocumentPage::from_matches` | `fixed-dataset-complete-traversal`, `matching-pages-skip-nonmatches` |
| Continuation supports a changed page size and ends without a cursor | query identity contains entity and predicates, excluding limit; page lookahead and truncation | `fixed-dataset-complete-traversal` |
| A foreign entity or predicate cursor is invalid | `DocumentQuery::after_id`; existing `a_cursor_is_bound_to_the_query_that_emitted_it` | `foreign-entity-cursor-refuses`, `foreign-predicate-cursor-refuses` |
| Malformed framing, hexadecimal or UTF-8 cursor input is invalid | `after_id`, `decode_hex`, `hex_digit` | `malformed-cursors-are-typed-refusals` |
| Consumed ordered input must name the requested entity and be strictly increasing | `query_ordered_instances` entity/order guards | `consumed-foreign-entity-refuses`, `consumed-duplicate-identity-refuses`, `consumed-descending-identity-refuses`, `consumed-prefix-before-cursor-is-validated` |
| Ordered input validation covers only what is consumed | early termination after limit plus one matching instances | `ordered-api-stops-after-matching-lookahead` |
| Public page assembly truncates an already matching ordered prefix | `DocumentPage::from_matches` | `page-builder-truncates-matches` |
| Query wire document rejects unknown keys | `DocumentQuery` uses `serde(deny_unknown_fields)` | `query-document-keys-are-closed` |

`Query` invokes the real memory query provider. `Ordered` calls `query_ordered_instances` with the
caller-supplied instances. `Page`, `DecodeCursor`, and `Limit` invoke the corresponding public helpers.
`Seed` is explicit fixture setup: it registers the dynamic definition, creates each subject through
the kernel, and commits it through `MemoryStore`. `Continue` retains only the actual cursor from
the prior page, supplies it to a fresh caller query, and invokes the provider again. It does not
reuse previous response items or manufacture an observation.

Outputs include the exact serialized public result, actual identities/count, the actual cursor and
whether it exists. Expectations are literal authored scenario responses. Neither adapter nor
fixture controls compare against expected answers. `value` is a lossless JSON document carrier;
it can hold an actual page or the actual fixture-created instances. The wire document types have
one underlying owner, `entity.core.JsonDocument`. Numeric JSON tokens remain in text until decoded
by `serde_json` with `arbitrary_precision`; they never traverse ESS's generic numeric Node decoder.

Cursor identity binds the canonical serialization of entity and predicates. Numerically equivalent
predicate spellings can therefore have equal containment behavior and distinct cursor identities.
The cursor is an opaque continuation, not an authorization token or snapshot handle. **No snapshot
isolation or stable traversal under concurrent dataset changes is promised.** Ordered-input
validation checks all consumed values, including those below the cursor bound, and stops after the
matching lookahead. `DocumentPage::from_matches` assumes matching ordered input; it is not a second
entity/order/containment validator.

Static dependency boundaries remain in Cargo manifests: the query crate depends on core/store;
ESS is isolated to the checker. PostgreSQL and Eventlog query facades consume these APIs but are
outside this contract's selected implementation coverage.

Mutation obligation: replacing the exact numeric comparator with `serde_json::Number` representation
equality must fail `numeric-values-remain-exact` when query `100.0` misses stored `100`. The retained
integration suite/report and mutation evidence establish execution; this traceability register does
not itself claim that a suite ran.
