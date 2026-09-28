# Entity Runtime 0.25.1 publication evidence

[PR 47](https://github.com/beyond10x/entity-runtime/pull/47) merged the executable contracts,
planning migration and release preparation into `origin/main` at
`72455539c0756290d03fd5ef28b30512a729c457`. Its tree
`0df0b796470c7b5ee10474068b43d01a62338139` exactly matches the checked candidate
`ba1e725aabba91b440d3f5b4fcd773b0e92ae251`.

The bot's annotated tag `0.25.1` has object
`343d03ddf0150ac0439c12eab8f9505e980f88a3` and peels to that mainline merge.
The workspace version and dated changelog both name 0.25.1. The exact tag's
[release run](https://github.com/beyond10x/entity-runtime/actions/runs/36409629222)
succeeded: tag provenance, repository gate, Eventlog runtime, MSRV, all five platform builds
and bundle assembly. `build.json` records the observed run identity.

The bot published [the release](https://github.com/beyond10x/entity-runtime/releases/tag/0.25.1)
after downloading that run's `release-bundle-0.25.1`. Every archive passed `sha256sum --check
SHA256SUMS`. The complete six-asset set was then checked against GitHub's names, sizes and SHA-256
digests, both before publishing the draft and after publication. `release.json` records those
observations; `SHA256SUMS` is the original bundle file. The extracted Linux x86_64 command
reported `entity 0.25.1`. The release author is `b10x-bot[bot]` and `draft` is false.

The [final source evidence](../final/release/README.md) remains unchanged: 414 passing scenarios,
zero nonpassing or omitted cases in the declared scope, exact suite/report/model association,
complete local gate, independent review and per-domain mutation evidence. ER's planning store is
`aep.project/5` with Git storage. Its pinned AEP revision
`18a18a3f1cfa110dc5c9a675b3e9956f74c29bb7` is published and passed its own complete gate and CI,
and validates ER's migrated store with current committed-history safeguards.

## Separate upstream delivery blocker

[ESS PR 185](https://github.com/beyond10x/ess/pull/185) is merged. The AEP implementation ER uses
is published, but [AEP PR 61](https://github.com/beyond10x/aep/pull/61) remains draft: integrating
later documentation-only mainline changes inherited GitHub branch-update commit
`74956edad272b9489193248b347692edfffdeda6`. Gates refused publication with
`merged pull request missing or ambiguous` because no associated merged PR names that intermediate
commit as its merge commit. This delivery refusal was not bypassed. AEP's governed task
`finish-er-evidence-upstream-publication` and blocker
`upstream-github-branch-update-provenance` are retained with the unpublished integration at
`ba0d7ec714b662963ac1a77a45ca4b8e2751da69` in managed archive
`aep/er-suite-evidence-admission`. This does not change ER's published pin or its release evidence.

## Cleanup handoff

Both ESS worktrees have been archived and removed by managed GC. The AEP worktree was also removed
using a verified archive preserving its two unpublished commits and ignored local output;
`aep-cleanup.json` retains the archive identity and removal observation. The ER release-record
worktree and read-only CI observation worktree
are retired after this record is published. Exact archive and GC results remain in the operator's
local release evidence directory. No unrelated worktree or primary-checkout changes are removed.

Documentation publication is asynchronous and was not verified as part of this source release.
