use tree_sitter::{Node, Parser, Tree};

use crate::syntax::parser::tree_sitter_language;

/// Ngôn ngữ parser mà SyntaxEngine đang xử lý.
/// Registry có thể ánh xạ nhiều file extension vào cùng một parser family
/// (ví dụ JavaScript + JSX cùng grammar, TypeScript + TSX cùng grammar).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LanguageId {
    Rust,
    JavaScript,
    Jsx,
    TypeScript,
    Tsx,
    Go,
    Yaml,
    Dockerfile,
    Json,
    Bash,
}

impl LanguageId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::JavaScript => "javascript",
            Self::Jsx => "jsx",
            Self::TypeScript => "typescript",
            Self::Tsx => "tsx",
            Self::Go => "go",
            Self::Yaml => "yaml",
            Self::Dockerfile => "dockerfile",
            Self::Json => "json",
            Self::Bash => "bash",
        }
    }
}

/// Tree state gắn chặt với revision của buffer/app-state tại thời điểm parse.
#[derive(Debug)]
pub struct SyntaxTreeState {
    tree: Tree,
    language_id: LanguageId,
    revision: u64,
}

impl SyntaxTreeState {
    fn new(tree: Tree, language_id: LanguageId, revision: u64) -> Self {
        Self {
            tree,
            language_id,
            revision,
        }
    }

    pub fn tree(&self) -> &Tree {
        &self.tree
    }

    pub fn root_node(&self) -> Node<'_> {
        self.tree.root_node()
    }

    pub fn language_id(&self) -> LanguageId {
        self.language_id
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }
}

/// Wrapper duy nhất cho tree-sitter parser.
/// App layer chỉ nói chuyện với abstraction này thay vì gọi parser API trực tiếp.
pub struct SyntaxEngine {
    parser: Parser,
    language_id: LanguageId,
    current_tree: Option<SyntaxTreeState>,
}

impl SyntaxEngine {
    pub fn new(language_id: LanguageId) -> Result<Self, String> {
        let mut parser = Parser::new();
        let language = tree_sitter_language(language_id).ok_or_else(|| {
            format!(
                "tree-sitter language '{}' is not available in this build",
                language_id.as_str()
            )
        })?;
        parser.set_language(&language).map_err(|err| {
            format!(
                "set tree-sitter language '{}' failed: {err}",
                language_id.as_str()
            )
        })?;

        Ok(Self {
            parser,
            language_id,
            current_tree: None,
        })
    }

    pub fn new_rust() -> Result<Self, String> {
        Self::new(LanguageId::Rust)
    }

    /// Full reparse — always correct, O(file size).
    /// Use for first parse, language switch, or when edit coordinates are unavailable.
    pub fn parse_source(
        &mut self,
        source: &str,
        revision: u64,
    ) -> Result<&SyntaxTreeState, String> {
        let tree = self
            .parser
            .parse(source, None)
            .ok_or_else(|| "tree-sitter parser returned None tree".to_string())?;

        self.current_tree = Some(SyntaxTreeState::new(tree, self.language_id, revision));
        self.current_tree
            .as_ref()
            .ok_or_else(|| "internal parser state missing after parse".to_string())
    }

    /// Incremental reparse — O(changed region).
    ///
    /// Calls `tree.edit()` with the supplied byte ranges to update stale node
    /// positions, then passes the edited tree to `parser.parse()` so tree-sitter
    /// only re-parses the affected region.
    ///
    /// Row/column in the `InputEdit` is approximated from the *new* text; this is
    /// acceptable because our highlight system uses byte ranges exclusively.
    ///
    /// Falls back to `parse_source` when no previous tree exists.
    pub fn parse_incremental(
        &mut self,
        source: &str,
        start_byte: usize,
        old_end_byte: usize,
        new_end_byte: usize,
        revision: u64,
    ) -> Result<&SyntaxTreeState, String> {
        if self.current_tree.is_none() {
            return self.parse_source(source, revision);
        }

        let edit = tree_sitter::InputEdit {
            start_byte,
            old_end_byte,
            new_end_byte,
            start_position: byte_to_point(source, start_byte),
            old_end_position: byte_to_point(source, old_end_byte),
            new_end_position: byte_to_point(source, new_end_byte),
        };

        // Update old tree's node positions in-place (required before passing as hint).
        if let Some(state) = self.current_tree.as_mut() {
            state.tree.edit(&edit);
        }

        // Parse incrementally: tree-sitter reuses unchanged nodes.
        let new_tree = {
            let old_tree = self.current_tree.as_ref().map(|s| &s.tree);
            self.parser
                .parse(source, old_tree)
                .ok_or_else(|| "tree-sitter incremental parse returned None".to_string())?
        };

        self.current_tree = Some(SyntaxTreeState::new(new_tree, self.language_id, revision));
        self.current_tree
            .as_ref()
            .ok_or_else(|| "internal state missing after incremental parse".to_string())
    }

