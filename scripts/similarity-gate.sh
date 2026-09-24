#!/usr/bin/env bash

set -euo pipefail
export LC_ALL=C
export GIT_LITERAL_PATHSPECS=1

readonly THRESHOLD="0.85"
readonly MIN_LINES="8"
readonly ZERO_OID="0000000000000000000000000000000000000000"
readonly SCRIPT_PATH="${BASH_SOURCE[0]}"
readonly REPOSITORY_ROOT="$(cd "$(dirname "$SCRIPT_PATH")/.." && pwd)"
readonly -a PROVIDER_SOURCES=(
  transports:controller/gitea/src/live/rest/transport.rs
  transports:controller/github/src/live/rest/transport.rs
  transports:controller/gitlab/src/live/transport.rs
  harnesses:controller/gitea-service/tests/lane/harness.rs
  harnesses:controller/github-service/tests/lane/harness.rs
  harnesses:controller/gitlab-service/tests/lane/harness.rs
  runtimes:controller/gitea-service/src/runtime.rs
  runtimes:controller/github-service/src/runtime.rs
  runtimes:controller/gitlab-service/src/runtime.rs
  verifies:controller/gitea/src/live/verify.rs
  verifies:controller/github/src/live/verify.rs
  verifies:controller/gitlab/src/live/verify.rs
)

