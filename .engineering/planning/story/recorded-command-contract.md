---
format: aep.planning-md/1
id: story:recorded-command-contract
kind: story
status: draft
title: Stored CLI commands require complete recording metadata
summary: Create and execute commit the exact record they print and reject partial provenance.
relations:
- decomposes: epic:the-shell
revision: 1
---
## Context

Only execute exposes envelope flags, partial sets are silently discarded, timestamps are not validated consistently and stored commands currently commit bare events.

## Acceptance

Stored create and execute require a record id, valid recorded time and explicit actor choice, preserve optional correlation and causation, print the committed record in the selected format and reject incomplete invocation with exit 2.
