; Dart tree-sitter highlight captures mapped directly to ThemeConfig syntax tokens.

; --- Comments ---
(comment) @syntax.comment

; --- Strings ---
(string_literal) @syntax.string
(escape_sequence) @syntax.string.escape

; --- Numbers ---
[
  (decimal_integer_literal)
  (hex_integer_literal)
  (decimal_floating_point_literal)
] @syntax.number

; --- Booleans & Null ---
(true) @syntax.boolean
(false) @syntax.boolean
(null_literal) @syntax.constant

; --- Keywords ---
; Control Flow
[
  "as"
  "assert"
  "async"
  "await"
  "break"
  "case"
  "catch"
  "continue"
  "default"
  "do"
  "else"
  "finally"
  "for"
  "hide"
  "if"
  "in"
  "on"
  "part"
  "rethrow"
  "return"
  "show"
  "switch"
  "throw"
  "try"
  "while"
  "yield"
] @syntax.keyword.control

; Storage / Declarations
[
  "abstract"
  "class"
  "covariant"
  "enum"
  "export"
  "extension"
  "external"
  "factory"
  "final"
  "implements"
  "import"
  "interface"
  "late"
  "library"
  "mixin"
  "operator"
  "required"
  "static"
  "typedef"
  "var"
  "with"
] @syntax.keyword.storage

; General Keywords
[
  "const"
  "new"
  "super"
  "this"
] @syntax.keyword

; --- Types ---
(type_identifier) @syntax.type
(void_type) @syntax.type.builtin

; Built-in Types
((type_identifier) @syntax.type.builtin
  (#match? @syntax.type.builtin "^(int|double|num|String|bool|List|Set|Map|Runes|Symbol|Future|Stream|Iterable|Never|dynamic|Object)$"))

; Capitalized identifiers (Classes)
((identifier) @syntax.type
 (#match? @syntax.type "^[A-Z]"))

; Named Declarations
(class_declaration name: (identifier) @syntax.type)
(mixin_declaration name: (identifier) @syntax.type)
(extension_declaration name: (identifier) @syntax.type)
(enum_declaration name: (identifier) @syntax.type)

; --- Functions & Methods ---
(function_signature name: (identifier) @syntax.function)
(constructor_signature name: (identifier) @syntax.constructor)
(constant_constructor_signature name: (identifier) @syntax.constructor)
(getter_signature name: (identifier) @syntax.function)
(setter_signature name: (identifier) @syntax.function)

; Calls
(call_expression
  function: (identifier) @syntax.function)

(call_expression
  function: (member_expression
    property: (identifier) @syntax.function))

; --- Fields & Properties ---
(member_expression
  property: (identifier) @syntax.field)

; --- Parameters ---
(formal_parameter
  name: (identifier) @syntax.parameter)
(super_formal_parameter
  (identifier) @syntax.parameter)

; --- Annotations ---
(annotation
  name: (identifier) @syntax.attribute)
