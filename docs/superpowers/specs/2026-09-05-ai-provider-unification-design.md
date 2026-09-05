# AI provider unification + inline completion quality — design

Date: 2026-09-05. Status: approved (user delegated design decisions:
"tự quyết đi, đừng hỏi tôi nữa").

## Problem

- Three separate `[<feature>.provider]` blocks (inline, leetcode, rerank), each
  with its own url/key/model/endpoint_kind. The user's real file had all three
  keys equal to the localhost-proxy key while `[leetcode.provider]` pointed at
  OpenRouter → 401 on every LeetCode generation; the model id
  `mistral/mistral-large-latest` does not exist on OpenRouter either.
- `candidate_paths()` read `./config/ai.toml` (cwd) BEFORE the user config and
  `save_user_override()` wrote to the first existing file — so Settings edits
  made while running from the repo were committed into the repo (with the key).
- Inline completion prompt was a plain "Prefix:/Suffix:" chat message with no
  stop sequences and temperature 0.2. Weak models echo the prefix, wrap in
  fences, or run on for several statements; mid-line completions spilled onto
  new lines ("kì lắm").

## Design

### One endpoint, per-feature model (`ai.toml`)

```toml
[provider]                          # shared, OpenAI-compatible (OpenRouter)
api_url = "https://openrouter.ai/api/v1"
api_key = ""

[inline_completion]
enabled = true
model = "mistralai/codestral-2508"
# reasoning_effort defaults to "none" for inline/rerank, unset for leetcode

[leetcode]
use_ai = true
model = "mistralai/mistral-large-2512"

[completion_rerank]
enabled = false
model = "mistralai/codestral-2508"
```

`AiConfig::resolve(feature) -> Option<AiProviderConfig>` merges: a legacy
`[<feature>.provider]` block with a non-empty `api_url` still overrides the
shared endpoint (old files keep working); `model` comes from the feature's
`model` key, else the legacy block; `reasoning_effort` from the feature key,
else the legacy block, else the per-feature default. `endpoint_kind` only
supports a custom `/path` now; `"responses"` is gone (OpenAI chat format only).

Load order becomes user config first (`~/.config/netherize/config/ai.toml`,
then `~/.config/netherize/ai.toml`), repo `./config/ai.toml` last as the dev
fallback. Saves always go to the user path. `save_user_override` is a no-op
under `cfg!(test)`.

### Shared client (`scheduler/ai_client.rs`)

`chat(provider, system, user, ChatOptions)` builds the body, adds auth, sends,
decodes, maps HTTP errors to `error.message`; `extract_content` returns the
content or a diagnostic (reasoning ate the budget / truncated). Reasoning is
translated per host: OpenRouter gets `reasoning: {enabled:false}` for `"none"`
or `reasoning: {effort}`; other hosts get the legacy `reasoning_effort`.
OpenRouter inline requests also pass `provider: {sort: "latency"}`.
`list_models(url, key)` GETs `/models` and parses the OpenAI shape plus
OpenRouter's pricing/context/`supported_parameters` when present.

Inline, rerank, LeetCode generate/verify/adapt all go through it (four copies
of the request code deleted).

### Inline completion prompt

Copilot-style FIM-in-chat: the file is sent as `{prefix}<|cursor|>{suffix}`
with a system prompt that demands only the inserted text, no echo, no fences,
stop at a natural boundary. `temperature = 0`. Mode by caret position:

| caret | stop | max_tokens |
|---|---|---|
| text after caret on the same line | `["\n"]` | `min(cfg, 64)` |
| end of line | `["\n\n"]` (one block) | cfg |

The sanitizer additionally strips a leaked `<|cursor|>` marker and truncates a
single-line completion at its first newline.

### Model picker

Settings → "Inline Model" / "LeetCode Model" → Enter opens
`CommandPaletteMode::AiModelPicker`: the worker fetches `/models` from the
shared endpoint (`WorkerRequestPayload::AiListModels`, topic `AiModels`);
rows are `id` + `$in/$out per M · ctx` (OpenRouter) fuzzy-filtered; Enter
writes the model for the pending target and toasts. Failure (no key, offline)
toasts the error and falls back to the plain text edit of the row.

Settings AI rows: Inline Completion (toggle), Inline Model (picker), LeetCode
AI (toggle), LeetCode Model (picker), AI Endpoint, AI API Key, and the four
inline tuning numbers. Removed rows: endpoint kind, LeetCode url/key/kind/
reasoning (config-only now).

### Recommended OpenRouter models (catalog checked 2026-09-05)

- Inline: `mistralai/codestral-2508` (purpose-built completion model, no
  reasoning, $0.30/$0.90 per M). Cheaper: `qwen/qwen3-coder-next`
  ($0.12/$0.80) or `qwen/qwen3-coder-flash`.
- LeetCode: `mistralai/mistral-large-2512` ($0.50/$1.50, no reasoning; user
  already validated mistral-large for expected-output generation). Stronger:
  `openai/gpt-5.4-mini` with `reasoning_effort = "low"`.

## Testing

- `ai_config`: resolve merges shared + model; legacy block wins on url; per-
  feature reasoning default; leetcode disabled when missing.
- `ai_client`: body shape (stop/reasoning per host), `extract_content`
  diagnostics, `parse_models_response` (batch/image filtering, prices).
- `ai.rs`: prompt contains marker + suffix; single-line detection; stop sets.
- sanitizer: single-line truncation, marker strip.
- settings: rows present, Enter on a model row opens the picker (loading).

## Round 2 (same night): ghost text ⟷ completion menu coexistence

