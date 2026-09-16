mod markdown;
mod mdx;

pub(crate) use markdown::from_markdown;
pub(crate) use mdx::from_mdast;

/// The one tree the sweep reads, whichever grammar built it. Spans are byte
/// offsets into the post-frontmatter suffix.
pub(crate) struct Node {
    pub(crate) kind: Kind,
    pub(crate) span: (usize, usize),
    pub(crate) children: Vec<Node>,
}

pub(crate) enum Kind {
    Root,
    Paragraph,
    Heading,
    ListItem,
    TableCell,
    Html,
    Mdx {
        expression: Option<String>,
    },
    /// A JSX element, with the tag name as written and the literal value of
    /// its `id` attribute.
    MdxElement {
        name: Option<String>,
        id: Option<String>,
    },
    Text(String),
    InlineCode(String),
    CodeBlock(String),
    Link {
        url: String,
    },
    Image {
        url: String,
    },
    LinkReference(Reference),
    ImageReference(Reference),
    Definition(Definition),
    Other,
}

#[derive(Clone, Copy)]
pub(crate) enum ReferenceForm {
    Full,
    Collapsed,
    Shortcut,
}

/// `key` is the span start of the winning definition, so every reference to
/// one label meets at one key however each spells it.
pub(crate) struct Reference {
    pub(crate) key: usize,
    pub(crate) form: ReferenceForm,
}

pub(crate) struct Definition {
    pub(crate) key: usize,
    pub(crate) label: String,
    pub(crate) url: String,
    pub(crate) title: Option<String>,
}

impl Node {
    const fn leaf(kind: Kind, span: (usize, usize)) -> Self {
        Self {
            kind,
            span,
            children: Vec::new(),
        }
    }
}
