#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HighlightCategory {
    Keyword,
    String,
    Comment,
    Type,
    Function,
    Number,
    Boolean,
    Identifier,
    Variable,
    Parameter,
    Field,
    Property,
    Constant,
    Operator,
    Punctuation,
    Escape,
    Macro,
    Lifetime,
    Constructor,
    Attribute,
    Namespace,
    Tag,
    MarkupStrong,
    MarkupItalic,
    MarkupInlineCode,
    MarkupLink,
}

impl HighlightCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Keyword => "keyword",
            Self::String => "string",
            Self::Comment => "comment",
            Self::Type => "type",
            Self::Function => "function",
            Self::Number => "number",
            Self::Boolean => "boolean",
            Self::Identifier => "identifier",
            Self::Variable => "variable",
            Self::Parameter => "parameter",
            Self::Field => "field",
            Self::Property => "property",
            Self::Constant => "constant",
            Self::Operator => "operator",
            Self::Punctuation => "punctuation",
            Self::Escape => "escape",
            Self::Macro => "macro",
            Self::Lifetime => "lifetime",
            Self::Constructor => "constructor",
            Self::Attribute => "attribute",
            Self::Namespace => "namespace",
            Self::Tag => "tag",
            Self::MarkupStrong => "markup.strong",
            Self::MarkupItalic => "markup.italic",
            Self::MarkupInlineCode => "markup.raw.inline",
            Self::MarkupLink => "markup.link.text",
        }
    }

    pub fn is_bold(self) -> bool {
        matches!(self, Self::Macro | Self::MarkupStrong)
    }

    pub fn is_italic(self) -> bool {
        matches!(self, Self::Comment | Self::MarkupItalic | Self::MarkupLink)
    }

    pub(crate) fn priority(self) -> u8 {
        match self {
            // Narrow but expressive captures should win over the generic fallback.
            Self::MarkupStrong => 130,
            Self::MarkupItalic => 128,
            Self::MarkupInlineCode => 126,
            Self::MarkupLink => 124,
            Self::Comment => 120,
            Self::Escape => 115,
            Self::Macro => 110,
            Self::String => 100,
            Self::Lifetime => 95,
            Self::Attribute => 93,
            Self::Keyword => 90,
            Self::Boolean => 88,
            Self::Function => 85,
            Self::Constructor => 84,
            Self::Constant => 83,
            Self::Parameter => 80,
            Self::Field => 78,
            Self::Property => 76,
            Self::Namespace => 74,
            Self::Tag => 73,
            Self::Type => 72,
            Self::Number => 68,
            Self::Variable => 42,
            Self::Identifier => 40,
            Self::Operator => 20,
            Self::Punctuation => 10,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct HighlightPalette {
    pub keyword: [u8; 4],
    pub string: [u8; 4],
    pub comment: [u8; 4],
    pub ty: [u8; 4],
    pub function: [u8; 4],
    pub number: [u8; 4],
    pub boolean: [u8; 4],
    pub identifier: [u8; 4],
    pub variable: [u8; 4],
    pub parameter: [u8; 4],
    pub field: [u8; 4],
    pub property: [u8; 4],
    pub constant: [u8; 4],
    pub operator: [u8; 4],
    pub punctuation: [u8; 4],
    pub escape: [u8; 4],
    pub macro_name: [u8; 4],
    pub lifetime: [u8; 4],
    pub constructor: [u8; 4],
    pub attribute: [u8; 4],
    pub namespace: [u8; 4],
    pub tag: [u8; 4],
}

impl Default for HighlightPalette {
    fn default() -> Self {
        Self {
            keyword: [234, 205, 97, 255],
            string: [60, 236, 133, 255],
            comment: [74, 94, 132, 255],
            ty: [183, 138, 255, 255],
            function: [105, 195, 255, 255],
            number: [227, 85, 53, 255],
            boolean: [255, 149, 92, 255],
            identifier: [208, 215, 228, 255],
            variable: [208, 215, 228, 255],
            parameter: [34, 236, 219, 255],
            field: [105, 195, 255, 255],
            property: [208, 215, 228, 255],
            constant: [255, 149, 92, 255],
            operator: [175, 187, 210, 255],
            punctuation: [129, 150, 181, 255],
            escape: [255, 149, 92, 255],
            macro_name: [105, 195, 255, 255],
            lifetime: [255, 149, 92, 255],
            constructor: [183, 138, 255, 255],
            attribute: [234, 205, 97, 255],
            namespace: [183, 138, 255, 255],
            tag: [183, 138, 255, 255],
        }
    }
}

impl HighlightPalette {
    pub fn color_for(self, category: HighlightCategory) -> [u8; 4] {
        match category {
            HighlightCategory::Keyword => self.keyword,
            HighlightCategory::String => self.string,
            HighlightCategory::Comment => self.comment,
            HighlightCategory::Type => self.ty,
            HighlightCategory::Function => self.function,
            HighlightCategory::Number => self.number,
            HighlightCategory::Boolean => self.boolean,
            HighlightCategory::Identifier => self.identifier,
            HighlightCategory::Variable => self.variable,
            HighlightCategory::Parameter => self.parameter,
            HighlightCategory::Field => self.field,
            HighlightCategory::Property => self.property,
            HighlightCategory::Constant => self.constant,
            HighlightCategory::Operator => self.operator,
            HighlightCategory::Punctuation => self.punctuation,
            HighlightCategory::Escape => self.escape,
            HighlightCategory::Macro => self.macro_name,
            HighlightCategory::Lifetime => self.lifetime,
            HighlightCategory::Constructor => self.constructor,
            HighlightCategory::Attribute => self.attribute,
            HighlightCategory::Namespace => self.namespace,
            HighlightCategory::Tag => self.tag,
            HighlightCategory::MarkupStrong => self.keyword,
            HighlightCategory::MarkupItalic => self.comment,
            HighlightCategory::MarkupInlineCode => self.string,
            HighlightCategory::MarkupLink => self.function,
        }
    }
}
