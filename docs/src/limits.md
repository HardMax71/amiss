# Limits and refusals

Every ceiling is a number in the contract, and crossing one is a typed error carrying the
resource name, the configured limit, and the observed lower bound. Nothing hangs, nothing
truncates silently, and an incomplete analysis is always exit 2 with the crossing retained in
the report's errors.

The scan-side contract:

| resource | limit |
| --- | --- |
| documents per snapshot | 100,000 |
| document blob bytes | 4 MiB |
| aggregate document bytes per snapshot | 512 MiB |
| raw link destination bytes | 16 KiB |
| parser nesting | 256 |
| parser nodes per document | 250,000 |
| parser nodes per snapshot | 5,000,000 |
| references per document | 4,096 |
| references per snapshot | 1,000,000 |
| referenced target blob bytes | 16 MiB |
| aggregate referenced target bytes | 512 MiB |
| complete findings | 100,000 |
| retained errors | 64 |

And the object-store side:

| resource | limit |
| --- | --- |
| inflated object bytes | 128 MiB |
| compressed stream bytes | 256 MiB |
| aggregate compressed bytes | 2 GiB |
| pack files | 4,096 |
| pack index bytes | 512 MiB |
| aggregate pack index bytes | 1 GiB |
| delta depth | 128 |
| index file bytes | 256 MiB |
| tree entries per snapshot | 1,000,000 |
| raw path bytes | 4,096 |

Count resources observe exactly one past the limit and stop. Per-value byte resources observe
the declared value. An aggregate observes the prior charged total plus the first crossing
member, and a member rejected by its own per-value limit is never charged to the aggregate,
so the numbers in a crossing are always reconstructible.

Refusals follow one rule: when the input cannot be trusted, there is no result to trust
either. A base object the store does not hold, a tracked blob missing from the store, an
index with an unmerged entry, a document whose bytes do not decode, a path the report format
cannot represent, a JSON control input with a duplicate key: each is a named error code in
the report (`GIT_OBJECT_MISSING`, `DOCUMENT_INVALID`, `UNREPRESENTABLE_PATH`, and the rest of
the closed set), and each ends the run at exit 2. The alternative in every case is a report
that looks complete and is not.
