# Scanner v0 specification

Date: 2026-07-12.

Status: normative implementation boundary for the first discard-state scanner. This file refines
the product boundary in [normative-core-spec.md](./normative-core-spec.md) and inherits the
snapshot, policy, threat, resource, and fail-closed rules in
[ci-security-spec.md](./ci-security-spec.md). If the files disagree, the stricter no-write,
no-execution, fail-closed rule applies and the discrepancy is a specification bug.

`MUST`, `MUST NOT`, `SHOULD`, and `MAY` are normative.

## Purpose

Scanner v0 answers only these questions:

1. Does an explicitly supported same-repository document reference resolve in the exact evaluated
   tree?
2. Did the raw bytes or Git mode of a referenced regular file change between the exact base and
   candidate trees?
3. Did the source block containing that reference remain byte-equivalent, co-change, disappear,
   or become impossible to correlate without guessing?
4. What document/reference surface was discovered, excluded, opaque, unsupported, or unlinked?

Provider/repository identity v1 supports public `github.com` only. GitHub Enterprise Server,
GitLab, Bitbucket, and other hosts require a new provider identity/event contract and produce
typed invocation error `UNSUPPORTED_PROVIDER_HOST`, exit 2, before same-repository URL or
required-event evaluation.

It does not answer whether prose is true, complete, fresh, reviewed, or semantically affected. It
has no baseline, ledger, state directory, claim acceptance, refresh, migration, lifecycle,
repository-authored waiver, network, command execution, or model judgment. The machine contracts
define a digest-protected external-waiver value for a future required wrapper, but the authorized
disposable CLI has no input lane for it and reports floor, debt, and waiver controls as `none`.

## Commands and profiles

The public local scanner exposes exactly:

```text
assure check --repo <path> --object-format <sha1|sha256> --base <full-oid> (--candidate <full-oid>|--index) [--repository github.com/<owner>/<name> --ref refs/heads/<name> --default-branch-ref refs/heads/<name>] --profile <observe|enforce> [--explain-scope] [--format <human|json>]
```

Each option occurs at most once and angle brackets are metavariables. Options may appear in any
order after `assure check`; parsing and semantics are order-independent. A value-taking option
consumes only its immediately following argv token when that token's Unicode scalar sequence does
not begin with ASCII `--` (an empty token is still a supplied value and is then classified by that
option). Every argv token must losslessly decode to Unicode scalar values: invalid UTF-8 argv bytes
on POSIX or an unpaired UTF-16 surrogate on Windows are `INVALID_INVOCATION` before lossy runtime
conversion. Lexical option/value rules operate on the canonical UTF-8 encoding of those scalars.
`--repo` is not normalized or serialized: POSIX opens the exact UTF-8 path bytes and Windows opens
the exact UTF-16 encoding of the same scalars; a relative path is resolved by the OS against the
process working directory captured at scanner startup. Non-UTF-8 POSIX repository names and
unpaired-surrogate Windows names are outside v0. An
option-shaped next token therefore leaves the value missing and is parsed independently; lone `--`
is an unknown option, not an end-of-options separator, and `--name=value` is unknown rather than
attached-value syntax. A literal path beginning with `--` must use a non-option prefix such as
`./`. `--repo` is an exact
acquisition location, not a discovery starting point. V0 supports only a primary non-bare
repository worktree root whose final path entry is opened as a directory without following that
entry as a symbolic link/junction/reparse point and whose direct `.git` child is likewise an actual
directory. It does not search parents or children. A bare repository, `.git` file (including linked
worktrees and submodules), missing `.git`, symlink/reparse `.git`, or path that merely contains a
nested repository is `GIT_REPOSITORY_UNAVAILABLE`, fatal exit 2. A directly named primary root is
eligible even if an unrelated outer repository exists.

The wrapper opens the root and `.git` handles once and performs all administrative/object access
relative to them; the host path never enters semantic output. Required index/object/pack paths must
be ordinary no-follow entries below that `.git` handle. Repository discovery, configured object
alternates, replacement refs, grafts, promisor fetches, and environment-selected object databases
are ignored; an object absent from the primary `.git/objects` database is missing. A platform that
cannot enforce this handle/no-follow boundary reports the repository unavailable rather than
falling back to pathname traversal. Root-shape failures use `GIT_REPOSITORY_UNAVAILABLE`; malformed
or non-ordinary index storage uses `GIT_INDEX_INVALID`; missing/unreadable selected objects use the
corresponding `GIT_OBJECT_*` code.

The repository reader is an in-process, configuration-free `primary-object-db-v1` implementation;
the public CLI does not delegate object selection to an installed Git. `--object-format` selects
the SHA-1 or SHA-256 object namespace. Every selected loose or packed object is reconstructed as
`<type> SP <decimal-size> NUL <body>` and MUST hash to the requested full OID under that algorithm;
there is no config-selected format that may override it. All base/candidate/index OIDs use that one
namespace. A hash, header, declared-size, checksum, delta, tree, or commit mismatch is
`GIT_OBJECT_UNREADABLE`.

The reconstructed loose-equivalent header grammar is byte-exact. `type` is exactly one lowercase
token from `blob`, `tree`, `commit`, or `tag`; `size` is exactly `0` or `[1-9][0-9]*`, with no sign,
leading zero, or whitespace; there is exactly one ASCII space and one NUL. Interpret the decimal as
an arbitrary-precision mathematical integer and compare it digitwise, before allocation or host
integer conversion, with the smallest per-value cap already applicable to the selected use:
document, referenced-target, or selected-control blob first, otherwise `git-object-bytes` for
commit/tree/delta-base/general object acquisition. A declared value above that first cap uses its
single resource crossing/saturation law, not header-invalid, host overflow, or a second larger-cap
error. For a within-limit value, the declared
size equals the body byte length and no byte follows that body. In a SHA-1 namespace every
Git object-OID preimage is checked with `sha1dc-855827c-v1`: the algorithm at
`cr-marcstevens/sha1collisiondetection` commit
`855827c583bc30645ba427885caa40c5b81764d2` (the submodule pinned by Git v2.44.0), with safe-hash
rewriting disabled, unavoidable-bitconditions enabled, and any nonzero collision return rejected as
`GIT_OBJECT_UNREADABLE`. A non-collision result must equal ordinary SHA-1. Pack/index metadata
checksums still use the declared ordinary Git checksum and their owning pack/index error
constructors; they are not object-OID authorization. E0 must run the pinned detector's complete
upstream test directory plus non-collision Git-object preimages on every supported architecture.
This mitigates known collision families, but does not make SHA-1 a cryptographically strong
authorization identity. A future provider-authorizing contract must therefore prefer a SHA-256
repository and, when SHA-1 cannot yet be retired, authenticate and bind a canonical independent
SHA-256 digest over every loose-equivalent object preimage actually used for snapshot construction
or evaluation (commits, traversed trees, and selected blobs) into its merge-time execution epoch.
Binding only the top commit/tree preimages is insufficient because a colliding child blob could
retain the same SHA-1 tree entry.

Object lookup is total:

1. Try the exact loose-object path first through the `.git/objects` no-follow handle. If it exists,
   it must be one ordinary zlib object. Its held-file compressed byte length is capped before
   inflation and the reader still stops at cap+1 if metadata races; it then validates completely.
   Corruption is fatal and never falls back to a pack.
2. If loose is absent, enumerate ordinary no-follow `pack-<hex>.pack`/`.idx` pairs beneath the
   primary `objects/pack` directory. An absent directory is an empty pack set; a present
   non-directory/symlink/unreadable entry is `GIT_OBJECT_UNREADABLE`. Count **every** actual
   directory entry—including ignored/junk names—while streaming, excluding only the `.` and `..`
   pseudoentries if the host API exposes them, and stop at entry 8,193 before retaining/sorting;
   then sort the bounded names by
   raw basename and cap valid pairs at 4,096. Parse pack versions 2/3 and index
   versions 1/2 exactly as the pinned `gitformat-pack` grammar; validate index ordering/fanout,
   CRC/offsets where present, and the complete index checksum. Per-index and aggregate raw-index
   byte caps apply in sorted basename order before later indexes are read. Read each paired pack header and
   trailer, require its object count to equal the index count and its stored trailer to equal the
   pack checksum recorded by the index; unrelated pack bodies are not claimed validated. The
   lowercase `<hex>` is exactly 40 digits in a SHA-1 namespace or 64 in a SHA-256 namespace, and its
   decoded value equals both the pack trailer checksum and the pack checksum stored in the index.
   An exact pack/index name without its pair or a malformed enumerated index/pack is fatal. `.bitmap`,
   `.rev`, `.mtimes`, MIDX, temporary names, alternates, and promisor metadata never select an
   object.
3. Among validated indexes containing the requested OID, choose the raw-byte-lowest pack basename.
   No later duplicate is a corruption fallback. Resolve OFS/REF deltas recursively under the same
   lookup, reject cycles/invalid bases, and reconstruct/hash the final object before use. Before
   inflating each selected packed entry/delta base, compute its stored interval from its offset to
   the next object offset or pack trailer and apply the per-stream plus evaluation-aggregate
   compressed-byte caps; no unbounded deflate padding is consumed.
4. If no validated loose/pack row contains the OID, emit `GIT_OBJECT_MISSING`. Wrong reconstructed
   type is `GIT_OBJECT_WRONG_KIND`. A repack race may yield missing/unreadable but never a different
   valid object's bytes; every successful result remains content-addressed.

