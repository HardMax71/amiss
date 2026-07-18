# Limits and refusals

The report has a closed set of named resource ceilings. Crossing a measured ceiling produces
a typed error carrying the wire resource name, configured limit, and observed lower bound;
a run that cannot complete exits 2. The table is rendered from the Rust defaults and checked
in CI, so a default cannot change without updating this page.

These are accounting ceilings, not all wall-clock deadlines. In particular, document bytes
are charged before parsing, but parser node and nesting totals are charged after the grammar
returns. The known in-parse CPU limitation and bootstrap-only watchdog are described in
[Security model](security.md).

Line-fragment work is charged pessimistically: the complete target size, once per
distinct target identity (path, file mode, and object id) and numeric range. Successful
and out-of-range results are cached, so repeated identical anchors do not multiply the
charge. A changed object or mode at the same path is charged again.

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

The closed list, one fixed sentence per code, generated from
[`AnalysisErrorCode::meaning`](https://github.com/HardMax71/amiss/blob/main/crates/amiss-wire/src/report.rs) and checked in CI.
The human output prints the same sentence as a `note` line whenever a code appears, so an
exit-2 log says how to unblock the run without this page open.

<!-- amiss-doc-contract:error-meanings:start -->
- `INVALID_INVOCATION`: the command line does not match the closed grammar; each documented option appears at most once and nothing else is accepted
- `INVALID_EVENT`: the declared repository, ref, or default-branch identity is not in canonical form; pass a lowercase owner and name and full refs/heads/ references
- `INVALID_PROFILE`: the profile is not one of observe or enforce
- `REQUEST_UNREADABLE`: the machine evaluation request bytes could not be read; nothing was evaluated
- `CONFIGURATION_INVALID`: a policy or control input violates its schema; one unknown field or malformed value makes the whole file invalid rather than partly honored
- `DUPLICATE_JSON_KEY`: a JSON input repeats an object key; strict parsing refuses the file instead of choosing one of the values
- `INVALID_UTF8`: a JSON input carries bytes that are not UTF-8
- `INVALID_JSON`: an input that must be JSON does not parse as strict JSON
- `UNKNOWN_SCHEMA`: a JSON input declares a schema identifier this engine does not recognize
- `UNKNOWN_FIELD`: a JSON input carries a field its closed schema does not define; unknown fields refuse rather than pass through unread
- `NONCANONICAL_ARRAY`: a JSON input array violates its required canonical ordering or uniqueness
- `DIGEST_MISMATCH`: a digest carried by an input does not match the bytes it names; the input is stale or altered
- `CONTROL_BINDING_MISMATCH`: an external control is bound to a different repository, ref, or run identity than this evaluation; nothing is applied and the run ends incomplete
- `EXCEPTION_OVERLAP`: accepted exception items select the same finding more than once; overlap ends evaluation incomplete instead of double-suppressing
- `UNSUPPORTED_CAPABILITY`: a candidate document declares a reserved amiss: capability this engine does not implement; the run ends incomplete rather than guessing at the claim
- `GIT_REPOSITORY_UNAVAILABLE`: the --repo path does not open as a Git repository of the declared object format
- `GIT_OBJECT_MISSING`: a commit, tree, or blob the run needs is absent from the object store; fetch full history or name commits the store holds
- `GIT_OBJECT_WRONG_KIND`: a Git object is not the kind its use requires, as when a named commit resolves to another type
- `GIT_OBJECT_UNREADABLE`: a Git object exists but its bytes cannot be decoded
- `GIT_INDEX_INVALID`: the staged index file does not parse under the index grammar
- `GIT_INDEX_UNMERGED`: the index holds unmerged conflict entries, so no single staged state exists; finish or abort the merge before checking the index
- `GIT_INTENT_TO_ADD`: the index holds an intent-to-add entry whose content is not staged; stage the file or drop the intent entry before checking the index
- `GIT_SNAPSHOT_CHANGED`: the staged index changed while the run was reading it; rerun when the repository is quiet
- `UNREPRESENTABLE_PATH`: a tree or index name is outside the path grammar, a backslash, a NUL, or a dot segment; the exact bytes are disclosed as hex
- `DOCUMENT_INVALID`: a discovered document's bytes cannot be decoded as its format requires; the run refuses instead of skipping the file and passing
- `PARSER_ERROR`: the pinned parser failed on a document; the document is named and the run is incomplete rather than the file silently dropped
- `PARSER_PANIC`: the pinned parser panicked on a document; the panic is caught and reported, and the run is incomplete
- `INVALID_SOURCE_SPAN`: the parser returned a node whose byte span does not address the document; the parse is not trusted
- `RESOLUTION_ERROR`: reference resolution failed internally; the run ends incomplete rather than reporting around the gap
- `RESOURCE_LIMIT_EXCEEDED`: a named resource crossed its ceiling; the row carries the resource, the configured limit, and the observed lower bound
- `OUTPUT_LIMIT_EXCEEDED`: the serialized report would cross the machine-json-bytes ceiling; the run ends incomplete instead of shortening the findings
- `TOO_MANY_ERRORS`: more distinct analysis errors accumulated than the retention ceiling; the lowest-keyed rows are kept and this sentinel stands for the rest
- `REPORT_CONSTRUCTION_FAILED`: the report could not be constructed or emitted; the run has no trustworthy output
- `SANDBOX_VIOLATION`: the run breached its sandbox descriptor; the result is not trustworthy
- `TRUSTED_TIME_INVALID`: a control that needs trusted time has no statement that verifies, absent or failing its binding; the run will not act on an unverified clock
- `INTERNAL_ERROR`: an engine invariant failed; this is a defect in Amiss, not in the input, and the run has no trustworthy result
<!-- amiss-doc-contract:error-meanings:end -->
