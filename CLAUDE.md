## Project Rules Precedence
- If this repository/workspace contains `CLAUDE.md`, `.clinerules`, or another agent rule file, agents must read and comply with those rules as project-level instructions.
- These rules supplement system/developer instructions and must be applied consistently across all tasks in this repository.
- Before implementing code changes, check whether project rule files exist and follow them.

# RTK - Rust Token Killer

**Usage**: Token-optimized CLI proxy for shell commands.

## Rule

Always prefix shell commands with `rtk` to minimize token consumption.

Examples:

rtk git status
rtk cargo test
rtk ls src/
rtk grep "pattern" src/
rtk find "*.rs" .
rtk docker ps
rtk gh pr list

## Meta Commands

rtk gain              # Show token savings
rtk gain --history    # Command history with savings
rtk discover          # Find missed RTK opportunities
rtk proxy <cmd>       # Run raw (no filtering, for debugging)

## Why

RTK filters and compresses command output before it reaches the LLM context, saving 60-90% tokens on common operations. Always use `rtk <cmd>` instead of raw commands.

# NETHERIZE EDITOR - CORE ARCHITECTURE & DATA FLOW RULES

You are an expert Rust developer assisting in building **Netherize Editor**, a high-performance, 0-latency, keyboard-first text editor written 100% in Rust.

## 🚫 STRICT ANTI-PATTERNS (NEVER DO THESE)
1. **NO WEB TECH:** Do not use HTML, CSS, DOM, Flexbox, or WebGL.
2. **NO STATE MUTATION IN EVENT LOOP:** Keyboard events must NEVER directly mutate the editor buffer or state.
3. **NO BLOCKING MAIN THREAD:** Never run heavy tasks (JSON parsing, File I/O, LSP, Tree-sitter) on the UI thread. Always use `tokio::spawn` and communicate via `mpsc::channel`.
4. **NO PANIC:** Never use `.unwrap()` or `.expect()` in render loops, async workers, or tree-sitter AST traversals. Handle errors gracefully with `anyhow::Result`.

## 🏗️ THE GOLDEN DATA FLOW (MEMORIZE THIS)
For any input-to-action feature, you MUST follow this exact path:
`application.rs` -> `app/input/handler.rs` -> `app/input_map/mod.rs` -> `app/resolved_keymap.rs` -> `app/event_loop/commands.rs` -> `core/command_dispatch.rs` -> `app/app_state.rs`

## 🧠 SYSTEM RESPONSIBILITIES
* **AppState (`app_state.rs`):** The central source of truth for text, cursor, mode, buffers, and transactions.
* **Command (`commands.rs`):** All possible editor actions must be defined here as an Enum.
* **ModeState (`mode.rs`):** Vim-style mode FSM. Validate all mode transitions here.
* **CommandDispatch (`command_dispatch.rs`):** The ONLY place where commands are allowed to mutate editor state and group undo transactions.

<!-- gitnexus:start -->
# GitNexus — Code Intelligence

This project is indexed by GitNexus as **netherize_editor** (6034 symbols, 15878 relationships, 300 execution flows). Use the GitNexus MCP tools to understand code, assess impact, and navigate safely.

> If any GitNexus tool warns the index is stale, run `npx gitnexus analyze` in terminal first.

## Always Do

- **MUST run impact analysis before editing any symbol.** Before modifying a function, class, or method, run `gitnexus_impact({target: "symbolName", direction: "upstream"})` and report the blast radius (direct callers, affected processes, risk level) to the user.
- **MUST run `gitnexus_detect_changes()` before committing** to verify your changes only affect expected symbols and execution flows.
- **MUST warn the user** if impact analysis returns HIGH or CRITICAL risk before proceeding with edits.
- When exploring unfamiliar code, use `gitnexus_query({query: "concept"})` to find execution flows instead of grepping. It returns process-grouped results ranked by relevance.
- When you need full context on a specific symbol — callers, callees, which execution flows it participates in — use `gitnexus_context({name: "symbolName"})`.

## Never Do

- NEVER edit a function, class, or method without first running `gitnexus_impact` on it.
- NEVER ignore HIGH or CRITICAL risk warnings from impact analysis.
- NEVER rename symbols with find-and-replace — use `gitnexus_rename` which understands the call graph.
- NEVER commit changes without running `gitnexus_detect_changes()` to check affected scope.

## Resources

| Resource | Use for |
|----------|---------|
| `gitnexus://repo/netherize_editor/context` | Codebase overview, check index freshness |
| `gitnexus://repo/netherize_editor/clusters` | All functional areas |
| `gitnexus://repo/netherize_editor/processes` | All execution flows |
| `gitnexus://repo/netherize_editor/process/{name}` | Step-by-step execution trace |

## CLI

| Task | Read this skill file |
|------|---------------------|
| Understand architecture / "How does X work?" | `.claude/skills/gitnexus/gitnexus-exploring/SKILL.md` |
| Blast radius / "What breaks if I change X?" | `.claude/skills/gitnexus/gitnexus-impact-analysis/SKILL.md` |
| Trace bugs / "Why is X failing?" | `.claude/skills/gitnexus/gitnexus-debugging/SKILL.md` |
| Rename / extract / split / refactor | `.claude/skills/gitnexus/gitnexus-refactoring/SKILL.md` |
| Tools, resources, schema reference | `.claude/skills/gitnexus/gitnexus-guide/SKILL.md` |
| Index, status, clean, wiki CLI commands | `.claude/skills/gitnexus/gitnexus-cli/SKILL.md` |
| Work in the App_state area (438 symbols) | `.claude/skills/generated/app-state/SKILL.md` |
| Work in the Event_loop area (263 symbols) | `.claude/skills/generated/event-loop/SKILL.md` |
| Work in the Terminal area (139 symbols) | `.claude/skills/generated/terminal/SKILL.md` |
| Work in the Workbench area (113 symbols) | `.claude/skills/generated/workbench/SKILL.md` |
| Work in the App area (110 symbols) | `.claude/skills/generated/app/SKILL.md` |
| Work in the Lsp area (106 symbols) | `.claude/skills/generated/lsp/SKILL.md` |
| Work in the Syntax area (102 symbols) | `.claude/skills/generated/syntax/SKILL.md` |
| Work in the Input area (101 symbols) | `.claude/skills/generated/input/SKILL.md` |
| Work in the Command_dispatch area (92 symbols) | `.claude/skills/generated/command-dispatch/SKILL.md` |
| Work in the Scheduler area (88 symbols) | `.claude/skills/generated/scheduler/SKILL.md` |
| Work in the Input_map area (79 symbols) | `.claude/skills/generated/input-map/SKILL.md` |
| Work in the Workspace area (64 symbols) | `.claude/skills/generated/workspace/SKILL.md` |
| Work in the Theme_config area (56 symbols) | `.claude/skills/generated/theme-config/SKILL.md` |
| Work in the Renderer area (55 symbols) | `.claude/skills/generated/renderer/SKILL.md` |
| Work in the Config area (44 symbols) | `.claude/skills/generated/config/SKILL.md` |
| Work in the Text area (42 symbols) | `.claude/skills/generated/text/SKILL.md` |
| Work in the Ui area (30 symbols) | `.claude/skills/generated/ui/SKILL.md` |
| Work in the Benches area (27 symbols) | `.claude/skills/generated/benches/SKILL.md` |
| Work in the Render area (26 symbols) | `.claude/skills/generated/render/SKILL.md` |
| Work in the Editor area (23 symbols) | `.claude/skills/generated/editor/SKILL.md` |

<!-- gitnexus:end -->
