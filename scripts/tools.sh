#!/bin/sh
set -eu

metadata() {
  cargo metadata --format-version 1 --no-deps --offline
}

case "${1:?usage: tools.sh prebuilt|source|key|version <name>}" in
  prebuilt)
    metadata | jq -r '.metadata.tools.prebuilt | to_entries | map("\(.key)@\(.value)") | join(",")'
    ;;
  source)
    metadata | jq -r '.metadata.tools.source | to_entries | map("\(.key)@\(.value)") | join(" ")'
    ;;
  key)
    metadata | jq -r '.metadata.tools | tojson' | sha256sum | cut -c1-16
    ;;
  version)
    result=$(metadata | jq -r --arg name "${2:?usage: tools.sh version <name>}" \
      '.metadata.tools | .prebuilt + .source | .[$name] // empty')
    [ -n "$result" ] || { echo "unknown tool: $2" >&2; exit 1; }
    printf '%s\n' "$result"
    ;;
  *)
    echo "usage: tools.sh prebuilt|source|key|version <name>" >&2
    exit 1
    ;;
esac
