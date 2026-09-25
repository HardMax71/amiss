#!/usr/bin/env bash
set -uo pipefail

input="$(cat)"
command -v jq > /dev/null && input="$(printf '%s' "$input" | jq -r '.tool_input.command // empty')"
case "$input" in
  *"git commit"* | *"git -C "*" commit"*) ;;
  *) exit 0 ;;
esac

repo="${CLAUDE_PROJECT_DIR:-.}"
if ! command -v amiss > /dev/null; then
  echo "amiss is not installed, so the commit goes ahead unchecked; cargo install --locked amiss" >&2
  exit 0
fi
base="$(git -C "$repo" rev-parse -q --verify 'HEAD^{commit}')" || exit 0
format="$(git -C "$repo" rev-parse --show-object-format 2> /dev/null || echo sha1)"

report="$(amiss check --repo "$repo" --object-format "$format" --base "$base" --index \
  --profile enforce-introduced 2>&1)"
case $? in
  0) exit 0 ;;
  *)
    printf '%s\n' "$report" >&2
    exit 2
    ;;
esac
