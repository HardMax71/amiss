#!/usr/bin/env bash
set -euo pipefail

usage() {
  echo "usage: scripts/corpus.sh [--update] <amiss-binary> <work-directory>" >&2
  exit 2
}

update=false
if [[ "${1:-}" == "--update" ]]; then
  update=true
  shift
fi
[[ $# -eq 2 ]] || usage

amiss="$(realpath "$1")"
work="$2"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
corpus="$here/corpus.tsv"
tripwires="$here/corpus-tripwires.tsv"
header=$'repository\tstatus\tscanned\tunsupported\tresolved\tdeclined\tmissing'

mkdir -p "$work/repos"
observed="$work/observed.tsv"
echo "$header" > "$observed"

while IFS=$'\t' read -r name url sha; do
  [[ -z "$name" || "$name" == repository ]] && continue
  tree="$work/repos/$name"
  if [[ "$(git -C "$tree" rev-parse -q --verify HEAD 2>/dev/null)" != "$sha" ]]; then
    rm -rf "$tree"
    git init -q "$tree"
    git -C "$tree" fetch -q --depth 1 "$url" "$sha"
    git -C "$tree" checkout -q --detach FETCH_HEAD
  fi
  report="$work/report.json"
  "$amiss" check --repo "$tree" --object-format sha1 --base "$sha" --index \
    --profile observe --format json > "$report" || true
  jq -r --arg name "$name" '.payload | [
      $name,
      ([.result.status] + [.errors[:1][].code] | join(":")),
      .summary.documents.scanned,
      .summary.documents.unsupported,
      .summary.references.resolved,
      .summary.references.unsupported,
      .summary.references.missing
    ] | @tsv' "$report" >> "$observed"
  rm -f "$report"
  echo "scanned $name" >&2
done < "$corpus"

if $update; then
  cp "$observed" "$tripwires"
  echo "wrote $tripwires" >&2
  exit 0
fi

if ! diff -u "$tripwires" "$observed"; then
  echo "the corpus moved: rerun with --update if every change above is intended" >&2
  exit 1
fi
echo "the corpus matches its tripwires" >&2
