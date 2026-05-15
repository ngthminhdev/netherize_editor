; YAML tree-sitter highlights mapped to Netherize's @syntax.* theme tokens.
;
; Goal: make keys, values, and structural elements visually distinct.
;
; NOTE: String scalars are captured in value/sequence contexts only, NOT
; globally, to avoid overriding @syntax.property on key nodes (String has
; higher priority than Property in the normalize_spans painter).

; ── Comments ──────────────────────────────────────────────────────────────────

(comment) @syntax.comment

; ── Document markers ──────────────────────────────────────────────────────────

[
  "---"
  "..."
] @syntax.keyword.storage

; ── Keys ──────────────────────────────────────────────────────────────────────
; Keys are highlighted as @syntax.property so they stand out from string values.

(block_mapping_pair
  key: (flow_node
    [
      (double_quote_scalar)
      (single_quote_scalar)
    ] @syntax.property))

(block_mapping_pair
  key: (flow_node
    (plain_scalar
      (string_scalar) @syntax.property)))

(flow_mapping
  (_
    key: (flow_node
      [
        (double_quote_scalar)
        (single_quote_scalar)
      ] @syntax.property)))

(flow_mapping
  (_
    key: (flow_node
      (plain_scalar
        (string_scalar) @syntax.property))))

; ── String values ─────────────────────────────────────────────────────────────
; Captured in value/sequence contexts to avoid overriding key highlights.

; Block mapping values
(block_mapping_pair
  value: (flow_node
    [
      (double_quote_scalar)
      (single_quote_scalar)
    ] @syntax.string))

(block_mapping_pair
  value: (flow_node
    (plain_scalar
      (string_scalar) @syntax.string)))

(block_mapping_pair
  value: (block_node
    (block_scalar) @syntax.string))

; Flow mapping values
(flow_mapping
  (_
    value: (flow_node
      [
        (double_quote_scalar)
        (single_quote_scalar)
      ] @syntax.string)))

(flow_mapping
  (_
    value: (flow_node
      (plain_scalar
        (string_scalar) @syntax.string))))

; Block sequence elements
(block_sequence_item
  (flow_node
    [
      (double_quote_scalar)
      (single_quote_scalar)
    ] @syntax.string))

(block_sequence_item
  (flow_node
    (plain_scalar
      (string_scalar) @syntax.string)))

(block_sequence_item
  (block_node
    (block_scalar) @syntax.string))

; Flow sequence elements
(flow_sequence
  (_
    [
      (double_quote_scalar)
      (single_quote_scalar)
    ] @syntax.string))

(flow_sequence
  (_
    (plain_scalar
      (string_scalar) @syntax.string)))

; ── Escape sequences ─────────────────────────────────────────────────────────

(escape_sequence) @syntax.string.escape

; ── Numbers ───────────────────────────────────────────────────────────────────

[
  (integer_scalar)
  (float_scalar)
] @syntax.number

; ── Booleans ──────────────────────────────────────────────────────────────────

(boolean_scalar) @syntax.boolean

; ── Null ──────────────────────────────────────────────────────────────────────

(null_scalar) @syntax.constant

; ── Timestamps (treated as a special string/constant) ─────────────────────────

(timestamp_scalar) @syntax.constant

; ── Anchors and aliases ───────────────────────────────────────────────────────

(anchor_name) @syntax.constant
(anchor "&" @syntax.punctuation)

(alias_name) @syntax.constant
(alias "*" @syntax.punctuation)

; ── Tags ──────────────────────────────────────────────────────────────────────

(tag) @syntax.type

; ── Directives ────────────────────────────────────────────────────────────────

[
  (yaml_directive)
  (tag_directive)
  (reserved_directive)
] @syntax.attribute

; ── Multi-line indicators ─────────────────────────────────────────────────────

[
  "|"
  ">"
  "?"
] @syntax.operator

; ── Punctuation ───────────────────────────────────────────────────────────────

[
  ":"
  "-"
  ","
] @syntax.punctuation

[
  "["
  "]"
  "{"
  "}"
] @syntax.punctuation
