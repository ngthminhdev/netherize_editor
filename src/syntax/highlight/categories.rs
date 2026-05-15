#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HighlightCategory {
    Keyword,
    KeywordControl,      // if, for, while, match, return
    KeywordStorage,      // let, const, static, var
    String,
    StringEscape,        // \n, \t, \x00 inside strings
    Comment,
    CommentDoc,          // /// doc comments
    Type,
    TypeBuiltin,         // i32, String, bool (primitive types)
    Function,
    FunctionBuiltin,     // println!, len, push (built-in functions)
    Number,
    Boolean,
    Identifier,
    Variable,
    VariableBuiltin,     // self, this, super
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
            Self::KeywordControl => "keyword.control",
            Self::KeywordStorage => "keyword.storage",
            Self::String => "string",
            Self::StringEscape => "string.escape",
            Self::Comment => "comment",
            Self::CommentDoc => "comment.doc",
            Self::Type => "type",
            Self::TypeBuiltin => "type.builtin",
            Self::Function => "function",
            Self::FunctionBuiltin => "function.builtin",
            Self::Number => "number",
            Self::Boolean => "boolean",
            Self::Identifier => "identifier",
            Self::Variable => "variable",
            Self::VariableBuiltin => "variable.builtin",
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
        matches!(self, Self::Macro | Self::MarkupStrong | Self::KeywordControl)
    }

    pub fn is_italic(self) -> bool {
        matches!(self, Self::Comment | Self::CommentDoc | Self::MarkupItalic | Self::MarkupLink)
    }

    pub(crate) fn priority(self) -> u8 {
        match self {
            // Narrow but expressive captures should win over the generic fallback.
            Self::MarkupStrong => 130,
            Self::MarkupItalic => 128,
            Self::MarkupInlineCode => 126,
            Self::MarkupLink => 124,
            Self::CommentDoc => 122,
            Self::Comment => 120,
            Self::StringEscape => 118,
            Self::Escape => 115,
            Self::Macro => 110,
            Self::String => 100,
            Self::Lifetime => 95,
            Self::Attribute => 93,
            Self::KeywordControl => 92,
            Self::KeywordStorage => 91,
            Self::Keyword => 90,
            Self::Boolean => 88,
            Self::FunctionBuiltin => 87,
            Self::Function => 85,
            Self::Constructor => 84,
            Self::Constant => 83,
            Self::Parameter => 80,
            Self::Field => 78,
            Self::Property => 76,
            Self::Namespace => 74,
            Self::Tag => 73,
            Self::TypeBuiltin => 72,
            Self::Type => 71,
            Self::Number => 68,
            Self::VariableBuiltin => 45,
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
    pub keyword_control: [u8; 4],
    pub keyword_storage: [u8; 4],
    pub string: [u8; 4],
    pub string_escape: [u8; 4],
    pub comment: [u8; 4],
    pub comment_doc: [u8; 4],
    pub ty: [u8; 4],
    pub type_builtin: [u8; 4],
    pub function: [u8; 4],
    pub function_builtin: [u8; 4],
    pub number: [u8; 4],
    pub boolean: [u8; 4],
    pub identifier: [u8; 4],
    pub variable: [u8; 4],
    pub variable_builtin: [u8; 4],
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
            // Keywords - warm yellow/orange tones
            keyword: [234, 205, 97, 255],           // Warm yellow
            keyword_control: [255, 120, 117, 255],  // Coral red (if, for, while, return)
            keyword_storage: [255, 180, 84, 255],   // Orange (let, const, var)

            // Strings - green tones
            string: [60, 236, 133, 255],            // Bright green
            string_escape: [255, 200, 87, 255],     // Golden yellow for escapes

            // Comments - muted blue/gray
            comment: [74, 94, 132, 255],            // Muted blue-gray
            comment_doc: [95, 135, 175, 255],       // Brighter blue for doc comments

            // Types - purple tones
            ty: [183, 138, 255, 255],               // Purple
            type_builtin: [220, 160, 255, 255],     // Lighter purple (i32, String)

            // Functions - cyan/blue tones
            function: [105, 195, 255, 255],         // Cyan
            function_builtin: [130, 220, 255, 255], // Brighter cyan (println!, len)

            // Numbers and booleans - orange/red tones
            number: [227, 85, 53, 255],             // Red-orange
            boolean: [255, 149, 92, 255],           // Orange

            // Variables - neutral tones
            identifier: [208, 215, 228, 255],       // Light gray
            variable: [208, 215, 228, 255],         // Light gray
            variable_builtin: [255, 203, 107, 255], // Golden (self, this, super)

            // Properties and fields - cyan tones
            parameter: [34, 236, 219, 255],         // Bright cyan
            field: [105, 195, 255, 255],            // Cyan
            property: [208, 215, 228, 255],         // Light gray

            // Constants - orange
            constant: [255, 149, 92, 255],          // Orange

            // Operators and punctuation - muted tones
            operator: [175, 187, 210, 255],         // Light blue-gray
            punctuation: [129, 150, 181, 255],      // Muted blue-gray

            // Special - orange/yellow
            escape: [255, 149, 92, 255],            // Orange
            macro_name: [105, 195, 255, 255],       // Cyan
            lifetime: [255, 149, 92, 255],          // Orange

            // Structural - purple
            constructor: [183, 138, 255, 255],      // Purple
            attribute: [234, 205, 97, 255],         // Yellow
            namespace: [183, 138, 255, 255],        // Purple
            tag: [183, 138, 255, 255],              // Purple
        }
    }
}

impl HighlightPalette {
    pub fn color_for(self, category: HighlightCategory) -> [u8; 4] {
        match category {
            HighlightCategory::Keyword => self.keyword,
            HighlightCategory::KeywordControl => self.keyword_control,
            HighlightCategory::KeywordStorage => self.keyword_storage,
            HighlightCategory::String => self.string,
            HighlightCategory::StringEscape => self.string_escape,
            HighlightCategory::Comment => self.comment,
            HighlightCategory::CommentDoc => self.comment_doc,
            HighlightCategory::Type => self.ty,
            HighlightCategory::TypeBuiltin => self.type_builtin,
            HighlightCategory::Function => self.function,
            HighlightCategory::FunctionBuiltin => self.function_builtin,
            HighlightCategory::Number => self.number,
            HighlightCategory::Boolean => self.boolean,
            HighlightCategory::Identifier => self.identifier,
            HighlightCategory::Variable => self.variable,
            HighlightCategory::VariableBuiltin => self.variable_builtin,
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