**Symptom:** with a valid key the endpoint answered in ~1.2 s (verified with the
app's exact body), yet no ghost text ever showed in the editor.

**Root cause:** the completion menu auto-opens on every identifier keystroke
(all LSP languages) and the inline pipeline treated "LSP always wins" as a
hard rule: `queue`/`flush` cancelled while `has_completion()`, both result
arms dropped the suggestion, and an LSP result arriving while ghost text was
visible returned early. In any file with an LSP the ghost text lost that race
on every keystroke.

**Model now (Cursor / Windsurf):**

| situation | behaviour |
|---|---|
| menu open, AI result arrives | ghost text renders on the caret line, menu stays open |
| ghost visible, LSP result arrives | menu opens over the ghost text |
| **Tab** | accepts the ghost text (handler intercept before the menu's Tab intercept); keymap Tab = `editor.insert_tab` when no ghost |
| **Enter** | accepts the menu item; if the inserted text is the ghost's head, the remainder stays as ghost (`post()` under `post().then(…)` → `.then(…)`), else the ghost is cleared and a fresh suggestion is queued |
| Ctrl+j / Ctrl+l | full / one-word accept, unchanged |
| Ctrl+Space | manual completion still dismisses ghost text and cancels in-flight AI |
| full accept | queues the next suggestion (chaining) |

Streaming chunks accumulate in `ai_inline_stream_buffer` and the whole buffer
is re-sanitized on every chunk, so an echoed indent/prefix never flashes.
The context window is cut on line boundaries (`inline_context_window`). The
three-strike cooldown toast includes the provider error text.

### Neighbouring-tab context (round 2b)

`[inline_completion] neighbor_files = 1`, `neighbor_chars = 1200`: the tabs
nearest the active one with the same extension contribute the head of their
file (imports + signatures, cut on a line boundary) as a reference-only block
before the current file in the prompt. Costs ≈ +300 tokens per request; `0`
disables it. Verified live: codestral answers a mid-line `axios.post(|)` with
`'/users', payload` in 1.2 s for $0.00006.

## Round 3 (morning): inline ghost text, rewrite suggestions

Two screenshots from the user: (1) a mid-line ghost drawn ON TOP of the text
after the caret, (2) `await new Promies.` + Tab producing
`await new Promies.await new Promise(…)` — the model re-emitted the corrected
line and the sanitizer inserted it verbatim. Ask: "allow Tab để fix nhanh".

**Rendering.** `rebuild_layout_projection` takes `caret_tail_shift_x`: glyphs
at/after the caret on the caret's own visual row (not wrapped continuation
rows) move right by the width of the ghost's first line, measured in the
overlay text system before projection (`inline_suggestion_first_line_width`,
stored in `Renderer::inline_ghost_tail_shift`). Diagnostic underlines on that
row follow. Multi-line ghosts only occur when the caret line has no
non-blank tail (single-line policy), so pushing the tail of the first line
covers every case.

**Sanitizer (`InlineEdit { text, replace_before_caret }`).** After the
existing full-prefix echo strip:

| model output vs line prefix | result |
|---|---|
| starts with the whole line prefix (trimmed or not) | echo stripped (as before) |
| starts with a *tail* of the prefix — `P` → `Promise(`, `foo.` → `.bar()` | `echoed_tail_len`: longest such tail stripped |
| shares ≥ 3 leading chars and at least one whole token, then differs — `await new Promies.` vs `await new Promise(` | **rewrite**: `replace_before_caret = "Promies."`, ghost = `Promise(…` |
| shares chars only inside an identifier — `const ` vs `config = 1` | plain insertion (the boundary backs off to a token edge) |

A rewrite renders the replaced span with an error tint + strike-through and
the ghost after the caret; **Tab** deletes the span and inserts the ghost in
one undo transaction. Typing never retains a rewrite; Ctrl+l (word accept)
applies it whole; a menu Enter treats it as "no ghost". While streaming,
`inline_stream_may_echo` holds a buffer that is still a prefix of some tail
of the line (`awa` of `await new P`) so it never flashes as ghost text.

**Diagnostics-aware prompt.** `caret_line_diagnostic_messages(3)` (errors +
warnings on the caret line, whitespace-normalised, ≤ 200 chars) is sent in
the request and listed as `Diagnostics on the caret line:` before the code;
the system prompt tells the model to rewrite the line from its first
non-blank char when the flagged mistake sits before the caret. When an LSP
publish adds a *new* message on the caret line while in insert mode and no
rewrite is showing, `requeue_ai_inline_for_new_diagnostic` queues exactly one
fresh request — that is how the typo fix appears after the user pauses
(`tsserver` publishes ~0.5 s after the edit, later than the debounce). Cost:
at most one extra request per new caret-line error.

### Round 4 (late morning): menu covers the ghost, auto-pair closers consumed

Two more screenshots. (1) A two-line ghost printed *through* the completion
menu: ghost glyphs and the menu share one overlay batch (all chrome quads,
then all glyphs), so the menu's opaque background could not hide them.
`draw_completion_menu` already returns its rect; the overlay now drops every
ghost glyph that touches it — the menu wins, as in VS Code.
(2) `console.error('|');` + ghost `Error during graceful shutdown', err);`
accepted to `…', err);');`. The model's output was fine: it closed the string
and the call itself, but the auto-paired `');` after the caret stayed.
`InlineEdit.replace_after_caret` now counts the leading closers of the
caret-line suffix (`)]}'"\`;,`) the completion makes redundant —
`consumed_closers`: a bracket whose opens/closes balance over kept prefix +
completion, a quote whose count is even, a `;`/`,` the completion ends with;
stop at the first closer that is still needed. Copilot-style output that
stops before the closers (`…', err`) consumes only the quote. While the
ghost shows, those closers are hidden (`CaretTail { shift_x, hidden_bytes }`
in the projection) so the row reads exactly as it will after Tab; Tab
deletes them, then the rewrite span, then inserts — one undo step.

