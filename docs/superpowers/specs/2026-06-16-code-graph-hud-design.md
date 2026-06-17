# Code Graph HUD & Blast Radius — Design

**Date:** 2026-06-16
**Status:** Approved (pending spec review)
**Trigger key:** `gp` (normal mode, `g`-prefix chord — sibling of `gr` = `lsp.references`)

## 1. Goal

When the caret sits on a function/method, pressing `gp` opens a **2D node-graph
overlay** centered over the editor (not a new tab). It shows:

- the focal symbol (center),
- its **callers** (left column) and **callees** (right column) as connected pills,
- a **blast-radius** risk coloring derived from `codegraph impact`,

and lets the user navigate between nodes with **vim `hjkl`**, jump to a node's
`file:line` with `Enter`, and close with `Esc`.

Visual reference: `.superpowers/brainstorm/32009-1781417986/content/code-graph.html`.

## 2. Data source — `codegraph` CLI via async worker (decision: B′)

We shell out to the external `codegraph` CLI (v1.0.1, tree-sitter knowledge
graph) through the existing async worker (Module 05). We do **not** read
`.codegraph/codegraph.db` directly — the SQLite schema is gitignored/transient,
whereas the CLI `--json` output is a stable interface and already computes
`impact` (blast radius) for us.

Commands used (all support `--json`):

| Command | Purpose |
|---|---|
| `codegraph sync` | incremental re-index since last run (fast; run before querying) |
| `codegraph callers <sym> --json --limit N` | direct callers (left column) |
| `codegraph callees <sym> --json --limit N` | direct callees (right column) |
| `codegraph impact <sym> --json --depth 2` | affected set → blast-radius color |
| `codegraph status --json` | health / freshness (optional) |

`impact` JSON shape (verified):
```json
{ "symbol": "...", "depth": 2, "nodeCount": N, "edgeCount": M,
  "affected": [ { "name", "kind", "filePath", "startLine" }, ... ] }
```
`callers`/`callees` return analogous node lists.

### Operational notes (why on-demand, not daemon-reliant)
- The codegraph daemon has a file watcher and auto-syncs, **but** it is started
  and kept alive by an MCP client (Claude Code), listens on a unix socket, and
  **dies after 5 min idle**. It is **not** guaranteed to run when Netherize runs
  standalone. Therefore the editor must refresh on its own.
- codegraph indexes files **on disk** → **unsaved buffer edits are not
  reflected**. The focal symbol (from the live buffer) is current; graph edges
  reflect last-saved state. This is acceptable for v1 and noted in the UI.
- Refresh strategy: run `codegraph sync` (incremental, not full `index`) on open.
  Optionally kick one background `sync` at editor start so first open is instant.

## 3. Flow

On `gp`:
1. Resolve focal symbol name at caret via **tree-sitter** (enclosing
   function/method node). Editor already has tree-sitter.
2. Worker runs `codegraph sync`; HUD shows **"indexing…"** state.
3. Worker runs `callers`, `callees`, `impact` **in parallel**.
4. Results return as `WorkerResultPayload::CodeGraphResult`; `AppShell::on_worker_result`
   builds the graph model and opens the overlay.

**Focal symbol resolution caveat:** `codegraph callers <name>` resolves by name.
If a name collides across overloads/modules, the wrong symbol may be picked.
v1 accepts this; a later refinement disambiguates by matching `file:line` from
`codegraph node <name> --json`.

## 4. Graph model & blast-radius

- **Nodes shown** = center (focal) + direct callers + direct callees (from
  `callers`/`callees`, depth 1).
- **Risk color** is derived from `impact`:
  - **Center** = cyan (`#22ECDB`).
  - **Callers** = at-risk (they depend on focal). Color by presence/distance in
    the `impact` affected set: direct = high (red `#E35535`), deeper = medium
    (amber `#EACD61`).
  - **Callees** = safe (green `#3CEC85`) — focal depends on them, not vice versa.
- The HUD explicitly labels blast radius as an **estimate** (codegraph
  under-reports trait/dynamic-dispatch and macro-generated calls; verified only
  2 traits / 16 `implements` edges in this repo).
