use serde_json::Value;

/// Bitmask nhỏ gọn của các tính năng mà LSP server khai báo trong `InitializeResult`.
/// Được parse một lần khi server khởi động và lưu trong `LspSessionHandle`.
/// Mỗi handler (hover, completion, …) phải check flag tương ứng trước khi gửi request,
/// tránh spam và timeout vô ích khi server không hỗ trợ tính năng đó.
#[derive(Debug, Clone, Default)]
pub struct ServerCapabilities {
    pub hover: bool,
    pub completion: bool,
    /// Ký tự trigger autocomplete (vd: `.`, `::`, `(` cho Rust).
    pub completion_trigger_chars: Vec<char>,
    /// Server có hỗ trợ `completionItem/resolve` để bổ sung docs/detail không.
    pub completion_resolve: bool,
    pub definition: bool,
    pub references: bool,
    pub document_highlight: bool,
    pub document_symbols: bool,
    pub document_formatting: bool,
    pub rename: bool,
    pub code_action: bool,
    pub inlay_hint: bool,
}

impl ServerCapabilities {
    /// Parse từ object `capabilities` trong `InitializeResult`.
    /// Một capability được coi là "hỗ trợ" khi field tồn tại và không phải null/false.
    pub fn from_json(caps: &Value) -> Self {
        let supported = |key: &str| -> bool {
            match caps.get(key) {
                None | Some(Value::Null) | Some(Value::Bool(false)) => false,
                _ => true,
            }
        };

        let completion_trigger_chars = caps
            .pointer("/completionProvider/triggerCharacters")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter_map(|entry| {
                let mut chars = entry.chars();
                let ch = chars.next()?;
                chars.next().is_none().then_some(ch)
            })
            .collect();

        let completion_resolve = caps
            .pointer("/completionProvider/resolveProvider")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        Self {
            hover: supported("hoverProvider"),
            completion: supported("completionProvider"),
            completion_trigger_chars,
            completion_resolve,
            definition: supported("definitionProvider"),
            references: supported("referencesProvider"),
            document_highlight: supported("documentHighlightProvider"),
            document_symbols: supported("documentSymbolProvider"),
            document_formatting: supported("documentFormattingProvider"),
            rename: supported("renameProvider"),
            code_action: supported("codeActionProvider"),
            inlay_hint: supported("inlayHintProvider"),
        }
    }
}
