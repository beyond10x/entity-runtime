#!/usr/bin/env bash
# The gate step for the one provider that needs a server. Runs the Postgres provider's tests when
# `ENTITY_POSTGRES_URL` names one; otherwise says, in one line the gate's output carries, that it
# did not — so a green gate cannot read as a tested provider. CI sets the variable against a service
# container; a laptop without a database stays green and stays honest.
set -euo pipefail
if [[ -z "${ENTITY_POSTGRES_URL:-}" ]]; then
  echo "postgres-check: skipped, ENTITY_POSTGRES_URL unset"
  exit 0
fi
echo "postgres-check: running against ENTITY_POSTGRES_URL"
cargo test -p entity-postgres --locked
