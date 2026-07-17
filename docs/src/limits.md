# Limits and refusals

The report has a closed set of named resource ceilings. Crossing a measured ceiling produces
a typed error carrying the wire resource name, configured limit, and observed lower bound;
a run that cannot complete exits 2. The table is rendered from the Rust defaults and checked
in CI, so a default cannot change without updating this page.

These are accounting ceilings, not all wall-clock deadlines. In particular, document bytes
are charged before parsing, but parser node and nesting totals are charged after the grammar
returns. The known in-parse CPU limitation and bootstrap-only watchdog are described in
[Security model](security.md).

Line-fragment work is charged pessimistically by the complete target size once per distinct
path and numeric range. Successful and out-of-range results are cached, so repeated identical
anchors do not multiply the charge.

<!-- amiss-doc-contract:limits:start -->
| Report resource | Limit |
| --- | ---: |
| `git-object-bytes` | 134,217,728 |
| `git-compressed-object-bytes` | 268,435,456 |
| `aggregate-git-compressed-object-bytes-per-evaluation` | 2,147,483,648 |
| `git-pack-directory-entries` | 8,192 |
| `git-pack-files` | 4,096 |
| `git-pack-index-bytes` | 536,870,912 |
| `aggregate-git-pack-index-bytes` | 1,073,741,824 |
| `git-delta-depth` | 128 |
| `git-index-bytes` | 268,435,456 |
| `git-tree-entries-per-snapshot` | 1,000,000 |
| `documents-per-snapshot` | 100,000 |
| `control-input-bytes` | 16,777,216 |
| `selected-control-blob-bytes` | 16,777,216 |
| `aggregate-selected-control-bytes-per-snapshot` | 67,108,864 |
| `repository-policy-entries` | 100,000 |
| `debt-items` | 100,000 |
| `waiver-items` | 100,000 |
| `raw-path-bytes` | 4,096 |
| `document-blob-bytes` | 4,194,304 |
| `referenced-target-blob-bytes` | 16,777,216 |
| `aggregate-referenced-target-bytes-per-snapshot` | 536,870,912 |
| `aggregate-line-fragment-evaluation-bytes-per-snapshot` | 536,870,912 |
| `aggregate-document-bytes-per-snapshot` | 536,870,912 |
| `raw-link-destination-bytes` | 16,384 |
| `parser-nesting` | 256 |
| `parser-nodes-per-document` | 250,000 |
| `parser-nodes-per-snapshot` | 5,000,000 |
| `references-per-document` | 4,096 |
| `references-per-snapshot` | 1,000,000 |
| `organization-policy-entries` | 100,000 |
| `complete-findings` | 100,000 |
| `typed-analysis-errors-retained` | 64 |
| `machine-json-bytes` | 67,108,864 |
| `private-temporary-storage-bytes` | 67,108,864 |
| `evaluator-managed-memory-bytes` | 1,073,741,824 |
<!-- amiss-doc-contract:limits:end -->

The last two rows are sandbox-descriptor values rather than ordinary scanner counters.
The CLI applies the managed-memory value as an address-space limit on Unix; the current
public lane does not independently verify that limit on every platform or establish a
provider-enforced temporary-storage sandbox. Reports therefore label this assurance
`self-asserted`, as described in [Project status](status.md). A process-level breach may
prevent a report rather than produce `RESOURCE_LIMIT_EXCEEDED`.

For measured counters, the charging rules keep every reported number reconstructible.
Counters stop exactly one
past the limit. Per-item byte limits report the declared size of the item. A snapshot-wide
total reports the running total plus the first item that crossed it, and an item already
rejected by its own per-item limit is never added to the total.

A crossing, as the report records it:

```json
{
  "code": "RESOURCE_LIMIT_EXCEEDED",
  "phase": "git",
  "resource": "raw-path-bytes",
  "configured_limit": 4096,
  "observed_lower_bound": 5008
}
```

Both numbers travel with the error, so the reader knows how far past the ceiling the input
went without rerunning anything.

Refusals follow one rule: when the input cannot be trusted, no complete pass is produced.
The machine report records the refusal and exit class 2. A base commit the store does not
hold, a tracked file whose object is missing, an
index with an unresolved merge conflict, a document whose bytes will not decode, a name
outside the path grammar, a control file with a duplicated JSON key: each has a named
error code (`GIT_OBJECT_MISSING`, `DOCUMENT_INVALID`, `UNREPRESENTABLE_PATH`, and the rest
of a closed list), and each ends the run at exit 2. A name that is merely not UTF-8 is
not on that list: it is an ordinary document whose path the report writes as hex. The
alternative in every one of these cases is a report that looks complete and is not.
