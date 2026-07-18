#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -ne 1 ]; then
  echo "usage: scripts/release-licenses.sh <output>" >&2
  exit 2
fi

repository_root="$(cd "$(dirname "$0")/.." && pwd)"
destination="$1"
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT
cd "$repository_root"

cargo metadata --locked --format-version 1 > "$scratch/metadata.json"
rust_docs="$(rustc --print sysroot)/share/doc/rust"
for target in \
  x86_64-unknown-linux-gnu \
  aarch64-apple-darwin \
  x86_64-apple-darwin \
  x86_64-pc-windows-msvc
do
  cargo tree --workspace --locked --edges normal,build --target "$target" \
    --prefix none --format '{p}'
done \
  | awk '{ version = $2; sub(/^v/, "", version); print $1 "\t" version }' \
  | sort -u > "$scratch/packages.tsv"

{
  printf '%s\n\n' 'AMISS THIRD-PARTY SOFTWARE LICENSES'
  printf '%s\n\n' 'Release binaries include the Rust standard library; its notice and license texts follow.'
  printf '%s\n' '================================================================================'
  printf '%s\n\n' 'Rust standard library'
  for rust_notice in "$rust_docs/COPYRIGHT-library.html" "$rust_docs"/licenses/*.txt; do
    printf '%s\n\n' "--- $(basename "$rust_notice") ---"
    sed -e '$a\' "$rust_notice"
  done
  printf '\n'

  while IFS=$'\t' read -r name version; do
    package="$(jq -c --arg name "$name" --arg version "$version" \
      '[.packages[] | select(.name == $name and .version == $version and .source != null)][0] // empty' \
      "$scratch/metadata.json")"
    if [ -z "$package" ]; then
      continue
    fi

    manifest="$(jq -r '.manifest_path' <<<"$package")"
    license="$(jq -r '.license // "unspecified"' <<<"$package")"
    repository="$(jq -r '.repository // "not declared"' <<<"$package")"
    mapfile -d '' license_files < <(
      find "$(dirname "$manifest")" -maxdepth 1 -type f \
        \( -iname 'license*' -o -iname 'copying*' -o -iname 'copyright*' \) \
        -print0 | sort -z
    )
    if [ "${#license_files[@]}" -eq 0 ]; then
      mapfile -t license_ids < <(
        awk '{
          gsub(/[()\/]/, " ")
          for (field = 1; field <= NF; field++) {
            if ($field != "AND" && $field != "OR" && $field != "WITH") {
              print $field
            }
          }
        }' <<<"$license" | sort -u
      )
      for license_id in "${license_ids[@]}"; do
        fallback="$rust_docs/licenses/${license_id}.txt"
        if [ ! -f "$fallback" ]; then
          echo "${name} ${version} has no license file or bundled text for ${license_id} (${license})" >&2
          exit 1
        fi
        license_files+=("$fallback")
      done
    fi

    printf '%s\n' '================================================================================'
    printf '%s %s\nLicense: %s\nSource: %s\n' "$name" "$version" "$license" "$repository"
    for license_file in "${license_files[@]}"; do
      printf '\n--- %s ---\n\n' "$(basename "$license_file")"
      sed -e '$a\' "$license_file"
    done
    printf '\n'
  done < "$scratch/packages.tsv"
} > "$scratch/licenses.txt"

mkdir -p "$(dirname "$destination")"
cp "$scratch/licenses.txt" "$destination"
