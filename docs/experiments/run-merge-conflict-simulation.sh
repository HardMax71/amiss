#!/usr/bin/env bash
set -euo pipefail

root=$(git rev-parse --show-toplevel)
experiments="$root/ci-idea/experiments"
trials=${1:-50}
tmp=$(mktemp -d /tmp/assure-merge-simulation.XXXXXX)
cleanup() {
  node -e "require('node:fs').rmSync(process.argv[1],{recursive:true,force:true})" "$tmp"
}
trap cleanup EXIT

node "$experiments/generate-merge-trials.mjs" --dir "$tmp" --trials "$trials"
printf 'updates_per_branch\ttrial\tresult\n' > "$experiments/merge-conflict-results.tsv"
tail -n +2 "$tmp/manifest.tsv" | while IFS=$'\t' read -r updates trial a base b; do
  if git merge-file -p --diff3 "$a" "$base" "$b" >/dev/null; then
    result=clean
  else
    status=$?
    if [[ $status -eq 255 ]]; then
      exit "$status"
    fi
    result=conflict
  fi
  printf '%s\t%s\t%s\n' "$updates" "$trial" "$result" >> "$experiments/merge-conflict-results.tsv"
done

printf 'updates_per_branch\tresult\n' > "$experiments/merge-conflict-sharded-results.tsv"
for updates in 1 5 20; do
  repo="$tmp/sharded-$updates"
  git -C "$repo" init -q
  git -C "$repo" config user.name 'Assure Simulation'
  git -C "$repo" config user.email 'simulation.invalid@example.invalid'
  git -C "$repo" add claims
  git -C "$repo" commit -qm base
  base=$(git -C "$repo" rev-parse HEAD)
  git -C "$repo" switch -qc branch-a
  cp -a "$repo/updates-a/." "$repo/claims/"
  git -C "$repo" add claims
  git -C "$repo" commit -qm branch-a
  git -C "$repo" switch -qc branch-b "$base"
  cp -a "$repo/updates-b/." "$repo/claims/"
  git -C "$repo" add claims
  git -C "$repo" commit -qm branch-b
  if git -C "$repo" merge --no-edit branch-a >/dev/null 2>&1; then
    result=clean
  else
    result=conflict
  fi
  printf '%s\t%s\n' "$updates" "$result" >> "$experiments/merge-conflict-sharded-results.tsv"
done

cp "$tmp/metadata.json" "$experiments/merge-conflict-workload.json"
node "$experiments/summarize-merge-trials.mjs" \
  --metadata "$tmp/metadata.json" \
  --single-results "$experiments/merge-conflict-results.tsv" \
  --sharded-results "$experiments/merge-conflict-sharded-results.tsv" \
  --out "$experiments/merge-conflict-simulation.json"
