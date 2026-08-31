---
format: aep.planning-md/1
id: story:postgres-advisory-lock-text-key
kind: story
status: implemented
title: PostgreSQL absent-identity locks accept ordinary text
summary: Hash advisory-lock coordinates without injecting a forbidden NUL byte.
relations:
- decomposes: epic:the-store-an-adopter-runs-on
- serves: vision:O2
revision: 5
---
## Context

The first live aep-service authority run showed that the provider built its advisory-lock key with an embedded NUL. PostgreSQL text rejects that byte, so every fresh create command failed before semantic evaluation even though the unset-server gate had remained green.

## Acceptance

An ordinary namespace and identity acquire a transaction-scoped absent-identity lock on PostgreSQL, the pair remains structurally separated for hashing, and the PostgreSQL conformance suite exercises the call against a live server.

## Implementation

`PostgresSession::lock_identity` now hashes the namespace first and uses that hash as the seed for hashing the identity, so PostgreSQL receives two valid text parameters and their structure cannot collapse through delimiter ambiguity. The existing R-120 PostgreSQL test now acquires an ordinary locator lock before exercising the remaining session primitives.