    pub fn current_tree(&self) -> Option<&SyntaxTreeState> {
        self.current_tree.as_ref()
    }

    pub fn language_id(&self) -> LanguageId {
        self.language_id
    }
}

/// Compute a tree-sitter `Point` (row, column in bytes) for the given byte offset.
/// Scans the text up to `byte_offset`, counting newlines.  Used to build the
/// `InputEdit` required by `parse_incremental`; accuracy is only needed for
/// tree-sitter's internal node-position bookkeeping, not for our highlight output.
fn byte_to_point(text: &str, byte_offset: usize) -> tree_sitter::Point {
    let clamped = byte_offset.min(text.len());
    // Walk back to the nearest UTF-8 character boundary.
    let boundary = (0..=clamped)
        .rev()
        .find(|&i| text.is_char_boundary(i))
        .unwrap_or(0);
    let prefix = &text[..boundary];
    let row = prefix.bytes().filter(|&b| b == b'\n').count();
    let col = boundary - prefix.rfind('\n').map(|i| i + 1).unwrap_or(0);
    tree_sitter::Point { row, column: col }
}

#[cfg(test)]
mod tests {
    use super::{LanguageId, SyntaxEngine};

    #[test]
    fn rust_parser_bootstrap_returns_source_file_root() {
        let mut engine = SyntaxEngine::new_rust().expect("init rust parser");
        let state = engine
            .parse_source("fn main() { let x = 1; }", 1)
            .expect("parse source");

        assert_eq!(state.language_id().as_str(), "rust");
        assert_eq!(state.revision(), 1);
        assert_eq!(state.root_node().kind(), "source_file");
    }

    #[test]
    fn parse_updates_tree_lifecycle_with_new_revision() {
        let mut engine = SyntaxEngine::new_rust().expect("init rust parser");
        let _ = engine.parse_source("fn first() {}", 1).expect("parse rev1");
        let _ = engine
            .parse_source("fn second() {}", 2)
            .expect("parse rev2");

        let current = engine.current_tree().expect("have current tree");
        assert_eq!(current.revision(), 2);
        assert_eq!(current.root_node().kind(), "source_file");
    }

    #[test]
    fn javascript_parser_bootstrap_returns_program_root() {
        let mut engine = SyntaxEngine::new(LanguageId::JavaScript).expect("init js parser");
        let state = engine
            .parse_source("function greet(name) { return name; }", 1)
            .expect("parse javascript");

        assert_eq!(state.language_id().as_str(), "javascript");
        assert_eq!(state.root_node().kind(), "program");
    }

    #[test]
    fn typescript_parser_bootstrap_returns_program_root() {
        let mut engine = SyntaxEngine::new(LanguageId::TypeScript).expect("init ts parser");
        let state = engine
            .parse_source("type User = { name: string };", 1)
            .expect("parse typescript");

        assert_eq!(state.language_id().as_str(), "typescript");
        assert_eq!(state.root_node().kind(), "program");
    }

    #[test]
    fn go_parser_bootstrap_returns_source_file_root() {
        let mut engine = SyntaxEngine::new(LanguageId::Go).expect("init go parser");
        let state = engine
            .parse_source("package main\n\nfunc main() {}\n", 1)
            .expect("parse go");

        assert_eq!(state.language_id().as_str(), "go");
        assert_eq!(state.root_node().kind(), "source_file");
    }
}
