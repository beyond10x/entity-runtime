# File Store v2 written by entity-store 0.33.1

`store/` is a File Store v2 root written by the released `entity-store` 0.33.1 library (tag
`0.33.1`, commit `45d2b64cde22d77fe2ee0816cce4ceaea0a9c580`), not by hand. `expected.json` is what
that library read back from it: for every subject its `load`, `records`, `observations` and
`events`, and the canonical bytes of each record in both record forms
(`record_comparison_bytes_in`). `tests/file_store_v3/mod.rs` holds this release's v2 reader and the
v3 migration to exactly these values. Tests copy the store before opening it; never open it in
place.

It holds two definitions (`counter`, `note`) and five subjects:

| subject | how 0.33.1 wrote it |
|---|---|
| `counter/c1` | a recorded creation, eight recorded operations (three `charge` with fractional amounts), an observation at revision 3 |
| `counter/c2` | a recorded creation and an observation with a `null` body |
| `counter/p1` | plain `commit`s only: no record, two events |
| `counter/legacy-1` | a pre-0.15 subject migrated by `migrate_file_store_v1` (`legacy_snapshot`, one legacy event), then one recorded operation |
| `note/n1` | a recorded creation, then a plain `commit` that moves the state past the last record |

Made on 2026-11-23 by a scratch example run inside a `git archive 0.33.1` tree:
`cargo run --locked -p entity-store --example gen_v2_fixture -- <out>`. The example is not kept;
it called only the 0.33.1 public API (`Runtime::create`/`execute`, `RecordedCommit::new`,
`FileStore::commit_recorded`/`commit`/`observe`, `migrate_file_store_v1`) with fixed timestamps.

```
a0f6d41bf20dcad0bca5c99014aa3116db5b9a89aae4bc6d7403a261f5acb0ce  expected.json
cc674ab9ca3b71e48cf776285f6756b4177076dafa8fa8411fc470683ccb65fe  store/.entity-store-format
0a88111852095cae045340ea1f0b279944b2a756a213d9b50107d7489771e159  store/.entity-store-lock
cefec4e1ce93fc0f13413e02b4768c2e27b57ca4350a2e702df2e2bc480165e7  store/subjects/636f756e746572/6331.json
1adf0d584fd5f5b360e8aa7a778e1526637ba184a0153bc6f359e213bb9166f2  store/subjects/636f756e746572/6332.json
10fb60ac5d9cdcfd7132ffcb26ecbe3ade4128239b25e517fe7a513fde39eac7  store/subjects/636f756e746572/6c65676163792d31.json
74911735a8052bd0d60ca59c792471fe4b89412ae3aca129b250cdac9ecd9d48  store/subjects/636f756e746572/7031.json
efec2eea4a39229aad028a83e5fcb532cce9ea47f198882424222ff6881704f8  store/subjects/6e6f7465/6e31.json
```
