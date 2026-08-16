#!/usr/bin/env bash

set -euo pipefail
export LC_ALL=C

readonly MANIFEST_SCHEMA="amiss-similarity-edges-v1"
readonly THRESHOLD="0.85"
readonly MIN_LINES="8"
readonly ZERO_OID="0000000000000000000000000000000000000000"
readonly SCRIPT_PATH="${BASH_SOURCE[0]}"
readonly REPOSITORY_ROOT="$(cd "$(dirname "$SCRIPT_PATH")/.." && pwd)"

tool_version() {
  local version want have
  version="$(
    cd "$REPOSITORY_ROOT"
    cargo metadata --locked --format-version 1 --no-deps --offline |
      jq -er '.metadata.tools.source["similarity-rs"]'
  )"
  want="similarity-rs $version"
  have="$(similarity-rs --version)"
  if [[ "$have" != "$want" ]]; then
    echo "the similarity gate pins $want, found $have" >&2
    return 1
  fi
  printf '%s\n' "$have"
}

policy_identity() {
  local tool script_blob
  tool="$(tool_version)"
  script_blob="$(git -C "$REPOSITORY_ROOT" hash-object "$SCRIPT_PATH")"
  printf '%s\0%s\0%s\0%s\0%s\n' \
    "$MANIFEST_SCHEMA" "$tool" "$THRESHOLD" "$MIN_LINES" "$script_blob" |
    git -C "$REPOSITORY_ROOT" hash-object --stdin
}

if [[ "${1:-}" == "--policy" ]]; then
  policy_identity
  exit 0
fi

