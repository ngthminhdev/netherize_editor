; Rust tree-sitter highlight captures mapped directly to ThemeConfig syntax tokens.

; Comments

(line_comment) @syntax.comment
(block_comment) @syntax.comment
(line_comment (doc_comment)) @syntax.comment
(block_comment (doc_comment)) @syntax.comment

; Strings and numbers

(char_literal) @syntax.string
(string_literal) @syntax.string
(raw_string_literal) @syntax.string
(escape_sequence) @syntax.escape

(integer_literal) @syntax.number
(float_literal) @syntax.number
(boolean_literal) @syntax.boolean

; Type names

(type_identifier) @syntax.type
(primitive_type) @syntax.type
((identifier) @syntax.type
 (#match? @syntax.type "^[A-Z]"))
((scoped_identifier
  path: (identifier) @syntax.type)
 (#match? @syntax.type "^[A-Z]"))
((scoped_identifier
  path: (scoped_identifier
    name: (identifier) @syntax.type))
 (#match? @syntax.type "^[A-Z]"))
((scoped_type_identifier
  path: (identifier) @syntax.type)
 (#match? @syntax.type "^[A-Z]"))
((scoped_type_identifier
  path: (scoped_identifier
    name: (identifier) @syntax.namespace))
 (#match? @syntax.namespace "^[A-Z]"))

(scoped_identifier path: (identifier) @syntax.namespace)
(scoped_type_identifier path: (identifier) @syntax.namespace)

; Constructors, variants, and attributes

(struct_item name: (type_identifier) @syntax.constructor)
(enum_item name: (type_identifier) @syntax.constructor)
(union_item name: (type_identifier) @syntax.constructor)
(trait_item name: (type_identifier) @syntax.constructor)
(impl_item type: (type_identifier) @syntax.constructor)
(tuple_struct_pattern type: (identifier) @syntax.constructor)
(struct_pattern type: (type_identifier) @syntax.constructor)
(scoped_identifier name: (identifier) @syntax.constructor)

(attribute_item) @syntax.attribute

; Functions and macros

(function_item name: (identifier) @syntax.function)
(function_signature_item name: (identifier) @syntax.function)

(call_expression
  function: (identifier) @syntax.function)
(call_expression
  function: (field_expression
    field: (field_identifier) @syntax.function))
(call_expression
  function: (scoped_identifier
    name: (identifier) @syntax.function))

(generic_function
  function: (identifier) @syntax.function)
(generic_function
  function: (scoped_identifier
    name: (identifier) @syntax.function))
(generic_function
  function: (field_expression
    field: (field_identifier) @syntax.function))

(macro_invocation
  macro: (identifier) @syntax.macro)
(macro_invocation
  macro: (scoped_identifier
    name: (identifier) @syntax.macro))
(macro_definition name: (identifier) @syntax.macro)

; Parameters

(parameter pattern: (identifier) @syntax.parameter)
(self_parameter (self) @syntax.parameter)
(closure_parameters (identifier) @syntax.parameter)

; Variables

(let_declaration pattern: (identifier) @syntax.variable)
(for_expression pattern: (identifier) @syntax.variable)

; Member access vs struct literal properties

(field_declaration name: (field_identifier) @syntax.field)
(field_expression field: (field_identifier) @syntax.field)

(field_initializer field: (field_identifier) @syntax.property)
(shorthand_field_initializer (identifier) @syntax.property)
(field_pattern name: (field_identifier) @syntax.property)
(field_pattern name: (shorthand_field_identifier) @syntax.property)

; Constants and lifetimes

(const_item name: (identifier) @syntax.constant)
(static_item name: (identifier) @syntax.constant)
(const_parameter name: (identifier) @syntax.constant)
((identifier) @syntax.constant
 (#match? @syntax.constant "^[A-Z][A-Z\\d_]+$"))

(lifetime) @syntax.lifetime

; Keywords

"as" @syntax.keyword
"async" @syntax.keyword
"await" @syntax.keyword
"break" @syntax.keyword
"const" @syntax.keyword
"continue" @syntax.keyword
"default" @syntax.keyword
"dyn" @syntax.keyword
"else" @syntax.keyword
"enum" @syntax.keyword
"extern" @syntax.keyword
"fn" @syntax.keyword
"for" @syntax.keyword
"if" @syntax.keyword
"impl" @syntax.keyword
"in" @syntax.keyword
"let" @syntax.keyword
"loop" @syntax.keyword
"macro_rules!" @syntax.keyword
"match" @syntax.keyword
"mod" @syntax.keyword
"move" @syntax.keyword
"pub" @syntax.keyword
"ref" @syntax.keyword
"return" @syntax.keyword
"static" @syntax.keyword
"struct" @syntax.keyword
"trait" @syntax.keyword
"type" @syntax.keyword
"union" @syntax.keyword
"unsafe" @syntax.keyword
"use" @syntax.keyword
"where" @syntax.keyword
"while" @syntax.keyword
(crate) @syntax.keyword
(mutable_specifier) @syntax.keyword
(self) @syntax.keyword
(super) @syntax.keyword

; Operators

[
  "*"
  "&"
  "="
  "=="
  "!="
  "+"
  "-"
  "/"
  "%"
  "!"
  "?"
  "|"
  "||"
  "&&"
  "^"
  "<<"
  ">>"
  "+="
  "-="
  "*="
  "/="
  "%="
  "&="
  "|="
  "^="
  "<<="
  ">>="
  "->"
  "=>"
  ".."
  "..="
] @syntax.operator

; Punctuation

"(" @syntax.punctuation
")" @syntax.punctuation
"[" @syntax.punctuation
"]" @syntax.punctuation
"{" @syntax.punctuation
"}" @syntax.punctuation

(type_arguments
  "<" @syntax.punctuation
  ">" @syntax.punctuation)
(type_parameters
  "<" @syntax.punctuation
  ">" @syntax.punctuation)

"::" @syntax.punctuation
":" @syntax.punctuation
"." @syntax.punctuation
"," @syntax.punctuation
";" @syntax.punctuation

; Generic identifier fallback comes last and is intentionally broad.

(identifier) @syntax.identifier
