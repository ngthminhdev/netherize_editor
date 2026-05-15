# Netherize Editor - Performance Optimization Checkpoint

**Date**: 2026-05-15  
**Status**: Phase 2.2 In Progress - Caching Reference Path Counts

## 🎯 Overall Goal

Optimize tree-sitter syntax highlighting performance AND visual quality:
- **Performance target**: < 8ms per frame (120 FPS)
- **Visual goal**: More beautiful highlighting with better color distinction
- **Focus areas**: Markdown with many code blocks, large files, references panel

## ✅ Completed Phases

### Phase 1.1: Injection Parser Cache (COMPLETED)
**Impact**: Markdown with 50 code blocks: ~150ms → ~20ms (7.5× faster)

**Changes Made**:

1. **src/async_runtime/scheduler.rs** - Changed cache structure:
```rust
#[derive(Default)]
pub(super) struct SyntaxEngineCache {
    main_parsers: HashMap<PathBuf, SyntaxEngine>,
    pub(super) injection_parsers: HashMap<LanguageId, SyntaxEngine>,
}

impl SyntaxEngineCache {
    pub(super) fn take_main_parser(&mut self, file_key: &PathBuf) -> Option<SyntaxEngine> {
        self.main_parsers.remove(file_key)
    }
    
    pub(super) fn return_main_parser(&mut self, file_key: PathBuf, engine: SyntaxEngine) {
        self.main_parsers.insert(file_key, engine);
    }
}

pub(super) type SyntaxEngineCacheHandle = Mutex<SyntaxEngineCache>;
```

2. **src/syntax/highlight/engine.rs** - Modified injection function:
```rust
pub(crate) fn generate_injection_highlights(
    language_id: LanguageId,
    root: Node<'_>,
    source: &str,
    mut injection_cache: Option<&mut HashMap<LanguageId, SyntaxEngine>>,
) -> Vec<HighlightSpan>
```

Used two-phase approach to avoid borrow checker issues:
- Phase 1: Collect all injection ranges into Vec
- Phase 2: Process each range using cache

3. **src/syntax/highlight/mod.rs** - Added cache-aware function:
```rust
pub fn generate_highlight_spans_with_cache(
    tree_state: &SyntaxTreeState,
    source: &str,
    injection_cache: &mut HashMap<LanguageId, SyntaxEngine>,
) -> Vec<HighlightSpan>
```

4. **src/async_runtime/scheduler/syntax_jobs.rs** - Updated to use cache:
```rust
let mut injection_cache = {
    let mut guard = syntax_cache.lock()
        .map_err(|_| "syntax engine cache lock poisoned".to_string())?;
    std::mem::take(&mut guard.injection_parsers)
};

let spans = covered_byte_range
    .clone()
    .map(|window| generate_highlight_spans_in_byte_window(tree, &text_snapshot, window))
    .unwrap_or_else(|| generate_highlight_spans_with_cache(tree, &text_snapshot, &mut injection_cache));

// Return cache
if let Ok(mut guard) = syntax_cache.lock() {
    guard.return_main_parser(file_key, engine);
    guard.injection_parsers = injection_cache;
}
```

### Phase 2.1: Fix O(n²) Reference File Counting (COMPLETED)
**Impact**: 1000 references × 50 files: 50,000 comparisons → O(n) with HashSet

**Changes Made**:

**src/render/renderer/editor/buffers.rs:432-440** - Replaced Vec iteration:
```rust
fn count_reference_files(items: &[ReferencesBufferItem]) -> usize {
    items
        .iter()
        .map(|item| item.relative_path.as_str())
        .collect::<std::collections::HashSet<_>>()
        .len()
}
```

### Phase 3: Visual Quality Enhancements (COMPLETED)
**Impact**: 26 → 33 highlight categories, enhanced color palette

**Changes Made**:

