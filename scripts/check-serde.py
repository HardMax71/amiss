import re
import sys
from pathlib import Path


RULES = {
    "handwritten Serde implementation": re.compile(
        r"\bimpl\b[^{};]*?\b(?:Serialize|Deserialize|SerializeAs|DeserializeAs|"
        r"DeserializeSeed|Serializer|Deserializer|Visitor|SeqAccess|MapAccess|"
        r"EnumAccess|VariantAccess|SerializeSeq|SerializeTuple|SerializeTupleStruct|"
        r"SerializeTupleVariant|SerializeMap|SerializeStruct|SerializeStructVariant)"
        r"\s*(?:<[^{};]*?>)?\s+for\b"
    ),
    "handwritten Serde callback": re.compile(
        r"\bfn\s+(?:serialize|deserialize|serialize_as|deserialize_as)\s*[<(]"
        r"|\bfn\s+\w+[^{};]*\b(?:Serializer|Deserializer)\b[^{};]*\{"
    ),
    "custom JSON representation guard": re.compile(
        r'\bremote\s*=\s*"Self"|\b(?:with_prefix|serde_conv|forward_to_deserialize_any)!'
        r"|\bdisable_recursion_limit\s*\("
    ),
}

failed = False
for filename in sys.argv[1:]:
    source = Path(filename).read_text()
    for reason, pattern in RULES.items():
        for match in pattern.finditer(source):
            line = source.count("\n", 0, match.start()) + 1
            print(f"{filename}:{line}: {reason}; use Serde derives and library calls")
            failed = True
sys.exit(int(failed))
