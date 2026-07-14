# Limits and refusals

Every internal ceiling is a published number, and crossing one produces a typed error
carrying the resource name, the configured limit, and the value that crossed it. Nothing
hangs and nothing is quietly cut short; a run that could not finish its analysis always
exits 2 with the crossing recorded in the report's errors.

The scanning ceilings:

| resource | limit |
| --- | --- |
| documents per snapshot | 100,000 |
| bytes per document | 4 MiB |
| document bytes per snapshot | 512 MiB |
| bytes per link destination | 16 KiB |
| parser nesting depth | 256 |
| parser nodes per document | 250,000 |
| parser nodes per snapshot | 5,000,000 |
| references per document | 4,096 |
| references per snapshot | 1,000,000 |
| bytes per referenced target | 16 MiB |
| referenced target bytes per snapshot | 512 MiB |
| findings per complete run | 100,000 |
| serialized report bytes | 64 MiB |
| analysis errors kept | 64 |

And the Git-reading ceilings:

| resource | limit |
| --- | --- |
| bytes per inflated object | 128 MiB |
| bytes per compressed stream | 256 MiB |
| compressed bytes per snapshot | 2 GiB |
| pack files | 4,096 |
| bytes per pack index | 512 MiB |
| pack index bytes total | 1 GiB |
| delta chain depth | 128 |
| index file bytes | 256 MiB |
| tree entries per snapshot | 1,000,000 |
| bytes per path | 4,096 |

The charging rules keep every reported number reconstructible. Counters stop exactly one
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

Refusals follow one rule: when the input cannot be trusted, no output is produced to trust
either. A base commit the store does not hold, a tracked file whose object is missing, an
index with an unresolved merge conflict, a document whose bytes will not decode, a path the
report cannot represent, a control file with a duplicated JSON key: each has a named error
code (`GIT_OBJECT_MISSING`, `DOCUMENT_INVALID`, `UNREPRESENTABLE_PATH`, and the rest of a
closed list), and each ends the run at exit 2. The alternative in every one of these cases
is a report that looks complete and is not.
