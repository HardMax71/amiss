# amiss-wire

The shared typed machine contracts of Amiss: report, control, request, and evidence models
with their validation rules. Serde owns JSON conversion, `serde_json_canonicalizer` owns
RFC 8785 output, and RustCrypto owns hashing. A Serde visitor enforces the wire's unique
keys, safe integers, and nesting limit without building another JSON representation.

Part of [Amiss](https://hardmax71.github.io/amiss/).