The grammar source is Git's
[`gitformat-pack` 2.44.0 contract](https://git-scm.com/docs/gitformat-pack/2.44.0).
`git-object-bytes` caps each inflated object or delta base at 134,217,728 bytes before allocation;
`git-compressed-object-bytes` caps each selected loose compressed file or packed-entry interval at
268,435,456 bytes; `aggregate-git-compressed-object-bytes-per-evaluation` caps their logical charges
at 2,147,483,648 bytes; `git-pack-directory-entries` caps all examined `objects/pack` names at 8,192;
`git-pack-files` caps retained pack/index pairs at 4,096; `git-pack-index-bytes` caps each index at
536,870,912 bytes; `aggregate-git-pack-index-bytes` caps indexes read in one evaluation at
1,073,741,824 bytes; and `git-delta-depth` caps a reconstruction chain at 128 (attempting member 129
is the count-resource crossing). A compressed storage member is charged once per snapshot and
selected OID, including every delta-chain member; repeated logical use/cache hits do not change the
charge, while base and candidate snapshots charge independently. Only the selected pack's
header/trailer, selected row CRC, and selected object/delta chain are read from `.pack`; the index's
stored pack checksum must equal the pack trailer, but the scanner does not stream every unrelated
pack body.
The smaller document/target/control inflated caps still apply first when their object header declares a
larger contextual value. E0 includes loose/pack, duplicate, corrupt-preferred-copy, v1/v2 index,
delta-cycle/depth, checksum, SHA-1/SHA-256, and concurrent-repack fixtures.

Selected commit/tree bodies use `git-object-grammar-v1`; no library may accept a broader shape:

- A tree body is zero or more concatenated
  `<mode-ascii> SP <name-bytes> NUL <raw-oid>` entries. Mode is exactly `40000`, `100644`,
  `100755`, `120000`, or `160000`; raw OID width is 20/32 bytes for SHA-1/SHA-256. A name is
  nonempty, contains neither NUL nor `/`, and is not `.` or `..`. Entries are unique and strictly
  ordered by unsigned byte comparison of `name + "/"` for tree mode and `name` otherwise. There is
  no trailing padding. During iterative snapshot traversal, only `40000` children are opened and
  must resolve to trees. Regular/symlink modes record their OID without opening it and require a
  type-correct hash-verified blob only if later selected as a document/control/target; `160000` is a
  gitlink OID in another repository and is never opened. Index mode retains its separately stated
  stricter whole-index blob/symlink validation.
- A commit body uses LF-only header records, contains no NUL/CR before its header terminator, and
  begins with exactly one `tree <lowercase-full-oid>` line. It then has zero or more contiguous
  `parent <lowercase-full-oid>` lines, exactly one nonempty `author ` line and one nonempty
  `committer ` line, in that order. Zero or more later extension headers have a nonempty ASCII key
  containing neither space nor control bytes, one space, and arbitrary non-LF value bytes;
  extension keys MUST NOT be `tree`, `parent`, `author`, or `committer`;
  continuation lines begin with one space and require a preceding extension header. One blank LF
  terminates headers; remaining message bytes are opaque. Unknown extension headers are retained
  for object hashing but do not affect snapshot identity. Parent OIDs are recorded in order but are
  not recursively opened; a provider-event parent is opened only when that event constructor
  separately requires it. Tags are not peeled into commits.

Duplicate/out-of-order tree names, invalid modes/OID widths, a tree cycle, bad commit header
order/multiplicity, or malformed continuations are `GIT_OBJECT_UNREADABLE`. An absent selected
tree/blob uses `GIT_OBJECT_MISSING`; a present wrong type uses `GIT_OBJECT_WRONG_KIND`, preserving
the global disjoint lookup constructors. Tree traversal is iterative; `RepoPath`'s 4,096-byte bound and the logical
tree-entry resource bound traversal without a call-stack-dependent depth limit. A cycle exists only
when the same tree OID recurs on the current ancestor stack. Reusing one subtree OID at distinct
non-ancestor logical paths is a valid DAG: expand and charge its complete entries separately at each
path; a global visited set is invalid. E0 contains exact empty/merge/signed-extension commit, tree
ordering/mode, ancestor-cycle, and shared-subtree-DAG fixtures.

`--base` is mandatory. Exactly one candidate selector is legal: `--candidate` creates explicit
commit-pair mode and `--index` creates the canonical staged-index projection. OIDs are lowercase,
full-length for the declared object format, and commit base/candidate OIDs must differ. There is no
implicit `HEAD`, merge base, worktree default, abbreviated OID, ancestry inference, or worktree
overlay. `--worktree` is `INVALID_INVOCATION`, exit 2 before repository traversal.

Repository identity is an all-or-none triple. Owner/name must already be lower-case and both refs
must pass the full-ref rules. When omitted, all three evaluation fields are null and a GitHub URL
cannot be classified as same-repository. Local mode derives event/finality exactly as
`explicit-commit-pair/explicit-replay` or `local-index/local-nonfinal`; users cannot spoof provider
event kinds.

`--format` defaults to `human`. JSON emits exactly the canonical envelope plus LF.
`--explain-scope` adds deterministic explanation only to the human projection; with JSON it is
accepted but produces byte-identical output to omission. Neither flag changes facts, controls,
disposition, or exit. Unknown flags, missing/partial tuples, positional arguments, and every other
command are `INVALID_INVOCATION`, exit 2.

Invocation value classification is closed and set-valued; it is not first-error-wins:

| Safely established argv defect | Error code |
| --- | --- |
| Unknown/duplicate option, missing value, positional token, illegal selector/identity tuple, empty `--repo`, bad `--object-format`, malformed/wrong-length/non-lowercase OID, or equal base/candidate OIDs | `INVALID_INVOCATION` |
| A unique syntactically complete `--profile` value other than `observe`/`enforce` | `INVALID_PROFILE` |
| A unique `--repository` value with exactly three nonempty slash components whose host is not `github.com` | `UNSUPPORTED_PROVIDER_HOST` |
| A complete unique GitHub repository/ref/default-ref triple whose owner/name or either full ref fails its lexical/`ref-format-v1` contract | `INVALID_EVENT` |

Parse the entire argv without repository access, deduplicate the resulting null-path error tuples,
and emit every applicable row. Structural invalidity does not suppress an independently complete
bad profile/provider/event value, but an incomplete value is not guessed into a lower row. The
special malformed-`--format` channel below still has no report envelope, so only its fixed stderr
line and exit code are externally visible. E0 vectors cover every row and simultaneous pairs.

Output selection for an invalid invocation is not first/last-wins. Parse the complete argument
vector without repository traversal. Exactly one syntactically complete `--format human|json`
selects that projection even if another option is invalid; zero `--format` occurrences selects
human. A duplicated `--format`, a missing value, or any value other than `human`/`json` makes output
selection itself invalid: stdout is empty, stderr is exactly the ASCII line
`assure: invalid invocation\n`, and exit is 2. Thus malformed output selection can never choose
which conflicting value controls a supposedly canonical error envelope. E0 includes all duplicate,
missing, option-shaped-value, lone-`--`, attached-value, invalid-native-encoding, unknown, and
otherwise-invalid argument permutations on every supported platform.

No provider-required wrapper API is published or authorized in v0. The request domains and
provider/control shapes in machine-contracts are target properties for a later `stable-v1`
wrapper, not an implicit transport. The disposable public command runs the evaluator in-process,
derives only the local event/finality rows above, reports execution constraint, external floor,
debt, waiver, and trusted time as `none`, reports the fixed sandbox as
`self-asserted/local-process` with null verification, and has no retained request streams.
A separate request-wire RFC with root schemas, framing goldens, and provider tests must define the
only future input lane; hidden flags, environment variables, and an implementation-private stream
cannot assert external trust.

There is one analysis and policy path: `check` always evaluates the complete declared scope and
applies the selected profile and verified controls. A report-only rollout uses `observe` and does
not register the job as required. The disposable local CLI has no external floor, so only its
built-in/profile dispositions apply; a verified floor promotion belongs to the future required
wrapper contract. Analysis failure remains exit 2.

For the disposable CLI, the caller selects the profile through the mandatory `--profile` flag.
Candidate repository content cannot select or lower it. A future pinned required wrapper may fix
that input and an externally verified organization floor may raise its minimum only after the
request-wire/provider gates open.

| Profile | Explicit supported structural failure | Raw change impact | Intended use |
| --- | --- | --- | --- |
| `observe` | `warn` | `record` or `warn` | Calibration and first rollout |
| `enforce` | `fail`, subject to externally registered adoption debt | Never above `warn` in scanner v0 | Required check after cleanup/calibration |

An external floor MAY promote a deterministic structural finding from `warn` to `fail`. It MUST
NOT promote raw change impact, rename suggestions, or ambiguous observation correlation to `fail`
in scanner v0.

## Candidate document set

Discovery independently enumerates the complete base snapshot and complete candidate commit/index
snapshot. On each side it applies that side's built-in rules, excluded-tree rule, and repository
policy; `DocumentResult` paths are the union of the two selected non-tree path sets. This is what
makes base-only document/reference removal and paired structural attribution observable. Candidate
summary denominators and `outside_document_set` remain candidate-only, as machine-contracts states.
Discovery never walks the host filesystem recursively.

### Structured documents

Regular blobs are structured documents when their path has one of these exact lowercase suffixes:

- `.md`;
- `.mdx`;
- `.markdown`.

The following exact extensionless basenames are parsed with the Markdown adapter:

- `README`;
- `CONTRIBUTING`;
- `CHANGELOG`;
- `SECURITY`;
- `SUPPORT`;
- `CODE_OF_CONDUCT`.

Names with other case or suffixes are not silently treated as equivalent. A future discovery
schema may add them explicitly.

### Plain-text advisory documents

The exact basenames `.cursorrules` and `llms.txt` are counted as plain-text advisory documents.
Scanner v0 extracts no references from them. This deliberate zero-lexer rule avoids turning an
underspecified token heuristic into a stable machine API. `CLAUDE.md` and `AGENTS.md` use the
Markdown adapter through their suffix.

Arbitrary `.txt`, `.rst`, `.adoc`, `.org`, HTML, notebooks, source comments, YAML, TOML, and config
files are not documents in scanner v0. They are counted as outside the disclosed document set, not
silently scanned as equivalent prose.

### Built-in excluded trees

A document under a path component in this closed set is discovered but excluded by built-in scope:

```text
node_modules
vendor
third_party
dist
build
.next
target
```

Tracked files still appear in the discovered/excluded denominator. Repository ignore rules do not
exclude tracked documents. Repository policy may explicitly add a document or tree to scanning;
scanner v0 repository policy cannot remove a built-in document or protected external inventory
member. The exact path, raise-only fields, ordering, and digest are defined in
[machine-contracts.md](./machine-contracts.md#repository-policy).

There is no content-sniffing exception for a matching name. A dependency lockfile, minified asset,
or binary-looking blob whose path matches a built-in document rule remains in the denominator; an
invalid UTF-8 or unparseable body fails analysis under the rules below. Such files are outside the
document set only when their paths match neither a built-in class nor a policy include. The
scanner's own hypothetical `.assure/state/**` and investigation files under `ci-idea/**` receive no
special product exemption.

### Document classification and exclusion precedence

`DocumentResult.classification` is intrinsic to the path plus the union of base/candidate include
rules and is shared across sides. Apply the first matching row:

1. lowercase `.md` or `.markdown` suffix: `structured-markdown`;
2. lowercase `.mdx` suffix: `structured-mdx`;
3. exact extensionless built-in basename: `extensionless-markdown`;
4. exact `.cursorrules` or `llms.txt` basename: `plain-advisory`;
5. otherwise matched by at least one base or candidate repository-policy document/tree include:
   `policy-included`.

There is no sixth value. A path that matches none of these rows has no `DocumentResult`; floor
inventory may separately report it as outside coverage. A side is non-null exactly when a non-tree
entry (blob, symlink, or gitlink) exists on that side and that side's intrinsic built-in rule or
repository-policy include selects the path. Thus a policy-only path included only by the base
retains a base-only `DocumentResult`
even if an otherwise nondocument candidate blob remains; that candidate blob is instead counted in
`outside_document_set`. For each non-null side independently, a built-in excluded-tree component
sets `DocumentSide.status = excluded-built-in` unless that side's
repository policy includes the path; a matching include overrides exclusion and the side is
scanned/unsupported normally. Exclusion never changes top-level classification. Native classes use
their named adapter; `policy-included` paths have no v0 adapter and are
`unsupported-document-format`. This precedence also fixes paths matched by both an exact and tree
include or by several discovery obligations.

### Unsupported documents

These deliberate object/format boundaries are discovered and represented as unsupported document
sides:

- symlink documents;
- Gitlinks/submodules;
- LFS pointer content when the document body is required;
- policy-included files for which no v0 adapter exists.

Invalid UTF-8 bytes/paths, unreadable content, a parser that cannot complete, and every resource
limit are typed `AnalysisError` failures under both profiles. They make the run incomplete and exit
2; a producer may not downgrade them to an unsupported document. Deliberately unsupported documents
remain in every summary. When repository/floor coverage explicitly brought an unsupported format
into scope, the accompanying requested-capability error also makes the run incomplete; an
unrequested disclosed boundary remains complete/advisory.

### Unlinked documents

Scanner v0 uses `unlinked` in one deliberately narrow, locally reproducible sense: a candidate-side
document is unlinked exactly when its `DocumentSide.status` is `scanned` and its
`extracted_references` count is zero. This is an outgoing-reference coverage observation, not an
inbound reachability/orphan analysis. An opaque construct is not an extracted reference; a
successfully extracted local, same-repository, unsupported, or external reference all make the
count nonzero regardless of resolution outcome.

The scanner emits exactly one `unlinked-document` finding for every such candidate path and no such
finding for an unsupported, built-in-excluded, or base-only removed document. The summary's
`documents.unlinked` value equals both the number of matching candidate document results and the
number of `unlinked-document` findings. This finding is advisory and does not create a passing
relationship.

## Source adapters

The stable v0 adapter set is exactly `markdown-v1`, `mdx-v1`, and `plain-advisory-v1`. A run cannot
load, select, or promote an external renderer, route, fence-metadata, or literal-attribute adapter.
Adding one requires a new engine/report contract and compatibility review.

### Markdown

The Markdown adapter uses the closed `commonmark-gfm-v1` grammar profile. Core block/inline syntax
is CommonMark 0.31.2. The parse additions are exactly the `remark-gfm@4.0.1` bundle: tables,
task-list items, strikethrough, extended autolinks, and footnote reference/definition nodes, with
the parser option object exactly `{singleTilde: true}`. Single-tilde strikethrough and footnotes are
explicit plugin/GitHub extensions beyond formal GFM 0.29 rather than being mislabeled as CommonMark.
When repeated core text differs, CommonMark 0.31.2 wins. GFM tagfilter is a rendering/HTML transform,
not a parser-AST rule here: the scanner runs no renderer and preserves matching source as opaque raw
HTML. A footnote reference is not a link observation; supported links/images nested inside a
footnote definition are ordinary syntax-node occurrences with the usual owner/address rules, and a
footnote definition is never a reserved CommonMark link-reference definition. The MDX adapter uses
`mdx-source-v1`: that exact profile plus the syntax accepted by `remark-mdx@3.1.1`, with MDX ESM,
JSX, and expressions made opaque by the interval law below and never evaluated.

The immutable Markdown oracle pipeline is `unified@11.0.5` → `remark-parse@11.0.0` →
`remark-gfm@4.0.1` with the explicit option above; MDX appends `remark-mdx@3.1.1` in that order. No
remark stringify/rehype/render plugin or workspace configuration runs. The packages are pinned by the
`docs/package-lock.json` blob `926269e581901acd6c4ce1aef42210d8efa548c5` at commit
`1e31dfebf2bc21fe90933394e7338541eaaadaad` (raw SHA-256
`cfcf4f37d9b619da23d1011f4d8e98f3f8cc677f374e64a7929e04151c26b71b`). It is a
development oracle, not runtime permission to load workspace packages. A different parser
name/version is conforming only when it reproduces the pinned extraction, raw-byte spans, node-path
addresses, definition precedence, and opaque intervals for the complete profile corpus. The
descriptor's free-form parser provenance cannot select grammar semantics.

`parser-work-accounting-v1` also freezes the otherwise implementation-dependent node resources.
For Markdown and MDX, first recognize `frontmatter-v1` and pass only the suffix beginning at its
exclusive raw end to the ordered oracle pipeline; with no recognized region, pass the complete
source. Oracle positions are translated back by adding that suffix's raw byte/line offset, while
node paths are rooted in the suffix tree. Frontmatter contributes no parser node and cannot be
interpreted as Markdown, MDX, JSX, or an expression. The logical tree is the ordered unist tree
returned for that suffix. Count the root and every object reachable through a
node's ordered `children` array whose `type` member is a string; do not count `position`, `data`,
tokenizer events, or other metadata objects. Raw HTML and MDX node objects are counted even when
their source intervals are opaque. Node depth is one for the
root and parent depth plus one for a child; `parser-nesting` is the maximum depth, so an otherwise
empty document charges one node at depth one. The per-snapshot node count is the sum of these
logical per-document counts for every parsed path on that side, independent of blob/cache reuse.
An implementation need not materialize this oracle tree, but it must reproduce both integers. The
required corpus includes hostile YAML/TOML bodies containing braces, JSX-looking text, imports, and
link syntax, proving that recognized frontmatter changes neither parsing nor extraction.

For `plain-zero-lexer-v1`, charge one synthetic document root plus one synthetic paragraph node for
each maximal run containing a nonblank line. Use the same line scanner as source spans: CRLF is one
ending and bare CR or LF is also an ending. A line is blank exactly when its content bytes after
removing that ending are zero or only ASCII space/tab. Paragraph runs are separated by one or more
blank lines. Plain depth is one with
no run and two otherwise. These synthetic nodes have no structural address, byte span, occurrence,
or report projection. Every parser-profile corpus case must publish its exact node count and depth;
until those goldens exist, different implementations cannot claim resource-equivalent conformance.

The adapter preserves source byte spans and recognizes frontmatter explicitly before assigning
blocks. It extracts supported constructs from syntax nodes, not regular expressions over the whole
document. E0 cannot begin parser integration until a nonshrinkable manifest checks in the raw source
and expected full-tree node count/depth plus extraction/span/address/owner/opaque goldens for every
executable CommonMark 0.31.2 example, every GFM 0.29 example, the pinned remark-gfm footnote and
single-/double-tilde examples (including nested links and first-definition interactions), and the
official MDX 3.1.1 ESM/JSX/expression syntax/error fixtures. Lists, headings, block quotes, raw HTML,
tables, task lists, strikethrough, footnotes, and their nesting around links are therefore corpus
inputs even when the container itself emits no observation.

`frontmatter-v1` recognizes a frontmatter region only at byte offset zero, optionally after one
UTF-8 BOM. The first complete line must be exactly `---` or `+++` after removing its line ending.
Define frontmatter offset zero as the first byte after that optional three-byte BOM. The closing
line must be the same delimiter (`---` also permits `...`), and the byte immediately after its line
ending—or immediately after the closing delimiter when it ends at EOF—must have the exclusive
frontmatter-relative offset at most 65,536.
Thus `frontmatter_bytes <= 65,536`; with a BOM the corresponding raw document end offset may be
65,539. Equality at 65,536 is accepted and 65,537 is not a frontmatter region. The adapter treats
the complete region as opaque bytes; it does not parse YAML,
TOML, or JSON. An opener without a permitted closer is ordinary Markdown, not a guessed partial
header. Frontmatter recognition runs before CommonMark block parsing and uses the same CR/LF line
scanner as source-span accounting. Every document side and candidate summary reports frontmatter
region/byte counts separately from opaque MDX and HTML; frontmatter URLs/paths are outside the
reference denominator rather than disappearing from coverage disclosure.

The optional three-byte UTF-8 BOM is not part of `frontmatter_bytes`; the region begins at the
opening delimiter and includes both delimiters and any line endings present. Opaque accounting is a
partition: remove frontmatter first, then MDX ESM/JSX/expression regions, then recognize raw HTML
only in the remaining Markdown surface. These byte spans never overlap, and their summed byte
counts cannot exceed the raw document byte count.

Opaque regions are canonical half-open raw-byte interval unions, not parser-selected node counts.
For MDX, take the complete source span of every MDX ESM node, flow/text expression, and flow/text
JSX element; an outer JSX span includes all Markdown-looking children, so none are extracted.
Discard spans inside frontmatter, then sort by `(start,end)`, discard any span contained in another,
and union overlapping or exactly adjacent spans. The resulting maximal disjoint intervals are the
MDX regions. On bytes outside frontmatter/MDX intervals, take complete CommonMark raw-HTML block and
inline-node spans and apply the same containment/overlap/adjacency union to obtain HTML regions.
Region count is the number of final intervals and byte count is the sum of `end - start`; therefore
each is zero exactly when the other is zero, and every retained region is nonempty. Markdown has
zero MDX regions; plain-advisory has zero MDX and HTML regions. Parser spans outside the document,
reversed spans, or a claimed overlap after this construction are `INVALID_SOURCE_SPAN`.

It MUST distinguish prose from code spans, fenced/indented code, raw HTML, link-reference
definitions, and container blocks. Link-looking text inside code is not a native reference.

### MDX

The MDX adapter follows the non-evaluating rules in
[ci-security-spec.md](./ci-security-spec.md#mdx). ESM, JSX, and expressions are opaque in scanner
v0. It reports opaque byte and region counts. It never imports a page or invokes the repository's
docs toolchain/plugins.

Ordinary Markdown links outside opaque regions remain supported. A reference hidden inside an
opaque region is not claimed as checked.

### Plain text

The plain-text adapter validates UTF-8 and performs only the internal paragraph-run accounting
defined by `parser-work-accounting-v1`. It extracts no reference candidates and emits no paragraph
node/span/address in scanner v0. It has no heading, link, anchor, HTML, fence, or
governed-directive semantics.

## Reference classes

Every extracted item has one immutable class. Confidence does not silently promote a class.

Any reserved `assure:` link-reference definition is parsed in source order. Scanner v0 contributes
one `unsupported-capability: governed-claim` occurrence for each definition, aggregates them into
one path-scoped finding per document, and exits 2 under either profile. Finding aggregation reports
the exact definition count and representative/omitted locations. The scanner neither treats a
directive as an ordinary link nor creates state. This is the one scanner-v0 behavior for all
syntactically recognizable governed declarations, including malformed or future versions.
Every such definition consumes one `references-per-document` and one `references-per-snapshot`
budget unit before a control-state source is constructed, even though it does not become an ordinary
reference observation. Ordinary references and reserved definitions therefore share the 4,096
per-document cap, proving that the path-scoped `ControlStateInput.sources` array can represent every
distinct contributing projection digest; crossing either shared cap is the ordinary parse-phase
resource error and exit 2.

The constructor is exact. Preserve every candidate-side CommonMark definition node in source
order, including definitions that lose first-definition lookup or duplicate another label. Decode
only the node's label-string backslash escapes/entities; before CommonMark whitespace/case
normalization, a node is reserved exactly when those decoded scalar bytes begin with lowercase
ASCII `assure:`. An ordinary consuming reference is suppressed only when its first winning
CommonMark definition node is reserved by that test. A losing reserved duplicate still contributes
its governed occurrence but cannot retroactively suppress a consumer whose first winner is
nonreserved; conversely a later nonreserved duplicate cannot unsuppress a reserved first winner.
A suppressed consumer does not add another governed occurrence. A base-only or removed definition
emits no v0 control finding.

For each reserved candidate node, its contributing span starts at the opening `[` and ends at the
exclusive end of the complete definition destination/title syntax, including internal continuation
line endings and excluding only the line ending after the node. Its source digest is
`HB("assure/scanner-governed-definition-source/v1", exact bytes in that span)`. Group equal digests
into sorted `ControlStateInput.sources` with exact positive multiplicity; `member_count` remains the
total node count, not the number of distinct digests. The finding has null base state, candidate
`unsupported`, candidate-side path/span locations for every node, and the least location as its
representative. Invalid parser spans are `INVALID_SOURCE_SPAN`, never an invented digest.

### Extracted reference observations

These constructs are extracted whether their destination is repository-local, same-repository,
external, or an unsupported boundary:

1. Markdown inline-link and inline-image destinations;
2. full, collapsed, and shortcut Markdown reference links/images after spec-compliant definition
   resolution;
3. CommonMark URI/email and GFM extended autolinks;
4. the same constructs in non-opaque MDX Markdown regions;
5. GitHub `blob` or `tree` URLs found through those constructs.

Only `repository-path` and the unique candidate-ref `same-repository-github` row perform structural
repository lookup. The default-only same-repository row is terminal version scope and does not
probe; external/site/unsupported intents remain observations with their exact boundary Resolution,
so an email- or external-link-only document is not falsely counted as unlinked.

An image target is structurally checked as a path but never parsed as a document. Raw HTML blocks
and inline HTML are opaque in scanner v0. Apparent `href` or `src` attributes inside them are
counted as opaque HTML, not extracted through a partial HTML grammar. V0 has no HTML-reference
coverage request.

### Renderer-specific constructs

Fenced-code metadata such as `file=` or `src=`, MDX literal attributes, imports, and transclusion
syntax are renderer-specific and are not extracted as scanner-v0 observations. Empty versus
non-empty fence bodies do not change that rule. A later contract may define one exact grammar and
its equality semantics, but v0 neither emits an adapter candidate nor promotes such syntax at run
time.

### Inference boundary

Repository-shaped inline code, bare filenames, plain-text tokens, code-fence body strings,
unlinked symbols, similarity, and history-derived paths are absent from the stable scanner-v0 fact
model. The repository experiment found that only 5 of 16 missing repository-rooted inline
occurrences were actionable, while a deterministic ambiguous sample produced no actionable miss;
see
[preimpl-experiments.md](./preimpl-experiments.md#3-inline-paths-are-valuable-evidence-against-making-inline-paths-blocking).

The dossier's existing inference scripts are research artifacts only. The authorized implementation
MUST NOT expose an inference command. No scanner-v0 option, policy, or floor field requests
inference, and a v0 report contains neither inferred observations nor an inference-request count.

### External and unsupported references

HTTP(S), mail, issue, package, and other foreign URLs are counted as
`external-out-of-scope`; scanner v0 performs zero requests. A foreign GitHub repository is
external. A same-repository URL pinned to a non-candidate revision requests historical scope and is
`unsupported-version-scope`, not current-tree success.

Site-root routes such as `/guide/start` are not repository-root paths. They are
`site-route-unsupported` in scanner v0. The scanner does not guess a docs framework from repository
code or load a route adapter at run time.

## Target parsing and resolution

Adapter destination bytes have two distinct representations. `raw_destination_digest` hashes the
exact source-token byte slice: for inline links/images and autolinks, the destination token without
syntactic angle-bracket delimiters; for reference links/images, the destination token of the first
winning CommonMark 0.31.2 definition, not the consuming label; titles and separating whitespace are
excluded. Empty destinations hash zero bytes. The source span still identifies the consuming
construct, while its definition provenance is recoverable from the evaluated document.

Semantic destination construction is syntax-specific. Inline and reference-style links/images use
CommonMark 0.31.2 link-destination processing: remove only its defined backslash escapes and decode
its named/numeric character references, then UTF-8 encode the resulting scalar sequence. A
CommonMark angle URI autolink uses the bytes inside `<...>` verbatim (backslash escapes do not
operate there). A CommonMark angle email autolink uses ASCII `mailto:` followed by the exact address
bytes inside `<...>`. For a token recognized only by the GFM 0.29 extended-autolink grammar, the raw
token is the grammar's final match after its trailing-delimiter rules; protocol forms use those
bytes verbatim, `www.` forms prepend ASCII `http://`, and email forms prepend ASCII `mailto:`.
All autolink forms retain `source_construct = markdown-autolink`; the source token/span and semantic
constructor distinguish them.

No syntax-to-semantic-destination constructor above performs percent decoding, Unicode
normalization, case folding, trimming, or repeated unescape. URI query/fragment splitting and path
resolution consume the resulting semantic bytes. Later classification may lowercase only the
emitted external scheme or ASCII-fold only literal GitHub owner/repository components for identity
comparison, as specified below; neither operation rewrites destination bytes or digest preimages.
Thus source spellings such as `&quest;` in an ordinary link may create a semantic delimiter, while
the raw-destination digest still distinguishes the spelling; an email autolink can never be misread
as a repository-relative path.

Target intent construction is fixed before lookup. A native Markdown/MDX link to a
repository-relative destination normally uses `target_kind = either`; when its semantic path has
exactly one terminal literal `/` after component splitting, remove that slash before RepoPath
construction and use `tree`. This is an authored directory hint, so `file.md/` cannot resolve a
blob. A native image with a terminal slash is `invalid/invalid-reference`; the scanner never strips
a path segment marker and then resolves `image.png/` as the blob `image.png`. Any other empty
component or multiple terminal slashes is invalid. A trusted
same-repository GitHub `/blob/` form uses `blob`, and `/tree/` uses `tree`. Reference-style forms
inherit link versus image kind from the consuming syntax node, not from their definition. Current
entry kind never changes the authored target kind.

Every occurrence receives exactly one TargetIntent variant after component splitting and before
repository entry lookup:

| Constructor outcome | TargetIntent |
| --- | --- |
| Native empty/relative path that reaches a contained `RepoPath` | `repository-path` with that path and native link/image target kind |
| GitHub URL with one unique candidate-ref split | `same-repository-github` with parsed path and `/blob/`/`/tree/` target kind |
| GitHub URL with one unique default-only split | `same-repository-github` with parsed path/kind even though Resolution is `unsupported-version-scope` |
| Syntactically valid ordinary or foreign URL (including nonmatching GitHub identity) | `external-url` with lowercased parsed scheme and null repository path/target kind |
| Native leading-slash site route | `site-route` with null repository path/target kind/scheme |
| Network-path reference, invalid URI/path/percent/control/traversal, ambiguous trusted-ref splits, or a GitHub version with no trusted split | `unsupported` with null repository path/target kind/scheme |

All rows retain the exact raw-destination digest and the component digests already safely split by
the rule below, including invalid/unsupported rows. A parse failure never nominates an external
scheme. This table, not the eventual Resolution, fixes ObservationId, correlation class, and
summary membership; `same_repository_github` includes the unique default-only row but not
ambiguous/no-split or foreign rows.

Component splitting follows RFC 3986 order, not a generic “next delimiter” loop. Find the first
semantic `#`; if present, `fragment` is every byte after it through end (including any later `?`).
Within the prefix before that `#`, find the first `?`; if present, `query` is every byte after it up
to the `#` (including additional `?` bytes). `path` is the remaining prefix before the first such
`?`/`#`. `query_digest` and `fragment_digest` hash those exact semantic UTF-8 component bytes before
percent decoding and excluding delimiters. Each field is null exactly when its delimiter is absent;
a present empty component hashes the empty byte string. Thus `a?x?y#z?u` has query `x?y` and
fragment `z?u`. Path decoding and fragment validation never rewrite those digest preimages.

Absolute URI parsing uses `uri-reference-v1`: the ASCII RFC 3986 generic syntax with no
normalization or IDNA conversion; scheme is `[A-Za-z][A-Za-z0-9+.-]*`, percent escapes are two hex
digits, and an HTTP(S) URI has `//` plus a nonempty authority. A raw non-ASCII authority, malformed
bracket/percent/authority, or control byte is `invalid-uri`. Other syntactically valid schemes and
authorities remain external. Only the emitted `external_scheme` field is ASCII-lowercased because
URI schemes are case-insensitive; every other semantic/raw byte and digest is preserved. Same-
repository GitHub recognition is narrower still: semantic bytes must have exact lowercase
`https://github.com/`, no userinfo or port, and literal unescaped ASCII owner/repository components.
For those two components only, fold `A`–`Z` to ASCII lowercase and compare with the declared
lowercase run-context RepositoryIdentity; the local CLI value is self-asserted, while a future
provider wrapper must authenticate it. Preserve the original URL bytes in `raw_destination_digest`.
Percent-encoded owner/repository, non-ASCII/IDNA components, uppercase host, default-port variants,
and `http` are valid external URLs, never normalized into the trusted identity. This case fold is
the provider's documented case-insensitive owner/repository lookup, not general URI normalization.
UTF-8 remains permitted in repository-relative path components under the separate RepoPath rules.

LFS-pointer detection is bytes-only and does not consult `.gitattributes`, repository/global Git
configuration, or installed LFS filters. A recognized pointer blob is 1–1,023 raw bytes, has no
BOM, decodes as UTF-8, and consists entirely of key/value lines ending in one consistent line
ending. Canonical Git LFS pointers use LF; v0 also recognizes the exact all-CRLF transform as a
defensive non-content classification, while mixed endings are invalid. Every line has one ASCII
key, exactly one separating ASCII space, and a value containing no CR or LF. Keys match
`[a-z0-9.-]+`. The first line is exactly one of:

```text
version https://git-lfs.github.com/spec/v1
version https://hawser.github.com/spec/v1
```

The second alternative is Git LFS's readable legacy pre-release form. After `version`, keys are
strictly increasing by ASCII byte order and therefore unique. The set contains exactly one `oid`
whose value is `sha256:` plus 64 lowercase hexadecimal digits and exactly one `size` whose value is
`0` or a nonzero decimal without a leading zero and is at most 9,223,372,036,854,775,807. Any number
of other sorted keys are extension lines; their values may be empty or contain UTF-8 and spaces,
but not CR or LF. Unknown extension keys are not interpreted, executed, or copied to the report;
the raw pointer digest binds them.

A BOM, mixed newline style, uppercase hash, duplicate/unsorted key, blank/comment line, missing
final line ending, unsupported OID algorithm, out-of-range size, or blob of 1,024 bytes or more
makes the blob ordinary content. A valid extension line does **not**. Conversely an ordinary file
whose complete bytes match this bounded grammar is deliberately treated as a pointer even without
attributes. This conservative false-positive rule is stable, and the raw pointer digest preserves
the exact LF/CRLF encoding. The finite positive/negative corpus is
[`lfs-pointer-v1-vectors.json`](./spec/examples/lfs-pointer-v1-vectors.json).

The same resolver runs independently for every base occurrence against the base snapshot and every
candidate occurrence against the candidate commit/index snapshot. The following total precedence
applies on both sides; the first terminal row supplies that occurrence's sole Resolution
status/code:

1. Parse URI syntax and split semantic query/fragment components. Invalid URI, percent encoding,
   decoded path controls, or fragment encoding terminates as the matching `invalid/*` row before
   lookup.
2. Classify ordinary external/foreign URLs, site-root routes, and scheme-relative network paths.
   For a trusted same-repository GitHub URL, perform the complete ref split, single segment decode,
   and remaining-path `RepoPath` validation specified below before classifying version scope. A
   trusted ref with no remaining path or an invalid remaining path terminates as the corresponding
   invalid row; only a valid default-only path may terminate as `unsupported-version-scope`. A
   semantic destination beginning `//` is exactly
   `unsupported/network-path-unsupported` with unsupported TargetIntent and no invented scheme;
   those terminal boundary rows do not probe the repository.
3. Percent-decode a repository URI path exactly once; reject decoded NUL/control bytes, traversal
   above root, backslash separators, or an encoded slash that would create a path separator. A
   decoded literal `%2F` from `%252F` remains those three literal characters. An empty native
   Markdown destination targets the source document whether or not query/fragment is present;
   otherwise apply the exact native terminal-slash constructor above and resolve relative to the
   source document parent. Normalize `.`/internal `..` only while
   proving containment, convert to
   `RepoPath`, and never strip punctuation/suffixes to find a match.
4. Exact lookup absence is `missing/path-not-found`, regardless of query/fragment.
5. A present symlink or gitlink is its exact unsupported entry-kind row, regardless of
   query/fragment; neither is followed.
6. A present regular blob/tree outside the authored target-kind set is
   `type-mismatch/target-type-mismatch`, regardless of query/fragment.
7. For a compatible entry, establish ordinary/tree/LFS content availability. This is retained even
   if a later semantic boundary becomes the primary code.
8. A present query, empty or nonempty, is accepted only for a compatible scanned Markdown/MDX
   document blob; otherwise terminate as `unsupported/unsupported-query-semantics` while retaining
   the resolved entry fields. An accepted query never participates in filesystem lookup and does
   not suppress fragment evaluation.
9. With no query or an accepted document query, apply fragment semantics: absent/empty fragments
   continue; recognized GitHub line fragments and other non-document code fragments terminate as
   `code-fragment-unevaluated`; renderer-defined document fragments terminate as
   `unsupported-fragment-semantics`. Both boundary codes retain the resolved entry fields.
10. Otherwise emit `resolved/exact-path`.

Consequently `doc.md?x#heading` reaches the unsupported document-fragment row, while
`code.rs?x#symbol` terminates earlier at unsupported query semantics. A producer never treats an
accepted query as permission to skip the independent fragment boundary.

A compatible LFS pointer independently creates the `unsupported-target-kind` content-boundary
Finding even when query/fragment semantics supplies the primary Resolution code. Earlier missing,
special-entry, or type-mismatch rows do not also create query/fragment findings. This precedence is
lossy only by explicit rule and forbids producers from choosing among simultaneous conditions.

### Same-repository GitHub URLs

Owner and repository must equal the trusted run-context repository identity after the exact ASCII
component fold above. Accepted forms are:

```text
https://github.com/<owner>/<repo>/blob/<ref>/<path>
https://github.com/<owner>/<repo>/tree/<ref>/<path>
```

The run context supplies exact full candidate-destination and default-branch refs. Strip
`refs/heads/` and split each trusted UTF-8 ref suffix on literal `/`. Split the URL suffix after
`blob`/`tree` on literal `/`, then percent-decode **each** URL segment exactly once to UTF-8 before
comparison. A `/tree/` form may contain exactly one terminal empty segment after a nonempty path;
remove it before ref matching and RepoPath validation. A `/blob/` terminal slash or any other empty
segment is invalid. Invalid UTF-8/escapes or a nonterminal segment decoding to slash, backslash,
NUL, or control is invalid; `%25` becomes literal `%` and is never decoded again. Compare decoded
segment bytes with the trusted ref segments; never take the first URL segment as the ref. If a
trusted ref consumes the complete suffix, terminate as
`invalid/invalid-reference` with unsupported TargetIntent: accepted `blob`/`tree` forms require one
nonempty `RepoPath`. Otherwise require at least one remaining decoded repository-path segment.
Join every remaining already-decoded path with literal `/` and validate it as `RepoPath` before
deciding whether the matching ref is candidate or default-only. Consequently a default-only suffix
such as `main/../x` is `invalid/path-traversal`, not an unsupported-version result carrying an
impossible path. Exactly one distinct split must match. Candidate and default
refs that are equal produce the same split, not ambiguity. The matching split must be the
candidate-destination ref. If the trusted refs match different splits, or only a different default
ref matches, the URL requests unsupported version scope. Unicode refs and a branch such as
`release/v1` are therefore handled without ambient remote lookup or guessing.

No second percent decode occurs. A full immutable object ID, another branch, tag, short hash,
`HEAD`, or ambiguous-looking ref is unsupported only when it does not exactly equal one of the two
trusted full branch refs; a legitimate trusted branch literally named `HEAD` or 40 hexadecimal
digits still matches by the same segment rule. The scanner never contacts GitHub. Without trusted
repository identity and both refs, every GitHub URL is exactly
`external-out-of-scope/foreign-repository`; it has external scheme `https`, null repository path
and target kind, contributes only to the external bucket, and never becomes an unsupported-version
finding. With trusted identity, an owner/repository mismatch uses the same exact external row;
`unsupported-version-scope` is reserved for an identity match whose ref split is outside the two
trusted destination/default refs.

`blob` requires a regular blob or supported document target. `tree` requires a tree. A type
mismatch is a structural failure.

### Fragments and anchors

- An empty fragment has no anchor check.
- Markdown/MDX heading-anchor validation is not built into scanner v0 because CommonMark does not
  define renderer heading IDs. The path portion is resolved, the fragment is retained, and anchor
  status is `unsupported-reference-semantics`. A future report/engine contract for a renderer must
  name its exact slug algorithm and pass duplicate-heading and renderer fixtures before it may
  emit `explicit-anchor-missing`. V0 has no complete-anchor-coverage request.
- GitHub line-fragment syntax is recognized only on a trusted same-repository `/blob/` intent.
  After one fragment percent-decode, its complete ASCII bytes must match
  `L[1-9][0-9]{0,15}` or `L[1-9][0-9]{0,15}-L[1-9][0-9]{0,15}`; each parsed number
  must be at most 9,007,199,254,740,991 and a range end must be at least its start. Leading zeros,
  lowercase `l`, more digits, suffix bytes, reversed ranges, and a bare `L` are not recognized.
  A recognized value is opaque display metadata: scanner v0 does not validate whether the lines
  exist and does not use the range as identity or impact scope. It therefore terminates as
  `code-fragment-unevaluated` after path lookup rather than silently resolving the fragment.
- For any other nonempty fragment, a target path selected as a document on that side—including
  plain-advisory, built-in-excluded, or policy-included/unsupported documents—uses
  `unsupported-fragment-semantics`. An ordinary nondocument blob or tree, a native relative
  `file#L1`, and a GitHub `/tree/` target use `code-fragment-unevaluated`; none implies symbol
  resolution.
- A percent-decoded fragment containing invalid UTF-8/control bytes is invalid.

### Missing targets and ambiguous syntax

An explicit supported reference with no exact target is `explicit-target-missing`. Stable v0 has
one exact path interpretation, so it has no `explicit-target-ambiguous` finding. Syntactically
ambiguous URI/ref/path input is `invalid-reference` or a typed unsupported capability; the resolver
never probes alternatives and selects the first existing path. An exact directory exists as a tree
but does not imply an index document.

V0 does not evaluate Git ignore rules, generated-looking names, or repository history and emits no
hint based on them. None of those signals can change or annotate the current structural fact; any
future diagnostic requires a new bounded input, output field, ordering law, and compatibility
contract.

## Source blocks and projections

The containing source owner uses an override order, not generic smallest-node selection:

1. the nearest ancestor list item, if any;
2. otherwise the nearest GFM table cell;
3. otherwise the nearest paragraph;
4. otherwise the document root.

`SourceProjectionV1` uses the exact complete source span of that selected owner. An enclosing list
or table/row may be retained only as non-identity human display context. Raw HTML is opaque and can
never own an extracted construct. This makes list-item ownership reachable even when a paragraph
is nested inside it and fixes nested-list/table precedence.

Heading-to-heading sections are reporting groups, not block identity. Frontmatter is not a prose
subject in scanner v0. A reference definition is attributed to each supported consuming prose
construct. An unused definition creates no occurrence or report count; it is only a parser node
charged to the parser-node resource budget.

`SourceProjectionV1` converts CRLF and bare CR to LF and preserves every other source byte,
including punctuation, digits, Unicode form, whitespace, and final-newline presence. It performs
no word-token or formatter normalization.

Machine source spans use zero-based half-open byte offsets into the raw Git blob. Their display
line and column values are one-based Unicode-scalar positions after that same CRLF/bare-CR-to-LF
conversion; tabs count as one scalar and no display-width expansion occurs. Adapters MUST convert
parser-native UTF-16, byte-column, or display-column positions to this contract. A span endpoint
between the CR and LF bytes of one CRLF pair is invalid.

The target projection for one regular file is its Git mode plus raw blob SHA-256 under the
scanner's domain-separated digest contract. Scanner v0 has no AST or semantic normalizer. Because
impact is advisory, formatting and comment changes are allowed to trigger it honestly.

## Observation identity and correlation

Each item receives the `ObservationId` defined by
[normative-core-spec.md](./normative-core-spec.md#23-observation-identifiers). It is a diagnostic
fingerprint, not durable claim identity.

The report embeds the complete `ObservationIdInput` and structural address defined in
[machine-contracts.md](./machine-contracts.md#adapter-observation-and-build-provenance). An adapter
may not substitute a line/column, heading slug, or opaque parser node ID for that input.

Base/candidate correlation has four outcomes:

| Outcome | Minimum evidence | Allowed conclusion |
| --- | --- | --- |
| `exact` | Same adapter contract, document path, construct kind, normalized target intent, duplicate occurrence key, and source projection digest | The same extracted source block/reference bytes exist in both trees |
| `candidate` | Same document/construct/target with an address change (projection equal or changed), or an exact Git document rename with unique unchanged source projection | Equality/change is derived from the projection, never the address |
| `ambiguous` | More than one plausible base or candidate item | Advisory ambiguity; no unchanged/co-change assertion |
| `none` | No plausible counterpart | New, removed, or uncorrelated observation |

Line, column, heading text, Git similarity, and content similarity cannot establish `exact`.
Scanner v0 computes and emits no similarity score, ranking, or suggestion.

“Normalized target intent” in correlation means exactly this internal `CorrelationIntentV1`
projection (it is not a report field or digest):

- `repository-path` and `same-repository-github` both become
  `{class: repository, path: repository_path, target_kind, query_digest, fragment_digest}`;
  `raw_destination_digest` and the native/GitHub origin are omitted, so an escape-only spelling
  change or equivalent origin change can form a candidate edge without becoming exact;
- `external-url` becomes
  `{class: external-url, raw_destination_digest, external_scheme, query_digest, fragment_digest}`;
- `site-route` and `unsupported` become
  `{class: <kind>, raw_destination_digest, query_digest, fragment_digest}`.

The JSON notation fixes keys and value types; equality is exact value equality with null distinct
from a digest and is not implementation hashing. External/site/unsupported raw spelling
deliberately participates because TargetIntent has no complete normalized URI/route value from
which a safer semantic identity could be reconstructed. The adapter-contract digest and source
construct are compared separately as stated below.

The matching algorithm is closed. Occurrence IDs must be unique within each snapshot. First pair
and remove the one base/candidate occurrence for every equal `ObservationId`; these are `exact`.
For the remainder, create a bipartite plausible edge only when adapter contract, source construct,
and normalized target intent agree and either (a) document path agrees or (b) the documents form an
exact Git rename and the source projection agrees. Case (a) may have a changed source projection;
case (b) may not. Compute connected components of this graph in observation-ID byte order:

- one base plus one candidate is `candidate`;
- a component with more than one occurrence on either side is one `ambiguous` comparison;
- an isolated occurrence is one `none` comparison.

For a one-to-one same-document component, equal source-projection digests use
`same-intent-unchanged-projection`/`source_change = equal`; unequal digests use
`same-intent-source-changed`/`changed`. A structural-address change alone can therefore change the
ObservationId without manufacturing a prose-byte change. A one-to-one exact-rename component uses
the rename reason and equal projection row.

For an ambiguous component, the primary base and candidate are the smallest IDs on their respective
sides and every remaining full occurrence appears in the matching `alternatives.base` or
`alternatives.candidate` array, sorted by ID. Exact, candidate, and none comparisons have empty
alternative arrays. Across the report, every extracted occurrence appears exactly once as a
primary or alternative; omission, duplication, ranking-based pruning, and arbitrary primary choice
are malformed output. The comparison array sorts by primary candidate ID, falling back to primary
base ID. Resource exhaustion cannot truncate a component into an apparently unambiguous match; it
is an analysis error and exit 2.

“Exact Git rename” is a scanner term, not Git metadata or similarity. Among unmatched document
paths, a removed base regular blob and an added candidate regular blob are a rename pair only when
their Git mode and raw-evidence digest are equal and that `(mode, raw_digest)` occurs exactly once
on each side. Duplicate-content candidates create no rename edge; they are not tie-broken by path,
history, or similarity.

Correlation fields use this closed table before target comparison:

| Correlation | Reason | Source change | Alternatives | Target change / impact before pair comparison |
| --- | --- | --- | --- | --- |
| `exact` | `same-extraction-key-and-projection` | `equal` | empty | derive below |
| `candidate` | `same-intent-unchanged-projection` | `equal` | empty | derive below |
| `candidate` | `same-intent-source-changed` | `changed` | empty | derive below |
| `candidate` | `exact-document-rename-unchanged-projection` | `equal` | empty | derive below |
| `ambiguous` | `multiple-counterparts` | `unknown` | every non-primary component member | `not-comparable` / `observation-correlation-ambiguous` |
| `none`, candidate only | `new-observation` | `added` | empty | `not-comparable` / `new-observation` |
| `none`, base only | `removed-observation` | `removed` | empty | `not-comparable` / `removed-observation` |

## Base-versus-candidate derivation

For exact/candidate pairs, target comparison uses exactly:

| Resolution comparison | `target_change` | Final impact |
| --- | --- | --- |
| Both resolved/available and target projections equal | `equal` | `subject-changed` only when source is `changed`; otherwise `none` |
| Both missing with equal complete resolution facts, or both type-mismatched with equal complete facts | `equal` | `subject-changed` only when source is `changed`; otherwise `none` |
| Both resolved/available and target projections differ | `changed` | `dependency-and-subject-cochanged` when source is `changed`; otherwise `dependency-changed-subject-unchanged` |
| Base missing/type-mismatched and candidate resolved | `newly-resolved` | `reference-resolved` |
| Base resolved and candidate missing/type-mismatched | `became-missing` | `not-applicable`; the structural finding carries the failure |
| Missing versus type-mismatched, unequal same-status structural facts, resolved content not `available`, or either side external/unsupported/invalid | `not-comparable` | `not-applicable` |

Unavailable comparison evidence never creates an observation row: every such fatal run uses the
empty-detail projection. Consequently `target_change` has no `unknown` value.

`reference-resolved`, `new-observation`, and `removed-observation` are observation-result details,
not additional FindingKinds. `observation-correlation-ambiguous` is a finding because it explicitly
limits the impact conclusion.

This is change impact, not semantic drift. A co-change does not establish review or correctness. An
unchanged source block does not prove the code change matters to its meaning.

Directory/tree targets participate in structural resolution only. Path-set and tree-change impact
requires a named deterministic selector in a later stage.

## Finding taxonomy and stable keys

The authoritative closed enum is in
[scanner-report-v1.schema.json](./spec/scanner-report-v1.schema.json). Its scanner-fact subset is:

Invocation, configuration, Git, discovery, parsing, resolution, resource, output, and internal
failures are bounded `AnalysisError` entries, not findings. They have no finding key, attribution,
disposition, debt/waiver application, or policy trace; any such error makes the report incomplete
and exits 2.

| Finding kind | Evidence class | Maximum v0 disposition |
| --- | --- | --- |
| `explicit-target-missing` | Deterministic structural | `fail` |
| `explicit-target-type-mismatch` | Deterministic structural | `fail` |
| `invalid-reference` | Deterministic structural/schema | `fail` |
| `unsupported-reference-semantics` | Unsupported | Exit 2 when requested as covered; otherwise disclosed |
| `unsupported-capability` | Unsupported requested feature | Exit 2 |
| `unsupported-document-format` | Unsupported requested adapter | Exit 2 when included/protected; otherwise outside built-in scope |
| `unsupported-target-kind` | Unsupported Git object/content boundary | Exit 2 when requested as covered; otherwise disclosed |
| `unsupported-version-scope` | Unsupported scope | Exit 2 when requested; no candidate-tree fallback |
| `dependency-changed-subject-unchanged` | Impact observation | `warn` |
| `dependency-and-subject-cochanged` | Impact observation | `record` |
| `subject-changed` | Impact observation | `record` |
| `explicit-reference-removed` | Coverage observation | `warn` |
| `document-removed` | Coverage observation | Always `record`; protected inventory separately emits blocking `coverage-reduced` |
| `external-out-of-scope` | Disclosed boundary | `record` |
| `opaque-mdx-region` | Coverage limitation | `record`; protected completeness request is unsupported |
| `opaque-html-region` | Coverage limitation | `record`; HTML reference coverage is unsupported |
| `observation-correlation-ambiguous` | Correlation boundary | `record`; no unchanged/co-change assertion |
| `unlinked-document` | Coverage observation | `record` |

### Exact ordinary-finding projection

Finding construction is not producer-selectable. After the complete document/observation arrays
exist, apply these rules and no others:

1. For each `DocumentResult`, emit `document-removed` exactly when base is non-null and candidate is
   null. For a non-null candidate with `status = unsupported`, emit one
   `unsupported-document-format` regardless of whether its exact reason is unsupported format,
   LFS pointer, symlink, or gitlink. For a scanned candidate, emit one `opaque-mdx-region` when its
   MDX region count is positive, one `opaque-html-region` when its HTML count is positive, and the
   already-defined `unlinked-document` exactly when extracted-reference count is zero. Every
   document finding has aggregation member count one; exact opaque region/byte multiplicity remains
   in its embedded `DocumentResult`. A base-only unsupported/opaque state creates no old boundary
   finding beyond `document-removed`.
2. Enumerate every ordinary candidate occurrence (primary plus alternatives). Its current
   Resolution maps exactly: `missing` -> `explicit-target-missing`; `type-mismatch` ->
   `explicit-target-type-mismatch`; `invalid` -> `invalid-reference`; unsupported query, fragment,
   code-fragment, site-route, or network-path code -> `unsupported-reference-semantics`; unsupported version ->
   `unsupported-version-scope`; symlink/gitlink code -> `unsupported-target-kind`; and
   `external-out-of-scope` -> `external-out-of-scope`. A compatible Resolution whose
   `content_availability = lfs-pointer-only` additionally emits `unsupported-target-kind`, including
   when query/fragment is its primary code. No other status/code emits an occurrence boundary.
3. Structural kinds are aggregated independently by FindingKey. Include every candidate structural
   `missing`/`type-mismatch` fact from step 2 and every base `missing`/`type-mismatch` fact, including a
   base-only removed observation. Group all
   included base/candidate occurrences by the canonical key, compute each side's exact
   multiplicity/fact, and emit one finding for every key with at least one included side. This
   yields current, introduced/pre-existing/unknown, and base-only `resolved` projections; those
   resolved projections are forced to record-only policy, so deletion cannot retain an old blocking
   failure. Unsupported and external boundaries are candidate-only and never receive resolved
   projections.
4. Emit `explicit-reference-removed` once for each `correlation = none` comparison with base
   non-null/candidate null, regardless of the former Resolution. Emit
   `observation-correlation-ambiguous` once for each ambiguous comparison. For each comparison,
   emit the finding named by impact only when impact is
   `dependency-changed-subject-unchanged`, `dependency-and-subject-cochanged`, or
   `subject-changed`. `none`, `reference-resolved`, new/removed observation, and `not-applicable`
   impacts emit no impact finding.
5. Reserved governed definitions and control-plane facts use only the separately closed control
   table in machine-contracts. Analysis errors never enter this projection.

Document findings use document scope/path and no observation IDs. `document-removed` has location
`{side: base, path: <document>, span: null}`. Candidate `unsupported-document-format`,
`opaque-mdx-region`, `opaque-html-region`, and `unlinked-document` instead use
`{side: candidate, path: <document>, span: null}`. There is no region-selected or producer-selected
span for an aggregate document fact. Occurrence boundaries use each contributing candidate
ObservationId; comparison removal/correlation/impact uses the primary candidate ID, falling back to
primary base. Global aggregation then applies the one-per-key law, exact member count, complete
sorted observation-ID set, and lowest-location representative. These rules deliberately make a
base-only deleted invalid/unsupported/external reference a removal observation only, while a deleted or
paired repair of a deterministic structural failure retains a resolved audit projection.

The same machine enum contains exactly these v0 control-plane findings: `policy-weakened`,
`coverage-reduced`, `control-plane-changed`, `debt-worsened`, `debt-expired`, and
`waiver-invalid`. They are unsuppressible. Other governed-state and future adapter control names in
the broader core are not scanner-v0 machine kinds. No prose-only alias is a machine kind.

Stable structural finding keys use the exact `FindingKeyInput`, duplicate-eligibility rule, and
domain in [machine-contracts.md](./machine-contracts.md#finding-identity-facts-and-duplicates).
They exclude line, column, heading, human message, and current resolution wording. Changing a
broken target resolves the old key and introduces a new one. Moving a line without changing the
construct does not. Indistinguishable duplicate occurrences cannot receive debt or a waiver.

Observation/impact finding keys use `ObservationId` and therefore may churn. They are not eligible
for adoption debt, waivers, or hard gating in scanner v0.

Deleting an ungoverned document/reference may resolve a structural failure, but the removed
document/reference remains a coverage observation. Only an externally protected inventory can make
that removal an absolute failure. Scanner v0 does not invent a universal “one link per page” quota.

## First run and adoption

A complete run always requires the event-authorized explicit base and a candidate with different
identity. Pull requests, merge groups, and pushes use the provider fields/parent checks in the CI
event table; explicit replay uses two caller-supplied distinct commits and makes no ancestry or
wall-clock claim; index mode uses its declared base commit plus a synthetic candidate.
Setting commit-pair base equal to candidate would mechanically label every reference
`pre-existing` and is therefore invalid, not a bootstrap. An all-zero creation event, root commit
without a representable predecessor, missing object, or otherwise unavailable authorized base
yields an incomplete exit-2 result. There is no complete candidate-only mode.

Bootstrap/remediation is an external rollout operation: remain report-only until a later event has
a valid base, or use an explicit administrator-approved provider bypass while retaining the
incomplete report. No first run can establish pre-existing narrative staleness or review time.

There is no `init` command and no mass baseline. Report-only rollout uses the `observe` profile.
Before `enforce`:

- fix all current supported structural failures; or
- supply the exact externally reviewed, expiring debt snapshot defined in
  [ci-security-spec.md](./ci-security-spec.md#adoption-debt-ratchet).

The base tree alone is not automatic debt authorization. A success summary with zero supported
references says exactly that, not that docs are valid.

## Determinism and output

The deterministic JSON report uses the strict envelope and payload in
[scanner-report-v1.schema.json](./spec/scanner-report-v1.schema.json) and MUST contain:

- report schema and compatibility status;
- engine, scanner-action/release-manifest, and complete adapter contract provenance;
- evaluation mode and verified base/candidate snapshot IDs;
- selected profile and trusted policy/debt/waiver, sandbox, and time-source provenance;
- one sorted document result for every discovered document;
- one sorted reference/observation result for every extracted item;
- source and target resolution facts, embedded observation/finding/fact digest preimages,
  correlation, attribution, classification, and policy trace;
- complete scope/coverage/opaque/unsupported/external/unlinked counts;
- completeness boolean, payload digest, and exit class.

Deterministic output contains no acquisition timestamp, host path, random ID, ANSI escape, commit
author, or commit message. When expiry-bearing external controls exist, the trusted wrapper
supplies an explicit `evaluation_instant` as part of the validity tuple; identical complete tuples
produce identical output. Provider display/acquisition metadata stays outside the payload. Arrays
sort by the exact machine-contract keys.

Human output is a non-wire convenience projection: its prose labels may change with
`engine_version`, but it cannot change facts, ordering, totals, or exit. On a valid human-selected
invocation, stdout is UTF-8 with LF-only lines and stderr is empty once a complete accepted
projection is available. It prints all retained analysis errors and the first 200 findings in the
same canonical order as JSON, followed by exact totals; it prints no source excerpt, raw link
destination, URL userinfo, or query value.

Every repository-derived scalar is rendered by `human-atom-v1`: take at most the first 200 Unicode
scalar values, append literal `...` if any were omitted, then emit a double-quoted ASCII JSON-style
string. Quote and backslash use `\"` and `\\`; printable ASCII U+0020–U+007E is otherwise literal;
every other scalar uses lowercase `\uXXXX` escapes, with a UTF-16 surrogate pair for a non-BMP
scalar. CR, LF, tab, ESC, bidi controls, and ANSI bytes are therefore never active terminal/log
syntax. Raw path-byte hex is treated as an untrusted scalar under the same 200-scalar bound. This
encoding is exact even though surrounding English wording is not a compatibility wire. The
malformed-format invocation's fixed stderr exception is defined in the command section above.

The paper schema and semantic rules are now published, closing only the missing-design portion of
Gate A and authorizing disposable CLI/schema/Git-acquisition scaffolding and the conformance harness.
Parser integration and evaluator implementation are not authorized until the complete pinned-oracle
profile corpus and its extraction/span/address/node/depth goldens are checked in. Gate A's remaining
evidence closure stays open until an independent strict parser, Draft 2020-12 validator,
cross-field validator, negative raw-byte fixtures, X-04/X-05 fixtures, and cross-platform
deterministic-output tests pass. The first implementation labels compatibility `experimental`;
publishing a schema or passing the dossier smoke checker is not producer conformance evidence.

## Performance and caching

Scanner v0 performs one complete bounded scan of each supplied snapshot. It MAY memoize parsed
blobs and target digests inside the process by `(adapter contract, Git mode, raw content digest)`.
It has no persistent cache and does not restore or save a CI cache.

Changed-file lists and reverse maps may reduce recomputation only after the complete discovery set
and control plane are known. They never remove a document or reference from the result denominator.

The security ceilings are correctness limits, not performance targets. Promotion to a required
check needs measured cold runtime, memory, and finding volume on user zero and unrelated
repositories.

## Required scanner tests

In addition to the attack matrix in [ci-security-spec.md](./ci-security-spec.md), scanner v0 MUST
test:

1. every candidate document/include/exclude basename and suffix rule;
2. Markdown inline, full, collapsed, shortcut, image, reference-definition, and autolink
   constructs with exact source spans; raw HTML and heading anchors must exercise the explicit
   unsupported path;
3. MDX frontmatter, ESM, JSX, expressions, templates, comments, and fake links without evaluation;
4. document-relative paths, encoded characters, query, fragments, same-repository GitHub URLs,
   foreign URLs, type mismatches, directories, assets, and site routes;
5. single-decode traversal, NUL, control, backslash, Unicode form, case, and odd path fixtures;
6. symlink, submodule, LFS, skip-worktree, sparse-directory/split-index rejection, nonrepresentable
   paths, unmerged index, concurrent index
   changes, and `--worktree` rejection;
7. exact/candidate/ambiguous/none correlation and every impact table row;
8. a formatting-only target change producing advisory raw impact, not a semantic claim;
9. introduced, pre-existing, resolved, and unknown attribution, plus separate debt-worsened cases;
10. report-only versus enforce profile, protected debt, external floor, and policy-weakening cases;
11. zero documents, zero references, all documents unsupported, opaque-only MDX, and result-limit
    cases with coverage-safe summaries;
12. the two currently broken explicit same-repository GitHub links and representative resolved
    explicit links recorded in [preimpl-experiments.md](./preimpl-experiments.md);
13. byte-identical JSON for the same mode and complete evaluation tuple across repeated runs,
    traversal orders, and supported platforms; across clean index/commit modes, compare policy-free
    document/observation/finding facts for equality while requiring the intentionally different
    snapshot/provenance fields to disclose their mode;
14. repository status/index/ref/object integrity before and after every command;
15. every crash/timeout/limit/partial-output path producing no accepted result and no exit 0.

## Implementation authorization

This specification defines only the discard-state scanner target. Current Gate-A authorization is
narrower: implement the CLI/schema/Git-acquisition scaffold, hostile fixtures, complete parser
corpus, and conformance harness first; parser integration and evaluator work begin only after those
corpus goldens pass. Even after that prerequisite, this specification does not authorize:

- stable governed directives or state writes merely because
  [directive-rfc.md](./directive-rfc.md) exists;
- a committed observation baseline;
- a global lockfile;
- narrative acceptance or lifecycle commands;
- executable/named repository validators;
- external fetching;
- privileged provider automation;
- SARIF, comments, fixes, or agent ranking.

Those capabilities remain separately gated. Scanner v0 succeeds if it produces trustworthy,
bounded evidence and proves whether teams need anything more.
