# Limits and refusals

The report has a closed set of named resource ceilings. Crossing a snapshot ceiling produces
a typed error carrying the wire resource name, configured limit, and observed lower bound,
and a run that cannot complete exits 2. A per-document ceiling costs that one document
instead, as the refusal rules further down set out. The table is rendered from the Rust
defaults and checked in CI, so a default cannot change without updating this page.

These are accounting ceilings, not all wall-clock deadlines. Document bytes are charged
before parsing, parser node and nesting totals after the grammar returns, and
embedded-code evaluation bytes inside the parse itself, at every candidate close of an
MDX code region. How the ceilings relate to CPU, and which lanes carry wall-clock
watchdogs, is described in [Security model](security.md).

Line-fragment work is charged pessimistically: the complete target size, once per
distinct target identity (path, file mode, and object id) and numeric range. Successful
and out-of-range results are cached, so repeated identical anchors do not multiply the
charge. A changed object or mode at the same path is charged again.

Heading-anchor work is charged the same way and by the same rule, once per target identity
rather than per anchor, because the identities every known renderer would publish are built
in one parse of the target and then answered from memory. This is the only place the engine
parses a file it did not discover as a document. A target the budget cannot afford is not
judged: its anchors stay unsupported rather than becoming missing.

Projection work has separate snapshot totals for admitted assertions, selected source bytes,
records prepared for comparison, canonical projected bytes, and diagnostic preview bytes.
Repeated assertions spend these totals even when their source blob is cached: caching avoids a
second Git read, but it does not make the projection comparison free. Preview bytes are charged
before they are copied into a finding; rows omitted by the fixed preview bound cost no report
memory and are represented by their exact omitted count.

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
| `ignore-declaration-blob-bytes` | 1,048,576 |
| `aggregate-ignore-declaration-bytes-per-snapshot` | 16,777,216 |
| `aggregate-line-fragment-evaluation-bytes-per-snapshot` | 536,870,912 |
| `aggregate-heading-anchor-evaluation-bytes-per-snapshot` | 536,870,912 |
| `projection-assertions-per-snapshot` | 10,000 |
| `aggregate-projection-selected-bytes-per-snapshot` | 67,108,864 |
| `projection-records-compared-per-snapshot` | 200,000 |
| `aggregate-projection-projected-bytes-per-snapshot` | 67,108,864 |
| `aggregate-projection-preview-bytes-per-snapshot` | 16,777,216 |
| `aggregate-document-bytes-per-snapshot` | 83,886,080 |
| `raw-link-destination-bytes` | 16,384 |
| `parser-nesting` | 256 |
| `parser-nodes-per-document` | 250,000 |
| `parser-nodes-per-snapshot` | 5,000,000 |
| `aggregate-embedded-code-evaluation-bytes-per-snapshot` | 536,870,912 |
| `references-per-document` | 16,384 |
| `references-per-snapshot` | 1,000,000 |
| `declared-labels-per-snapshot` | 1,000,000 |
| `organization-policy-entries` | 100,000 |
| `complete-findings` | 100,000 |
| `typed-analysis-errors-retained` | 64 |
| `machine-json-bytes` | 268,435,456 |
| `private-temporary-storage-bytes` | 67,108,864 |
| `evaluator-managed-memory-bytes` | 1,073,741,824 |
<!-- amiss-doc-contract:limits:end -->

Three of these ceilings have been measured against real repositories rather than reasoned
about. `references-per-document` was 4,096 until fastapi's release notes came in at 7,075
references in one auto-generated changelog; the next largest documents measured anywhere are
just's and helix's changelogs, at about 2,900 and still growing. `machine-json-bytes` moved
for the same reason and against the same repositories. A 64 MiB reservation refused both of the largest
documentation sets measured: fastapi serializes 1,664 documents and 15,334 references to
78 MB, and Docusaurus's own repository 1,543 documents and 23,035 references to 111 MB. At
256 MiB both pass, and the value stays a quarter of the memory ceiling bounding the same
process.

Raising it treated a symptom whose cause has since been removed. Ninety-one percent of
fastapi's report was one finding per external URL, each carrying a verbatim copy of an
observation row the report already held and already named by id. An external reference is now
an observation and nothing else. Declined references went the same way later: a row per declined
reference, each with its own copy of that reference's observation, was 58% of MDN's content
report, and those rows now fold to one per document and kind. fastapi serializes 29 MB and
Docusaurus 41 MB, both of which would have fitted the old reservation. What the reservation buys now is headroom rather
than admission. `complete-findings` allows 100,000 findings, and only the leanest finding this
engine builds fits a hundred thousand times under the reservation. A broken-link finding carries
the evidence of its occurrences and weighs several kilobytes, so a flood of those meets the
reservation first, at some tens of thousands of findings, and ends as `OUTPUT_LIMIT_EXCEEDED`.
Either ceiling ends the run incomplete, and neither ever ships a findings array cut short.

`aggregate-document-bytes-per-snapshot` is the third, and it was held at 33,554,432 by an
ordering bug rather than by a measurement. The report used to be spelled into memory before
its size was checked, so a document set whose report would overrun `machine-json-bytes` died
in the allocator instead of naming the ceiling it crossed, and the document total was the
only counter that fired early enough to prevent that. The size check now counts the report
as it is written to a sink, so an overrun ends the run at exit 2 with `OUTPUT_LIMIT_EXCEEDED`
and the document budget is free to answer its own question. At 83,886,080 the Kubernetes
website scans: 8,232 documents and 70,743,496 bytes of them, 68,408 references, a 139 MB
report, and a 519 MB peak against the 1 GiB address space. MDN's content repository scans too,
14,649 documents in a 182 MB report at a 631 MB peak. Before the fold it crossed the report
ceiling at 333,847,393 bytes.

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

Refusals follow one rule: when the run cannot be trusted, no complete pass is produced.
The machine report records the refusal and exit class 2. A base commit the store does not
hold, a tracked file whose object is missing, an index with an unresolved merge conflict, a
name outside the path grammar, a control file with a duplicated JSON key: each has a named
error code (`GIT_OBJECT_MISSING`, `UNREPRESENTABLE_PATH`, and the rest of a closed list),
and each ends the run at exit 2. A name that is merely not UTF-8 is not on that list: it is
an ordinary document whose path the report writes as hex. The alternative in every one of
these cases is a report that looks complete and is not.

One file is not the run. A document whose bytes will not decode as its format requires, one
whose markup its grammar rejects, and one that crosses one of the five per-document ceilings
say nothing about the rest of the tree, so none of them ends the run. Such a document is
unsupported: it is counted in the summary, named in the human output, and its report row
carries `undecodable-document`, `unparsable-document` or `resource-ceiling-crossed`. An `unsupported-document-format` finding records that its
references went unchecked. What the old refusal protected still holds. The file is never
quietly skipped, its references are never counted as checked, and a repository policy that
protects that path fails the run through `coverage-reduced`.

The five per-document ceilings are `document-blob-bytes`, `raw-link-destination-bytes`,
`parser-nesting`, `parser-nodes-per-document`, and `references-per-document`. Every other
ceiling on this page bounds a whole snapshot or the run itself, so crossing one is a refusal.

The codes themselves live on [Analysis errors](errors.md), one fixed sentence each saying
what happened and what to do about it. Two of them are this page's own:
`RESOURCE_LIMIT_EXCEEDED` for a crossing and `OUTPUT_LIMIT_EXCEEDED` for a report that would
not fit.
