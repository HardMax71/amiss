#!/usr/bin/env bash
set -euo pipefail

root=$(git rev-parse --show-toplevel)
experiments="$root/ci-idea/experiments"
runs=${1:-10}
tmp=$(mktemp -d /tmp/assure-runtime.XXXXXX)
cleanup() {
  node -e "require('node:fs').rmSync(process.argv[1],{recursive:true,force:true})" "$tmp"
}
trap cleanup EXIT

printf 'command\trun\texternal_wall_ms\tinternal_ms\tmax_rss_kib\toutput_bytes\n' > "$experiments/runtime-benchmark.tsv"
for run in $(seq 1 "$runs"); do
  start=$(date +%s%N)
  node "$experiments/scan-current.mjs" --out "$tmp/scan-$run.json"
  end=$(date +%s%N)
  external=$(((end - start) / 1000000))
  node -e 'const fs=require("node:fs");const r=JSON.parse(fs.readFileSync(process.argv[1]));console.log(["experiment-scanner",process.argv[2],process.argv[3],r.measurement.elapsedMilliseconds,r.measurement.maxRssKiB,fs.statSync(process.argv[1]).size].join("\t"))' "$tmp/scan-$run.json" "$run" "$external" >> "$experiments/runtime-benchmark.tsv"
done

for run in $(seq 1 "$runs"); do
  start=$(date +%s%N)
  node "$root/docs/scripts/check-links.mjs" > "$tmp/link-$run.txt"
  end=$(date +%s%N)
  external=$(((end - start) / 1000000))
  printf 'existing-link-checker\t%s\t%s\t0\t0\t%s\n' "$run" "$external" "$(wc -c < "$tmp/link-$run.txt")" >> "$experiments/runtime-benchmark.tsv"
done

node "$experiments/summarize-runtime.mjs" \
  --input "$experiments/runtime-benchmark.tsv" \
  --out "$experiments/runtime-benchmark.json"
