# Provider tracking verification

The final integration gate, `task check`, exited 0 with the published Eventlog dependency
`6983cc25eb92e07844b3a6fa3e0decbdb300f43c`. Both real PostgreSQL lanes executed against the
same disposable fixture. All 421 original ESS scenarios and all 17 provider scenarios passed,
with no ESS skips or unsupported cases. The provider suite contains seven authored semantic
cases and ten generated command-response cases. Exact reports and suites are admitted in AEP.

The release performance probe was then run explicitly with Rust 1.91:

```console
CARGO_BUILD_JOBS=2 cargo +1.91.0 test -p entity-eventlog --features sqlite,sync-bridge --release --locked --test shared_clock_cost -- --ignored --nocapture
```

It passed on the first final integrated run: warm batch medians were 11.532148 ms, 13.049370 ms,
and 13.568977 ms for 55, 601 and 1,203 initial provider events. Growth was 1.132x and 1.177x,
below the unchanged 2x bound. The ordinary gate intentionally ignores this timing probe;
this separate command executed it. `performance-final.txt` retains the complete test stdout,
starting at `running 1 test`; compiler output with local paths remains private.

The earlier matched baseline failed (92.517484 ms / 7,038.153041 ms / 12,449.723852 ms).
The first development treatment passed (10.511861 ms / 11.983571 ms / 17.701283 ms).
The final run above follows receipt-validation and projection-alias review corrections and
uses the published Git dependency without path overrides. The governed story and review
records distinguish these runs and retain the negative controls.

Only warm batches have the 2x claim. Cold full verification and full-history output scale
with their inputs and outputs. SQL-visible changes invalidate tracked observations;
raw database-file edits bypassing SQLite are outside this explicitly selected warm policy.
No consumer adoption or complete consumer-invocation benchmark is claimed.

`source.sha256` identifies the implementation and Cargo inputs tested. Verify it from the
repository root with `sha256sum --check`. The final commit additionally records documents
and evidence without changing those inputs.