- **Per node:** symbol name, `file:line`, risk dot.
- **Tooltip (focused node):** qualified name, kind, `file:line`, role
  (caller/callee), risk label. Signature is **deferred** (would need an extra
  `codegraph node` call; fetch lazily later).

## 5. Layout (variable node counts)

- Three columns: Callers (left) · Center (fixed middle) · Callees (right).
- Each side stacks nodes vertically. **Cap 8 per column**; overflow shows
  "+N more". `j`/`k` scroll within the focused column when capped.

## 6. Rendering (wgpu — reuse existing primitives)

- **Pills** = rounded quads via existing `.with_radius()` (used by sidebar /
  test_runner / terminal). Risk dot = small max-radius quad (approx circle).
- **Focus ring** = static outline quad (no animation in v1).
- **Edges** = thin rotated quads (straight segments); arrowhead = `▸` glyph.
  No bezier / line-shader in v1 (renderer has only quad/glyph/icon/image/caret/
  region shaders; adding a path primitive is deferred).
- **Backdrop** = existing overlay dim over the editor.
- **Top bar** (focal name · blast level · node/edge count · file path · close)
  and **footer** (shortcut hints) = rect + text.

Lives alongside the existing overlay system (`workbench/overlay_manager.rs` +
`render/renderer/editor/overlays.rs`). Add `OverlayKind::CodeGraphHud`.

## 7. Keybindings

- `g p` (normal) → new command `codegraph.open_graph_hud`. Mirror the
  `g r` → `lsp.references` binding block in `config/keymaps/default.toml`.
- Inside the overlay (new input focus context, e.g. `InputFocusContext::CodeGraph`):
  `h j k l` navigate, `Enter` jump to focused node, `Esc` close.
  - `h` left: center→caller, callee→center.
  - `l` right: center→callee, caller→center.
  - `j`/`k`: down/up within the current column.

## 8. Extensions manager integration

Add `codegraph` as an `ExtensionItem` (`src/app/app_state/mod.rs`
`default_extension_items()`):
- `binary: "codegraph"`, category Code Intelligence,
- `macos_install` / `linux_install`: `npm install -g codegraph`,
- detected via the existing `which` sweep; upgrade via `codegraph upgrade`.

When codegraph is **not installed**, the HUD shows a **not-installed empty state**
pointing the user to `<leader>m e` to install it.

## 9. States

- **indexing…** (sync running) · **ready** (graph) · **empty** (no callers/callees)
  · **not-installed** (codegraph missing → install CTA) · **error** (command failed).

## 10. Scope

**In v1:** open/close, callers/callees/impact, `hjkl` nav, jump, blast-radius
color, extension entry, all five states above.

**Deferred (YAGNI):** `o` expand (drill into a node), `t` tests
(`codegraph affected`), bezier edges, animations, signature tooltip, live update
on edit, daemon lifecycle management, name-collision disambiguation.

## 11. Components / seams touched

- `async_runtime/message.rs` — new `WorkerRequestPayload::CodeGraph*` +
  `WorkerResultPayload::CodeGraphResult`.
- `async_runtime/scheduler/` — new module (e.g. `codegraph.rs`) to spawn the CLI
  and parse JSON, dispatched like other external-tool jobs.
- `core/command_ids.rs` + `core/commands.rs` — `codegraph.open_graph_hud`.
- `config/keymaps/default.toml` — `g p` binding.
- `app/app_state/` — graph HUD state + `InputFocusContext::CodeGraph` + extension item.
- `app/input/` — overlay focus handling (`hjkl`/`Enter`/`Esc`).
- `workbench/overlay_manager.rs` — `OverlayKind::CodeGraphHud`, layout build.
- `render/renderer/editor/overlays.rs` — draw pills, edges, focus ring, panels.
- `app/event_loop/` — wire `on_worker_result` → open overlay.

## 12. Testing

- Unit: graph-model builder (callers/callees/impact JSON → nodes + risk colors),
  `hjkl` navigation state machine, column overflow/cap, layout coordinate math
  (mirror the existing `overlay_manager` test style).
- Parse tests against captured `codegraph --json` fixtures (callers/callees/impact).
- Not-installed / empty / error state selection logic.
