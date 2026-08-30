#!/usr/bin/env bash
# Run one git command with b10x-bot authorship and push authentication.
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
bot_id='195735192'
common=(
  -c user.name='b10x-bot[bot]' \
  -c user.email="${bot_id}+b10x-bot[bot]@users.noreply.github.com" \
)

if [[ "${1:-}" != push ]]; then
  exec git "${common[@]}" "$@"
fi

# A token exists only for the push process. Commits, tags and every read-only git command neither
# mint one nor put one in their environment.
bot_token="$(${repo_root}/scripts/bot-token.sh)"
export B10X_BOT_TOKEN="$bot_token"
exec git "${common[@]}" \
  -c 'url.https://github.com/.pushInsteadOf=git@github.com:' \
  -c 'credential.https://github.com.helper=' \
  -c 'credential.https://github.com.helper=!f() { echo username=x-access-token; echo "password=${B10X_BOT_TOKEN}"; }; f' \
  "$@"