if [[ $# -ne 0 ]]; then
  echo "usage: scripts/similarity-gate.sh [--policy]" >&2
  exit 2
fi

readonly TOOL_VERSION="$(tool_version)"
readonly POLICY="${AMISS_SIMILARITY_POLICY:-$(policy_identity)}"
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

tree_of() {
  local revision="$1"
  if [[ "$revision" == "$ZERO_OID" ]]; then
    printf '%s\n' empty
    return
  fi
  git -C "$REPOSITORY_ROOT" rev-parse --verify "$revision^{tree}"
}

validate_manifest() {
  local manifest="$1" tree="$2"
  [[ -f "$manifest" ]] || return 1
  [[ ! -L "$manifest" ]] || return 1
  [[ "$(wc -c < "$manifest")" -le 5242880 ]] || return 1
  awk -F '\t' -v schema="$MANIFEST_SCHEMA" -v policy="$POLICY" \
    -v tree="$tree" -v tool="$TOOL_VERSION" '
      function endpoint(value, fields, count) {
        count = split(value, fields, /\|/)
        return count == 6 && (fields[1] == "same" || fields[1] == "provider") &&
          fields[2] ~ /^(crates|controller)\// && fields[4] ~ /^(function|method)$/ &&
          fields[5] ~ /^[A-Za-z_][A-Za-z0-9_]*$/ && fields[6] ~ /^occurrence=[1-9][0-9]*$/
      }
      NR == 1 { if ($0 != schema) exit 1; next }
      NR == 2 { if (NF != 2 || $1 != "policy" || $2 != policy) exit 1; next }
      NR == 3 { if (NF != 2 || $1 != "tree" || $2 != tree) exit 1; next }
      NR == 4 { if (NF != 2 || $1 != "tool" || $2 != tool) exit 1; next }
      {
        if (NF != 3 || $1 != "edge" || !endpoint($2) || !endpoint($3) ||
            $2 > $3) exit 1
        row = $2 "\t" $3
        if (seen && row <= previous) exit 1
        previous = row
        seen = 1
      }
      END { if (NR < 4) exit 1 }
    ' "$manifest"
}

write_manifest() {
  local tree="$1" edges="$2" destination="$3" temporary
  mkdir -p "$(dirname "$destination")"
  temporary="$WORK_DIR/manifest.$RANDOM"
  {
    printf '%s\n' "$MANIFEST_SCHEMA"
    printf 'policy\t%s\n' "$POLICY"
    printf 'tree\t%s\n' "$tree"
    printf 'tool\t%s\n' "$TOOL_VERSION"
    sed 's/^/edge\t/' "$edges"
  } > "$temporary"
  mv "$temporary" "$destination"
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
        if (path[side] !~ /^(crates|controller)\//) {
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

scan_root() {
  local root="$1" tree="$2" destination="$3" required_sources="$4" source_aliases="$5"
  local stage="$WORK_DIR/stage.$RANDOM" raw_same="$WORK_DIR/same.$RANDOM"
  local raw_provider="$WORK_DIR/provider.$RANDOM" maps="$WORK_DIR/maps.$RANDOM"
  local provider_edges="$WORK_DIR/provider-edges.$RANDOM"
  local edges="$WORK_DIR/edges.$RANDOM" sorted="$WORK_DIR/sorted.$RANDOM"
  mkdir -p "$stage"
  : > "$maps"

  (cd "$root" && similarity-rs crates controller --threshold "$THRESHOLD" \
    --min-lines "$MIN_LINES") > "$raw_same"
  canonicalize same "$raw_same" "$maps" "$edges"

  : > "$stage/transports.rs"
  append_provider_source "$root" "$stage/transports.rs" "$maps" "$source_aliases" \
    controller/gitea/src/live/rest/transport.rs "$required_sources"
  append_provider_source "$root" "$stage/transports.rs" "$maps" "$source_aliases" \
    controller/github/src/live/rest/transport.rs "$required_sources"
  append_provider_source "$root" "$stage/transports.rs" "$maps" "$source_aliases" \
    controller/gitlab/src/live/transport.rs "$required_sources"

  : > "$stage/harnesses.rs"
  append_provider_source "$root" "$stage/harnesses.rs" "$maps" "$source_aliases" \
    controller/gitea-service/tests/lane/harness.rs "$required_sources"
  append_provider_source "$root" "$stage/harnesses.rs" "$maps" "$source_aliases" \
    controller/github-service/tests/lane/harness.rs "$required_sources"
  append_provider_source "$root" "$stage/harnesses.rs" "$maps" "$source_aliases" \
    controller/gitlab-service/tests/lane/harness.rs "$required_sources"

  : > "$stage/runtimes.rs"
  append_provider_source "$root" "$stage/runtimes.rs" "$maps" "$source_aliases" \
    controller/gitea-service/src/runtime.rs "$required_sources"
  append_provider_source "$root" "$stage/runtimes.rs" "$maps" "$source_aliases" \
    controller/github-service/src/runtime.rs "$required_sources"
  append_provider_source "$root" "$stage/runtimes.rs" "$maps" "$source_aliases" \
    controller/gitlab-service/src/runtime.rs "$required_sources"

  : > "$stage/verifies.rs"
  append_provider_source "$root" "$stage/verifies.rs" "$maps" "$source_aliases" \
    controller/gitea/src/live/verify.rs "$required_sources"
  append_provider_source "$root" "$stage/verifies.rs" "$maps" "$source_aliases" \
    controller/github/src/live/verify.rs "$required_sources"
  append_provider_source "$root" "$stage/verifies.rs" "$maps" "$source_aliases" \
    controller/gitlab/src/live/verify.rs "$required_sources"

  (cd "$stage" && similarity-rs . --threshold "$THRESHOLD" \
    --min-lines "$MIN_LINES") > "$raw_provider"
  canonicalize provider "$raw_provider" "$maps" "$provider_edges"
  cat "$provider_edges" >> "$edges"

  LC_ALL=C sort "$edges" > "$sorted"
  if [[ -n "$(uniq -d "$sorted")" ]]; then
    echo "similarity canonicalization produced duplicate edge identities" >&2
    uniq -d "$sorted" >&2
    return 1
  fi
  write_manifest "$tree" "$sorted" "$destination"
}

build_renames() {
  local revision="$1" destination="$2" status first second
  : > "$destination"
  if [[ "$revision" == "$ZERO_OID" ]]; then
    return
  fi
  while IFS= read -r -d '' status; do
    IFS= read -r -d '' first || {
      echo "truncated git rename record" >&2
      return 1
    }
    case "$status" in
      R*|C*)
        IFS= read -r -d '' second || {
          echo "truncated git rename record" >&2
          return 1
        }
        if [[ "$status" == R* ]]; then
          if [[ "$first" == *$'\t'* || "$first" == *$'\n'* ||
                "$second" == *$'\t'* || "$second" == *$'\n'* ]]; then
            echo "a renamed Rust path contains a manifest delimiter" >&2
            return 1
          fi
          printf '%s\t%s\n' "$second" "$first" >> "$destination"
        fi
        ;;
    esac
  done < <(git -C "$REPOSITORY_ROOT" diff --name-status -z -M "$revision" -- crates controller)
  LC_ALL=C sort -o "$destination" "$destination"
}

manifest_edges() {
  local manifest="$1" aliases="$2" output="$3" unsorted="$WORK_DIR/edges.$RANDOM"
  awk -F '\t' -v aliases="$aliases" '
    function endpoint(value, fields, count, result, part_index) {
      count = split(value, fields, /\|/)
      if (count != 6) {
        print "invalid endpoint in validated similarity manifest" > "/dev/stderr"
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
    $1 == "edge" {
      left = endpoint($2)
      right = endpoint($3)
      if (right < left) {
        temporary = left
        left = right
        right = temporary
      }
      print left "\t" right
    }
    END { if (bad) exit 1 }
  ' "$manifest" > "$unsorted"
  LC_ALL=C sort "$unsorted" > "$output"
  if [[ -n "$(uniq -d "$output")" ]]; then
    echo "similarity rename normalization collapsed distinct edges" >&2
    uniq -d "$output" >&2
    return 1
  fi
}

base_revision="$(resolve_base)"
base_tree="$(tree_of "$base_revision")"
current_tree="${AMISS_SIMILARITY_CURRENT_TREE:-worktree}"
base_manifest="$WORK_DIR/base.manifest"
current_manifest="${AMISS_SIMILARITY_CURRENT_MANIFEST:-$WORK_DIR/current.manifest}"
renames="$WORK_DIR/renames"
no_aliases="$WORK_DIR/no-aliases"
build_renames "$base_revision" "$renames"
: > "$no_aliases"

cache_root="${AMISS_SIMILARITY_LOCAL_CACHE:-$(git -C "$REPOSITORY_ROOT" rev-parse --git-common-dir)/amiss/similarity}"
if [[ "$cache_root" != /* ]]; then
  cache_root="$REPOSITORY_ROOT/$cache_root"
fi
local_manifest="$cache_root/$POLICY-$base_tree.manifest"
external_manifest="${AMISS_SIMILARITY_BASE_MANIFEST:-}"

if [[ -n "$external_manifest" ]] && validate_manifest "$external_manifest" "$base_tree"; then
  cp "$external_manifest" "$base_manifest"
elif [[ ! -s "$renames" ]] && validate_manifest "$local_manifest" "$base_tree"; then
  cp "$local_manifest" "$base_manifest"
elif [[ "$base_revision" == "$ZERO_OID" ]]; then
  :
else
  if ! git -C "$REPOSITORY_ROOT" cat-file -e "$base_revision^{commit}"; then
    echo "similarity base commit is unavailable: $base_revision" >&2
    exit 1
  fi
  mkdir -p "$WORK_DIR/base"
  git -C "$REPOSITORY_ROOT" archive "$base_revision" crates controller |
    tar -x -C "$WORK_DIR/base"
  scan_root "$WORK_DIR/base" "$base_tree" "$base_manifest" 0 "$renames"
  if [[ ! -s "$renames" ]]; then
    mkdir -p "$cache_root"
    local_temporary="$(mktemp "$cache_root/manifest.XXXXXX")"
    cp "$base_manifest" "$local_temporary"
    mv "$local_temporary" "$local_manifest"
  fi
fi

scan_root "$REPOSITORY_ROOT" "$current_tree" "$current_manifest" 1 "$no_aliases"
if [[ "$base_revision" == "$ZERO_OID" ]]; then
  awk -F '\t' -v tree=empty '
    NR == 3 { print "tree\t" tree; next }
    { print }
  ' "$current_manifest" > "$base_manifest"
fi

base_edges="$WORK_DIR/base.edges"
current_edges="$WORK_DIR/current.edges"
new_edges="$WORK_DIR/new.edges"
removed_edges="$WORK_DIR/removed.edges"
manifest_edges "$base_manifest" "$no_aliases" "$base_edges"
manifest_edges "$current_manifest" "$renames" "$current_edges"
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
printf 'near-twin edges: %s base, %s candidate, %s added, %s removed\n' \
  "$base_count" "$current_count" "$new_count" "$removed_count"
