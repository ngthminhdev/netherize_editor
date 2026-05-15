; JSON tree-sitter highlights mapped to Netherize's @syntax.* theme tokens.
;
; Goal: make keys, values, and structural elements visually distinct.
;
; NOTE: Strings are captured in value/array contexts only, NOT globally,
; to avoid overriding @syntax.property on key nodes (String has higher
; priority than Property in the normalize_spans painter).

; ── Comments ──────────────────────────────────────────────────────────────────

(comment) @syntax.comment

; ── Keys ──────────────────────────────────────────────────────────────────────
; Keys use @syntax.property so they stand out from regular string values.

(pair
  key: (_) @syntax.property)

; ── String values ─────────────────────────────────────────────────────────────
; Captured in value/array contexts to avoid overriding key highlights.

(pair
  value: (string) @syntax.string)

(array (string) @syntax.string)

(document (string) @syntax.string)

; ── Escape sequences in strings ───────────────────────────────────────────────

(escape_sequence) @syntax.string.escape

; ── Numbers ───────────────────────────────────────────────────────────────────

(number) @syntax.number

; ── Booleans ──────────────────────────────────────────────────────────────────

[
  (true)
  (false)
] @syntax.boolean

; ── Null ──────────────────────────────────────────────────────────────────────

(null) @syntax.constant

; ── Punctuation ───────────────────────────────────────────────────────────────

":" @syntax.punctuation
"," @syntax.punctuation

[
  "["
  "]"
  "{"
  "}"
] @syntax.punctuation