if [[ $# -ne 0 ]]; then
  echo "usage: scripts/similarity-gate.sh" >&2
  exit 2
fi

version="$(
  cd "$REPOSITORY_ROOT"
  cargo metadata --locked --format-version 1 --no-deps --offline |
    jq -er '.metadata.tools.source["similarity-rs"]'
)"
if [[ "$(similarity-rs --version)" != "similarity-rs $version" ]]; then
  echo "the similarity gate pins similarity-rs $version, found $(similarity-rs --version)" >&2
  exit 1
fi

readonly WORK_DIR="$(mktemp -d)"
trap 'rm -rf "$WORK_DIR"' EXIT

resolve_base() {
  local candidate
  if [[ -n "${AMISS_SIMILARITY_BASE:-}" ]]; then
    printf '%s\n' "$AMISS_SIMILARITY_BASE"
    return
  fi
  for candidate in refs/remotes/origin/main refs/remotes/origin/HEAD '@{upstream}'; do
    if git -C "$REPOSITORY_ROOT" rev-parse --verify --quiet "$candidate^{commit}" >/dev/null; then
      git -C "$REPOSITORY_ROOT" merge-base HEAD "$candidate"
      return
    fi
  done
  if git -C "$REPOSITORY_ROOT" rev-parse --verify --quiet HEAD^ >/dev/null; then
    git -C "$REPOSITORY_ROOT" rev-parse HEAD^
    return
  fi
  printf '%s\n' "$ZERO_OID"
}

canonicalize() {
  local scope="$1" raw="$2" maps="$3" output="$4"
  awk -v scope="$scope" -v maps="$maps" '
    function fail(message) {
      print "similarity output parse error: " message > "/dev/stderr"
      bad = 1
    }
    function trim(value) {
      sub(/^[[:space:]]+/, "", value)
      sub(/[[:space:]]+$/, "", value)
      return value
    }
    function endpoint(value, side, fields, count, marker, span) {
      value = trim(value)
      count = split(value, fields, /[[:space:]]+/)
      if (count != 3 || (fields[2] != "function" && fields[2] != "method")) {
        fail("unrecognized function endpoint: " value)
        return
      }
      marker = match(fields[1], /:[0-9]+-[0-9]+$/)
      if (!marker) {
        fail("unrecognized source span: " fields[1])
        return
      }
      raw_path[side] = substr(fields[1], 1, RSTART - 1)
      span = substr(fields[1], RSTART + 1)
      split(span, range, /-/)
      start_line[side] = range[1] + 0
      end_line[side] = range[2] + 0
      kind[side] = fields[2]
      name[side] = fields[3]
      owner[side] = ""
    }
    function mapped_path(side, group, idx, key) {
      path[side] = ""
      if (scope == "same") {
        path[side] = raw_path[side]
        sub(/^\.\//, "", path[side])
        local_start[side] = start_line[side]
        local_end[side] = end_line[side]
        if (path[side] !~ /^(api|crates|controller)\//) {
          fail("source escaped the scan roots: " raw_path[side])
        }
      } else {
        group = raw_path[side]
        sub(/^\.\//, "", group)
        for (idx = 1; idx <= map_count[group]; idx++) {
          key = group SUBSEP idx
          if (start_line[side] >= map_start[key] && end_line[side] <= map_end[key]) {
            path[side] = map_path[key]
            local_start[side] = start_line[side] - map_start[key] + 1
            local_end[side] = end_line[side] - map_start[key] + 1
            break
          }
        }
        if (path[side] == "") {
          fail("generated span has no source mapping: " raw_path[side] ":" start_line[side])
        }
      }
    }
    function emit(side, locator, span, span_key, start_key) {
      if (!pending) return
      if (!score) fail("pair has no similarity score")
      for (side = 1; side <= 2; side++) {
        mapped_path(side)
        if (path[side] ~ /[|\t]/ || owner[side] ~ /[|\t]/ || name[side] ~ /[|\t]/) {
          fail("identity contains a reserved delimiter")
        }
        locator = scope "|" path[side] "|" owner[side] "|" kind[side] "|" name[side]
        span = path[side] ":" local_start[side] "-" local_end[side]
        span_key = locator SUBSEP span
        start_key = locator SUBSEP local_start[side]
        if (start_key in start_span && start_span[start_key] != span) {
          fail("ambiguous identity start " locator " at " span)
        }
        start_span[start_key] = span
        span_start[span_key] = local_start[side]
        pair_locator[parsed + 1, side] = locator
        pair_span[parsed + 1, side] = span
      }
      parsed++
      if (scope == "provider" && path[1] == path[2]) {
        pair_included[parsed] = 0
      } else {
        pair_included[parsed] = 1
      }
      pending = 0
      score = 0
      classes_seen = 0
    }
    function identity(locator, span, entry, parts, rank, start) {
      rank = 1
      start = span_start[locator SUBSEP span]
      for (entry in span_start) {
        split(entry, parts, SUBSEP)
        if (parts[1] == locator &&
            (span_start[entry] < start || (span_start[entry] == start && parts[2] < span))) {
          rank++
        }
      }
      return locator "|occurrence=" rank
    }
    BEGIN {
      if (scope == "provider") {
        while ((getline map_line < maps) > 0) {
          count = split(map_line, item, /\t/)
          if (count != 4) {
            fail("invalid provider source map")
            continue
          }
          map_count[item[1]]++
          key = item[1] SUBSEP map_count[item[1]]
          map_start[key] = item[2] + 0
          map_end[key] = item[3] + 0
          map_path[key] = item[4]
        }
        close(maps)
      }
    }
    /^  .*:[0-9]+-[0-9]+ (function|method) [^ ]+ <-> .*:[0-9]+-[0-9]+ (function|method) [^ ]+$/ {
      emit()
      line = trim($0)
      count = split(line, pair, / <-> /)
      if (count != 2) {
        fail("pair separator is ambiguous")
        next
      }
      endpoint(pair[1], 1)
      endpoint(pair[2], 2)
      pending = 1
      next
    }
    /^  Similarity: [0-9]+([.][0-9]+)?%$/ {
      if (!pending || score) fail("misplaced similarity score")
      score = 1
      next
    }
    /^  Classes: / {
      if (!pending || !score || classes_seen) {
        fail("misplaced class metadata")
        next
      }
      line = $0
      sub(/^  Classes: /, "", line)
      count = split(line, classes, / <-> /)
      if (count != 2) {
        fail("class separator is ambiguous")
        next
      }
      owner[1] = trim(classes[1])
      owner[2] = trim(classes[2])
      classes_seen = 1
      next
    }
    /^Total duplicate pairs found: [0-9]+$/ {
      emit()
      if (total_seen) fail("duplicate total")
      expected = $0
      sub(/^Total duplicate pairs found: /, "", expected)
      total_seen = 1
      next
    }
    /^No duplicate functions found!$/ {
      emit()
      if (total_seen) fail("duplicate total")
      expected = 0
      total_seen = 1
      next
    }
    /^$/ || /^Analyzing Rust code similarity\.\.\.$/ || /^=== Function Similarity ===$/ ||
      /^Checking [0-9]+ files for duplicates\.\.\.$/ || /^Duplicates in .*:$/ || /^-+$/ { next }
    { fail("unrecognized line: " $0) }
    END {
      emit()
      if (!total_seen) fail("missing duplicate total")
      if (parsed != expected) fail("parsed " parsed " pairs, tool reported " expected)
      if (bad) exit 1
      for (pair_index = 1; pair_index <= parsed; pair_index++) {
        if (!pair_included[pair_index]) continue
        left = identity(pair_locator[pair_index, 1], pair_span[pair_index, 1])
        right = identity(pair_locator[pair_index, 2], pair_span[pair_index, 2])
        if (right < left) {
          temporary = left
          left = right
          right = temporary
        }
        print left "\t" right
      }
    }
  ' "$raw" > "$output"
}

append_provider_source() {
  local root="$1" generated="$2" maps="$3" aliases="$4" relative="$5" required="$6"
  local source="$relative" start lines end group aliased
  if [[ ! -f "$root/$source" ]]; then
    aliased="$(awk -F '\t' -v path="$relative" '$1 == path { print $2; exit }' "$aliases")"
    if [[ -n "$aliased" ]] && [[ -f "$root/$aliased" ]]; then
      source="$aliased"
    fi
  fi
  if [[ ! -f "$root/$source" ]]; then
    if [[ "$required" == 1 ]]; then
      echo "provider similarity source is missing: $relative" >&2
      return 1
    fi
    return
  fi
  group="$(basename "$generated")"
  start="$(( $(wc -l < "$generated") + 1 ))"
  lines="$(wc -l < "$root/$source")"
  cat "$root/$source" >> "$generated"
  printf '\n' >> "$generated"
  end="$((start + lines - 1))"
  printf '%s\t%s\t%s\t%s\n' "$group" "$start" "$end" "$source" >> "$maps"
}

scan_files() {
  local root="$1" output="$2" raw="$2.raw" path
  local -a files=()
  shift 2
  for path in "$@"; do
    if [[ -f "$root/$path" ]]; then
      files+=("$path")
    fi
  done
  : > "$output"
  if (( ${#files[@]} == 0 )); then
    return
  fi
  (cd "$root" && similarity-rs "${files[@]}" --threshold "$THRESHOLD" \
    --min-lines "$MIN_LINES") > "$raw"
  canonicalize same "$raw" /dev/null "$output"
}

scan_providers() {
  local root="$1" output="$2" required="$3" aliases="$4" entry
  local stage="$2.stage" raw="$2.raw" maps="$2.maps"
  mkdir -p "$stage"
  : > "$maps"
  for entry in "${PROVIDER_SOURCES[@]}"; do
    : > "$stage/${entry%%:*}.rs"
  done
  for entry in "${PROVIDER_SOURCES[@]}"; do
    append_provider_source "$root" "$stage/${entry%%:*}.rs" "$maps" "$aliases" \
      "${entry#*:}" "$required"
  done
  (cd "$stage" && similarity-rs . --threshold "$THRESHOLD" \
    --min-lines "$MIN_LINES") > "$raw"
  canonicalize provider "$raw" "$maps" "$output"
}

edge_set() {
  local aliases="$1" output="$2" unsorted="$2.unsorted"
  shift 2
  cat "$@" | awk -F '\t' -v aliases="$aliases" '
    function endpoint(value, fields, count, result, part_index) {
      count = split(value, fields, /\|/)
      if (count != 6) {
        print "invalid similarity endpoint: " value > "/dev/stderr"
        bad = 1
        return value
      }
      if (fields[2] in alias_path) fields[2] = alias_path[fields[2]]
      result = fields[1]
      for (part_index = 2; part_index <= count; part_index++) {
        result = result "|" fields[part_index]
      }
      return result
    }
    BEGIN {
      while ((getline alias_line < aliases) > 0) {
        count = split(alias_line, item, /\t/)
        if (count != 2 || item[1] == "" || item[2] == "" || item[1] in alias_path) {
          print "invalid or duplicate rename mapping" > "/dev/stderr"
          bad = 1
          continue
        }
        alias_path[item[1]] = item[2]
      }
      close(aliases)
    }
    {
      left = endpoint($1)
      right = endpoint($2)
      if (right < left) {
        temporary = left
        left = right
        right = temporary
      }
      print left "\t" right
    }
    END { if (bad) exit 1 }
  ' > "$unsorted"
  LC_ALL=C sort "$unsorted" > "$output"
  if [[ -n "$(uniq -d "$output")" ]]; then
    echo "similarity canonicalization produced duplicate edge identities" >&2
    uniq -d "$output" >&2
    return 1
  fi
}

base_revision="$(resolve_base)"
if [[ "$base_revision" == "$ZERO_OID" ]]; then
  echo "near-twin edges: no base commit to compare with"
  exit 0
fi
if ! git -C "$REPOSITORY_ROOT" cat-file -e "$base_revision^{commit}"; then
  echo "similarity base commit is unavailable: $base_revision" >&2
  exit 1
fi

# The tool compares functions within one file, so an unchanged file keeps its edges.
renames="$WORK_DIR/renames"
no_aliases="$WORK_DIR/no-aliases"
: > "$renames"
: > "$no_aliases"
base_files=()
current_files=()
while IFS= read -r -d '' status; do
  IFS= read -r -d '' first || {
    echo "truncated git diff record" >&2
    exit 1
  }
  second="$first"
  if [[ "$status" == R* ]]; then
    IFS= read -r -d '' second || {
      echo "truncated git diff record" >&2
      exit 1
    }
    if [[ "$first" == *$'\t'* || "$first" == *$'\n'* ||
          "$second" == *$'\t'* || "$second" == *$'\n'* ]]; then
      echo "a renamed Rust path contains a manifest delimiter" >&2
      exit 1
    fi
    printf '%s\t%s\n' "$second" "$first" >> "$renames"
  fi
  if [[ "$status" != A && "$first" == *.rs ]]; then
    base_files+=("$first")
  fi
  if [[ "$status" != D && "$second" == *.rs ]]; then
    current_files+=("$second")
  fi
done < <(git -C "$REPOSITORY_ROOT" diff --name-status -z -M "$base_revision" -- api crates controller)
while IFS= read -r -d '' path; do
  if [[ "$path" == *.rs ]]; then
    current_files+=("$path")
  fi
done < <(git -C "$REPOSITORY_ROOT" ls-files -z --others --exclude-standard -- api crates controller)

providers_changed=0
archive_paths=("${base_files[@]}")
for entry in "${PROVIDER_SOURCES[@]}"; do
  for path in "${base_files[@]}" "${current_files[@]}"; do
    if [[ "$path" == "${entry#*:}" ]]; then
      providers_changed=1
    fi
  done
done
if (( providers_changed )); then
  for entry in "${PROVIDER_SOURCES[@]}"; do
    archive_paths+=("${entry#*:}")
    aliased="$(awk -F '\t' -v path="${entry#*:}" '$1 == path { print $2; exit }' "$renames")"
    if [[ -n "$aliased" ]]; then
      archive_paths+=("$aliased")
    fi
  done
fi

mkdir -p "$WORK_DIR/base"
base_present=()
if (( ${#archive_paths[@]} )); then
  while IFS= read -r -d '' path; do
    base_present+=("$path")
  done < <(git -C "$REPOSITORY_ROOT" ls-tree -r -z --name-only "$base_revision" -- "${archive_paths[@]}")
fi
if (( ${#base_present[@]} )); then
  git -C "$REPOSITORY_ROOT" archive "$base_revision" -- "${base_present[@]}" |
    tar -x -C "$WORK_DIR/base"
fi

# One file scans on one core, so the base side runs beside the candidate side.
scan_files "$WORK_DIR/base" "$WORK_DIR/base.same" "${base_files[@]}" &
base_scan=$!
scan_files "$REPOSITORY_ROOT" "$WORK_DIR/current.same" "${current_files[@]}"
wait "$base_scan"
: > "$WORK_DIR/base.provider"
: > "$WORK_DIR/current.provider"
if (( providers_changed )); then
  scan_providers "$WORK_DIR/base" "$WORK_DIR/base.provider" 0 "$renames" &
  base_scan=$!
  scan_providers "$REPOSITORY_ROOT" "$WORK_DIR/current.provider" 1 "$no_aliases"
  wait "$base_scan"
fi

base_edges="$WORK_DIR/base.edges"
current_edges="$WORK_DIR/current.edges"
new_edges="$WORK_DIR/new.edges"
removed_edges="$WORK_DIR/removed.edges"
edge_set "$no_aliases" "$base_edges" "$WORK_DIR/base.same" "$WORK_DIR/base.provider"
edge_set "$renames" "$current_edges" "$WORK_DIR/current.same" "$WORK_DIR/current.provider"
comm -13 "$base_edges" "$current_edges" > "$new_edges"
comm -23 "$base_edges" "$current_edges" > "$removed_edges"

new_count="$(wc -l < "$new_edges")"
removed_count="$(wc -l < "$removed_edges")"
base_count="$(wc -l < "$base_edges")"
current_count="$(wc -l < "$current_edges")"
if [[ "$new_count" -ne 0 ]]; then
  echo "the candidate introduces near-twin function edges" >&2
  printf 'base: %s, candidate: %s, added: %s, removed: %s\n' \
    "$base_count" "$current_count" "$new_count" "$removed_count" >&2
  sed 's/\t/ <-> /' "$new_edges" >&2
  echo "base commit: $base_revision" >&2
  exit 1
fi
printf 'near-twin edges in the changed files: %s base, %s candidate, %s added, %s removed\n' \
  "$base_count" "$current_count" "$new_count" "$removed_count"
