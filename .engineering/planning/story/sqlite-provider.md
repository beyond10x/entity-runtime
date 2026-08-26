---
format: aep.planning-md/1
id: story:sqlite-provider
kind: story
status: implemented
title: A second provider, so the SPI has two implementors rather than one
summary: entity-sqlite writes state and events in one transaction, which is the case a file store cannot be tested for.
relations:
- decomposes: epic:the-shell
revision: 4
---
# Story: A second provider, so the SPI has two implementors rather than one

## Outcome

An adopter who needs a refused write to leave *nothing* behind has a provider that can promise it.
The memory and file stores write both halves and hope; SQLite writes them inside one `BEGIN` and
either both land or neither does, and there is a test that pulls the plug between them.

## Context

`Store::commit` claims an instance and its events arrive together (R-83). A trait with one
implementor is a description of that implementor, and a file store cannot demonstrate atomicity —
so the claim was untestable until something transactional implemented it. This is also the story
that makes `story:provider-conformance` mean something: a suite run against one provider proves
nothing about the trait.

## Acceptance

`entity-sqlite` implements `Store`; one `BEGIN`, both writes, one `COMMIT`; the revision check reads
inside the transaction, so what is checked is what is written against; a refused commit rolls both
halves back and leaves no trace; the store survives being closed and reopened; and it passes the
conformance suite unchanged. R-103.

## Out of Scope

Connection pooling, migrations of the SQLite schema itself, and any query surface beyond what
`StateProvider` and `EventProvider` declare. A second database — Postgres — is a later story and
adds a dependency this workspace has a written policy about.

## Open Questions

None outstanding.