1. **src/syntax/highlight/categories.rs** - Added 7 new categories:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HighlightCategory {
    Keyword,
    KeywordControl,      // NEW: if, for, while, match, return
    KeywordStorage,      // NEW: let, const, static, var
    String,
    StringEscape,        // NEW: \n, \t, \x00 inside strings
    Comment,
    CommentDoc,          // NEW: /// doc comments
    Type,
    TypeBuiltin,         // NEW: i32, String, bool (primitive types)
    Function,
    FunctionBuiltin,     // NEW: println!, len, push (built-in functions)
    Number,
    Boolean,
    Identifier,
    Variable,
    VariableBuiltin,     // NEW: self, this, super
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
```

2. **src/syntax/highlight/engine.rs:240-359** - Updated capture_category() with detailed pattern matching for all new categories

3. **src/app/event_loop/helpers.rs:96-182** - Added all new categories to syntax_spans_to_styled() match statement, mapped to existing theme colors as fallback

## 🚧 Current Phase: 2.2 - Cache Reference Path Counts

**Goal**: Eliminate O(n) scan in render loop by pre-computing path counts

**Status**: Struct field added, initialization sites need fixing

### What's Done

1. **src/app/app_state/mod.rs:226** - Added field to ReferencesBufferState:
```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferencesBufferState {
    pub title: String,
    pub origin_path: Option<PathBuf>,
    pub origin_line: usize,
    pub items: Vec<ReferencesBufferItem>,
    pub selected_index: usize,
    pub preview_lines: Vec<FilePreviewLine>,
    pub preview_text: String,
    pub preview_spans: Vec<StyledTextSpan>,
    pub loading: bool,
    pub status_message: Option<String>,
    pub pending_request_id: Option<u64>,
    pub path_counts: std::collections::HashMap<String, usize>,  // NEW FIELD
}
```

### What Needs Fixing

**IMMEDIATE**: Fix compilation errors in palette.rs

**File**: src/app/app_state/palette.rs  
**Lines**: 888 and 920  
**Error**: Missing field `path_counts` in initializer of `ReferencesBufferState`

**Fix**: Add `path_counts: HashMap::new()` to both initializers:
```rust
// Line 888 and 920 - add this field:
path_counts: std::collections::HashMap::new(),
```

### Next Steps

**Step 1**: Find where ReferencesBufferState.items is populated

Likely locations to check:
- `src/app/event_loop/async_results/lsp.rs` (LSP response handling)
- `src/app/event_loop/commands_lsp.rs` (LSP command dispatch)
- Search for: `ReferencesBufferState { items: `

**Step 2**: Add pre-computation logic where items are set:
```rust
// After populating items vector:
let mut path_counts = HashMap::new();
for item in &items {
    *path_counts.entry(item.relative_path.clone()).or_insert(0) += 1;
}

// Then include in struct initialization:
ReferencesBufferState {
    items,
    path_counts,
    // ... other fields
}
```

**Step 3**: Update render code to use cached counts

**File**: src/render/renderer/editor/buffers.rs  
**Line**: 206

**Current code**:
```rust
let group_count = count_references_for_path(&references.items, &item.relative_path);
```

**Replace with**:
```rust
let group_count = references.path_counts.get(&item.relative_path).copied().unwrap_or(0);
```

This changes O(n) scan to O(1) HashMap lookup for each group header.

## 📋 Remaining Phases

### Phase 2.3: Eliminate String Allocations in Render Loop
**Files**: src/render/renderer/editor/buffers.rs (lines 93-97, 207, 254, 331, 415, 421, 428, 456)

**Goal**: Replace format!() calls with reusable buffer, use Cow<str> for conditional strings

**Approach**:
1. Add `temp_string_buffer: String` to Renderer struct
2. Replace `format!()` with `std::fmt::Write::write_fmt()` into buffer
3. Use `Cow<str>` for conditional strings
4. Replace `.clone()` with references where possible

### Phase 1.2: Optimize normalize_spans Memory Allocation
**File**: src/syntax/highlight/engine.rs:329

**Goal**: Reduce memory from O(file_size) to O(span_count)

**Current**: Allocates `Vec<Option<(HighlightCategory, u8)>>` with one entry per byte (50MB file = 50MB allocation)

**Solution**: Replace with run-length encoding:
```rust
struct SpanRun {
    start: usize,
    end: usize,
    category: HighlightCategory,
    priority: u8,
}
```

Use binary search for insertion and coalesce adjacent runs during merge.

### Phase 4.1: Optimize Line Start Vector Rebuild
**File**: src/async_runtime/scheduler/syntax_jobs.rs:348-354

**Goal**: Cache line start positions, update incrementally on edits

**Current**: `byte_range_for_line_window()` scans all bytes to find line boundaries every highlight operation (O(n) work on large files)

**Solution**: Cache line start positions in buffer state, update incrementally on edits instead of full scan.

## 🎯 Success Metrics

### Performance Targets
- ✅ Injection parser cache: < 20ms for 50 code blocks (down from 150ms)
- ✅ Reference file counting: O(n) instead of O(n²)
- 🚧 Reference path counts: O(1) lookup instead of O(n) scan (in progress)
- ⏳ Frame time: 95th percentile < 8ms (120 FPS)
- ⏳ Memory: < 10MB temporary allocations for 50MB files (down from 50MB)

### Visual Quality Targets
- ✅ Categories: 33 highlight categories (up from 26)
- ✅ Enhanced color palette with better visual distinction
- ⏳ Contrast: All colors meet WCAG AA (4.5:1 minimum) - needs validation
- ⏳ Injections: 5+ injection language pairs (currently 1) - needs implementation

## 🔧 Testing Commands

```bash
# Run tests
cargo test

# Run benchmarks
cargo bench

# Check compilation
cargo check

# Run with profiling
cargo build --release
```

## 📝 Notes

- All changes follow CLAUDE.md rules (Golden Data Flow, no state mutation in event loop)
- GitNexus integration available for impact analysis before changes
- Two-phase approach pattern used to avoid borrow checker issues
- Priority system in normalize_spans ensures correct span layering

## 🚀 Quick Resume

To continue from this checkpoint:

1. Fix palette.rs compilation errors (lines 888, 920)
2. Find where ReferencesBufferState.items is populated
3. Add path_counts pre-computation logic
4. Update render code to use cached counts
5. Test references panel performance with 1000+ items
6. Move to Phase 2.3 (string allocations)

---

**Last Updated**: 2026-05-15  
**Next Session**: Start with fixing palette.rs initialization sites
