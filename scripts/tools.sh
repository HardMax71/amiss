#!/bin/sh
set -eu

json=$(cargo metadata --format-version 1 --no-deps --offline)

digest() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum; else shasum -a 256; fi
}

case "${1:?usage: tools.sh prebuilt|source|key|version <name>}" in
  prebuilt)
    printf '%s' "$json" | jq -re '.metadata.tools.prebuilt | to_entries | map("\(.key)@\(.value)") | join(",")'
    ;;
  source)
    printf '%s' "$json" | jq -re '.metadata.tools.source | to_entries | map("\(.key)@\(.value)") | join(" ")'
    ;;
  key)
    printf '%s' "$json" | jq -re '.metadata.tools | tojson' | digest | cut -c1-16
    ;;
  version)
    result=$(printf '%s' "$json" | jq -re --arg name "${2:?usage: tools.sh version <name>}" \
      '.metadata.tools | .prebuilt + .source | .[$name] // empty')
    [ -n "$result" ] || { echo "unknown tool: $2" >&2; exit 1; }
    printf '%s\n' "$result"
    ;;
  *)
    echo "usage: tools.sh prebuilt|source|key|version <name>" >&2
    exit 1
    ;;
esac
