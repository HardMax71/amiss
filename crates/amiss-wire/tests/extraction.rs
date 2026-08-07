use std::collections::BTreeSet;

use amiss_wire::extraction::{BlockKind, HeadingSource};

/// The two extraction name tables are pinned by distinctness: an emptied
/// projection fails non-emptiness and a constant one fails uniqueness.
#[test]
fn block_and_heading_names_are_nonempty_and_distinct() {
    let blocks = [
        BlockKind::Paragraph,
        BlockKind::ListItem,
        BlockKind::TableCell,
        BlockKind::DocumentRoot,
    ]
    .map(BlockKind::as_str);
    let sources = [
        HeadingSource::Markdown,
        HeadingSource::AsciiDoc,
        HeadingSource::Rst,
        HeadingSource::RawHtml,
    ]
    .map(HeadingSource::as_str);
    for table in [&blocks, &sources] {
        assert!(table.iter().all(|name| !name.is_empty()));
        let unique: BTreeSet<&str> = table.iter().copied().collect();
        assert_eq!(unique.len(), table.len(), "{table:?}");
    }
}
