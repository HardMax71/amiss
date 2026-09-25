# Analysis errors

An analysis error is not a finding. A finding is something the scan established about the
repository. An error means the scan itself could not be trusted, so the run ends at exit 2
with no verdict, and the machine report arrives marked incomplete with one row per retained
error.

Every row names its code, the phase it came from (invocation, configuration, git, discovery,
parse, resolution, policy, output, or internal), and the fixed sentence below. A row that
knows a file carries its path, or the raw bytes as hex when the name is outside the path
grammar, and a ceiling crossing carries the resource, the configured limit, and the observed
lower bound. [The report](report.md) has the exact shape. The human output prints the same
sentence as a `note` line once per code, so an exit-2 log says what to do without this page
open.

Most codes name something to change: a policy file, the command line, the index, a control
bound to another repository. Seven point at Amiss instead. `INTERNAL_ERROR`,
`RESOLUTION_ERROR`, `PARSER_ERROR`, `PARSER_PANIC`, `INVALID_SOURCE_SPAN` and
`SANDBOX_VIOLATION` are engine defects, and the next step for each is a bug report.
`REPORT_CONSTRUCTION_FAILED` sits beside them with one thing to rule out first, since the
report is written to a stream and a stream that will not take the bytes fails the same way.

Two codes come from the ceilings, `RESOURCE_LIMIT_EXCEEDED` and `OUTPUT_LIMIT_EXCEEDED`. No
option raises a ceiling, so the two numbers in the row are the whole answer: the input is
past what one run reads or what one report holds. [Limits and refusals](limits.md) lists
every ceiling and the measurements behind the three that moved.

`DOCUMENT_INVALID` is the one code that does not end a run. A document whose bytes will not
decode says nothing about the rest of the tree, so that file is reported unsupported, an
`unsupported-document-format` finding records that its references went unchecked, and the
scan carries on.

## What each code means

