# Fetch LeetCode Problem Design

## Goal

Add a command-palette workflow that accepts a LeetCode problem ID, title slug,
or URL, downloads the official starter snippet and examples, creates a runnable
solution file, fills the Test Runner, and leaves the editor ready for F5.

## User Flow

1. `Fetch LeetCode Problem` opens a text-input palette.
2. The user enters an ID, slug, or full LeetCode URL and confirms.
3. A supported active-file extension selects the language automatically.
4. Otherwise the existing MRU-sorted LeetCode language picker opens.
5. The async worker resolves the slug, fetches GraphQL problem data, extracts
   examples, and adapts the official snippet.
6. The result handler creates `solution.ext` (or `solution-N.ext`) in the
   workspace root. It falls back to the active file directory, then
   `$TMPDIR/netherize-leetcode`.
7. The editor opens the file, rescans the workspace, replaces Test Runner cases,
   focuses the Test Runner, and shows an F5-ready toast.

## Architecture

The command follows the existing palette and command-dispatch flow. Palette
confirmation submits a typed `LeetCodeFetchRequest` through the worker channel.
All network access, JSON parsing, HTML example extraction, and optional AI work
runs under `tokio::spawn`; the UI thread only applies the completed result.

`runner/leetcode_api.rs` owns input normalization, GraphQL DTOs, problem parsing,
and example extraction. `runner/leetcode_adapter.rs` owns mechanical templates
and the AI prompt/response contract. The scheduler coordinates those modules and
falls back to mechanical adaptation when AI fails.

## Configuration

`AiConfig` gains a LeetCode toggle. During the initial implementation, enabled
LeetCode adaptation reuses `inline_completion.provider`; the adapter boundary
allows a dedicated stronger provider to be introduced later for generated test
cases and explanations without changing the command workflow.

```toml
[leetcode]
use_ai = false
```

The Settings AI section exposes a `LeetCode AI` toggle. Disabled is the default.

## Error Handling

Invalid input, unknown problem IDs, GraphQL errors, missing language snippets,
and network failures return a worker failure and do not create a file. AI errors
are non-fatal and use the mechanical adapter. Expected-output parsing failure
keeps the input case and uses an empty expected value.

## Testing

Unit tests cover ID/slug/URL normalization, GraphQL parsing, metadata-to-JSON
inputs, HTML output extraction, snippet selection, mechanical templates, config
defaults, palette transitions, and result application. Verification includes
targeted tests, `cargo check`, rustfmt, and diff checks.
