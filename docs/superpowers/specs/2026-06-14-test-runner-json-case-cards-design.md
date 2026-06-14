# Test Runner JSON Case Cards Design

## Goal

Make the Test Runner approachable for JavaScript LeetCode work while preserving
the editor's keyboard-first flow. Cases use JSON text for stdin and expected
stdout, and the JavaScript file decides how those JSON values map to function
parameter types.

## User Flow

1. `New LeetCode File` creates a JavaScript scaffold whose `solve(data)` parses
   stdin with `JSON.parse(data)` and returns a JSON-serializable result.
2. The Test Runner header shows the active filename, runtime, JSON protocol,
   current `NAV`/`EDIT` mode, and a Run action.
3. Each case is a card with multiline `INPUT JSON` and `EXPECTED JSON` previews,
   status, duration, and actual/error output when relevant.
4. Selecting a field by mouse or keyboard opens a focused mini-editor inside the
   right dock. The mini-editor owns Test Runner state; it is not an editor file
   buffer and does not run LSP or tree-sitter.
5. Run validates both JSON fields before launching. Valid input is sent unchanged
   to stdin; expected and actual outputs are parsed as JSON and compared by JSON
   value so insignificant whitespace and object key ordering do not fail a case.

## Interaction

- Keyboard nav remains: `j/k` or arrows select cases, `h/l` or Tab select fields,
  `i`/Enter edit, `a` add, `x` delete, `F5` run, Esc leave edit/panel.
- Mini-editor supports multiline insertion, arrows, Home/End, Backspace/Delete,
  paste/IME, and local undo/redo.
- Mouse can select a card, open either JSON field, run cases, or add a case.
- The card list scrolls vertically and keeps the selected case visible.

## Architecture

- `runner/mod.rs` owns JSON validation/comparison, mini-editor cursor/undo state,
  card scroll offset, and pure layout-independent transitions.
- `render/renderer/ui/test_runner.rs` computes card/editor geometry, renders the
  header/cards/editor/footer, and exposes pure hit-test results.
- `application.rs` converts mouse coordinates and wheel input into Commands only.
- `commands_terminal.rs` is the mutation boundary for mouse-derived Test Runner
  commands and run validation.
- The async worker remains raw stdin/stdout and does not learn language types.

## Error Handling

- Invalid input or expected JSON prevents launch and selects the offending case
  and field with a concise inline error.
- Runtime failures still surface stderr and exit status on the case card.
- Non-JSON program output is a failed case with an explicit `actual is not JSON`
  message rather than a whitespace string mismatch.

## Non-Goals

- No LSP, syntax tree, completion, or file-buffer persistence in the mini-editor.
- No parameter-schema builder or cross-language type inference.
- No automatic conversion from arbitrary LeetCode website signatures.

## Verification

- Unit tests for JSON validation/comparison, cursor movement, undo/redo, scrolling,
  hit-testing, JavaScript scaffold behavior, and input routing.
- Event-loop tests confirm mouse actions dispatch through Commands.
- Full library tests, cargo check, rustfmt, and diff checks.