One fixed sentence per code, copied from
[`AnalysisErrorCode::meaning`](https://github.com/HardMax71/amiss/blob/main/crates/amiss-wire/src/report.rs)
and checked against it in CI. The machine report carries the same sentence on every error
row, so this page is a reference, not a second source of truth.

<!-- amiss-doc-contract:error-meanings:start -->
- `INVALID_INVOCATION`: the command line does not match the closed grammar; each documented option appears at most once and nothing else is accepted
- `INVALID_EVENT`: the declared repository, ref, default-branch, or forge identity is unusable as written; pass a lowercase owner and name, full refs/heads/ references, and --forge for a host outside the known table, as the stderr reason line names
- `INVALID_PROFILE`: the profile is not observe, enforce-introduced, or enforce; pass one of those three to --profile
- `REQUEST_UNREADABLE`: the machine evaluation request bytes could not be read; nothing was evaluated, so resend the whole sealed request frame on stdin with nothing after it
- `CONFIGURATION_INVALID`: a policy or control input violates its schema; correct the file the row names, since one unknown field or malformed value makes the whole file invalid rather than partly honored
- `DUPLICATE_JSON_KEY`: a JSON input repeats an object key; drop the duplicate, since strict parsing refuses the file rather than choosing one of the values
- `INVALID_UTF8`: a JSON input carries bytes that are not UTF-8
- `INVALID_JSON`: an input that must be JSON does not parse as strict JSON; fix the syntax in the file the row names
- `UNKNOWN_SCHEMA`: a JSON input declares a schema identifier this engine does not recognize; correct the identifier, or upgrade to a release that defines it
- `UNKNOWN_FIELD`: a JSON input carries a field its closed schema does not define; unknown fields refuse rather than pass through unread
- `NONCANONICAL_ARRAY`: a JSON input array violates its required canonical ordering or uniqueness; sort it and drop the repeated members
- `DIGEST_MISMATCH`: a digest carried by an input does not match the bytes it names; the input is stale or altered
- `CONTROL_BINDING_MISMATCH`: an external control is bound to a different repository, ref, or run identity than this evaluation; nothing is applied, so reissue the control against the identity under review or leave it out
- `EXCEPTION_OVERLAP`: accepted exception items select the same finding more than once; narrow the debt and waiver items until each finding is selected once, since overlap is refused rather than double-suppressed
- `UNSUPPORTED_CAPABILITY`: a candidate document declares a reserved amiss: capability this engine does not implement; correct the declaration in the named document, or upgrade to a release that implements it
- `GIT_REPOSITORY_UNAVAILABLE`: the --repo path does not open as a Git repository of the declared object format; pass the checkout root from git rev-parse --show-toplevel and match --object-format to git rev-parse --show-object-format
- `GIT_OBJECT_MISSING`: a commit, tree, or blob the run needs is absent from the object store; fetch full history or name commits the store holds, and for a whole-tree scan pass --base $(git rev-parse HEAD) --index rather than the all-zero or empty-tree id
- `GIT_OBJECT_WRONG_KIND`: a Git object is not the kind its use requires, as when a named commit resolves to another type
- `GIT_OBJECT_UNREADABLE`: a Git object exists but its bytes cannot be decoded; the object store is damaged, so check it with git fsck and restore it from a fresh clone
- `GIT_INDEX_INVALID`: the staged index file does not parse under the index grammar; remove .git/index and run git reset to rebuild it, or compare two commits instead of the index
- `GIT_INDEX_UNMERGED`: the index holds unmerged conflict entries, so no single staged state exists; finish or abort the merge before checking the index
- `GIT_INTENT_TO_ADD`: the index holds an intent-to-add entry whose content is not staged; stage the file or drop the intent entry before checking the index
- `GIT_SNAPSHOT_CHANGED`: the staged index changed while the run was reading it; rerun when the repository is quiet
- `UNREPRESENTABLE_PATH`: a tree or index name is outside the path grammar, a backslash, a NUL, or a dot segment; the exact bytes are disclosed as hex
- `DOCUMENT_INVALID`: a document's bytes cannot be decoded as its format requires; save the file as UTF-8 or fix the syntax its grammar refused, and only that document is unsupported, never the run
- `PARSER_ERROR`: the pinned parser failed on a document; this is an Amiss defect rather than a fault in the file, so report it as a bug with the named document
- `PARSER_PANIC`: the pinned parser panicked on a document; the panic is caught and the run is incomplete, so report it as an Amiss bug with the named document
- `INVALID_SOURCE_SPAN`: the parser returned a node whose byte span does not address the document; the parse is not trusted, so report it as an Amiss bug with the named document
- `RESOLUTION_ERROR`: reference resolution failed internally; no input fixes this, so report it as an Amiss bug with the command line
- `RESOURCE_LIMIT_EXCEEDED`: a named resource crossed its ceiling, and no option raises one, so the input has to come under it; the row carries the resource, the configured limit, and the observed lower bound
- `OUTPUT_LIMIT_EXCEEDED`: the serialized report would cross the machine-json-bytes ceiling; no option raises it and findings are never dropped to fit, so the repository is past what one report holds
- `TOO_MANY_ERRORS`: more distinct analysis errors accumulated than the retention ceiling; the lowest-keyed rows are kept and this sentinel stands for the rest, so fix those and rerun for what is left
- `REPORT_CONSTRUCTION_FAILED`: the report could not be constructed or emitted; the run has no trustworthy output, so check that the output stream can still be written, then report it as an Amiss bug
- `SANDBOX_VIOLATION`: the run breached its sandbox descriptor; the result is not trustworthy, so discard it and report an Amiss bug with the command line
- `TRUSTED_TIME_INVALID`: a control that needs trusted time has no statement that verifies, absent or failing its binding; the run will not act on an unverified clock, so supply a statement bound to this run or drop the control
- `INTERNAL_ERROR`: an engine invariant failed; this is a defect in Amiss, not in the input, so discard the run and report it as a bug with the command line
<!-- amiss-doc-contract:error-meanings:end -->
