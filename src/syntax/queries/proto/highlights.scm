; Netherize Editor — Protobuf highlight queries

; --- Comments ---
(comment) @syntax.comment

; --- Strings / escapes ---
(string) @syntax.string
(escape_sequence) @syntax.escape

; --- Numbers ---
(int_lit) @syntax.number
(float_lit) @syntax.number

; --- Booleans ---
(true) @syntax.boolean
(false) @syntax.boolean

; --- Keywords ---
; IMPORTANT: do not capture statement/container nodes like `(message)` or `(service)`
; as keywords, because those nodes span the whole declaration body and would paint
; almost the entire block with one color. Capture only the keyword tokens.
[
  "syntax"
  "edition"
  "package"
  "import"
  "option"
  "reserved"
  "enum"
  "extend"
  "extensions"
  "message"
  "oneof"
  "service"
  "rpc"
  "returns"
  "optional"
  "repeated"
  "required"
  "stream"
  "to"
  "max"
  "public"
  "weak"
] @syntax.keyword

; --- Package namespace ---
(package
  (full_ident
    (identifier) @syntax.namespace))

; --- Built-in types ---
(key_type) @syntax.type
(type) @syntax.type

; --- Named types ---
(message_name) @syntax.type
(enum_name) @syntax.type
(service_name) @syntax.type
(message_or_enum_type) @syntax.type

(extend
  (full_ident
    (identifier) @syntax.type))

(oneof
  (identifier) @syntax.type)

; --- RPC method names ---
(rpc_name) @syntax.function

; --- Constants ---
(constant
  (full_ident
    (identifier) @syntax.constant))

(enum_field
  (identifier) @syntax.constant)

; --- Field / property names ---
(field
  (identifier) @syntax.property)

(map_field
  (identifier) @syntax.property)

(oneof_field
  (identifier) @syntax.property)

(field_option
  (identifier) @syntax.property)

(enum_value_option
  (identifier) @syntax.property)

(block_lit
  (identifier) @syntax.property)

; --- Option names ---
(option
  (full_ident
    (identifier) @syntax.variable))

(option
  (full_ident
    (identifier)
    (identifier) @syntax.variable))

; --- Punctuation ---
[
  "("
  ")"
  "["
  "]"
  "{"
  "}"
  "<"
  ">"
  ";"
  ","
  "."
  ":"
] @syntax.punctuation

; --- Operators ---
[
  "="
  "-"
  "+"
] @syntax.operator
