; Netherize Editor — JavaScript highlight queries

; --- Comments ---
[
  (comment)
  (html_comment)
] @syntax.comment

; --- Strings / templates / regex ---
[
  (string)
  (string_fragment)
  (regex)
] @syntax.string

(escape_sequence) @syntax.escape

; --- Numbers / booleans / null / undefined ---
(number) @syntax.number
[
  (true)
  (false)
] @syntax.boolean
(null) @syntax.constant
(undefined) @syntax.constant

; --- Keywords ---
[
  "as" "async" "await" "break" "case" "catch" "class" "const"
  "continue" "debugger" "default" "delete" "do" "else" "export"
  "extends" "finally" "for" "from" "function" "get" "if" "import"
  "in" "instanceof" "let" "new" "of" "return" "set" "static"
  "super" "switch" "target" "this" "throw" "try" "typeof" "var"
  "void" "while" "with" "yield"
] @syntax.keyword

; --- Types / constructors / namespaces ---
(class_declaration name: (identifier) @syntax.type)
(class name: (identifier) @syntax.type)
(new_expression constructor: (identifier) @syntax.constructor)
(new_expression constructor: (member_expression property: (property_identifier) @syntax.constructor))

((member_expression object: (identifier) @syntax.namespace)
 (#match? @syntax.namespace "^[A-Z]"))

; --- Functions ---
(function_declaration name: (identifier) @syntax.function)
(generator_function_declaration name: (identifier) @syntax.function)
(method_definition name: (property_identifier) @syntax.function)
(pair key: (property_identifier) @syntax.function value: [(function) (arrow_function) (generator_function)])
(call_expression function: (identifier) @syntax.function)
(call_expression function: (member_expression property: (property_identifier) @syntax.function))

; --- Parameters ---
(formal_parameters (identifier) @syntax.parameter)
(arrow_function parameter: (identifier) @syntax.parameter)

; --- Constants (must come before variables) ---
((identifier) @syntax.constant
 (#match? @syntax.constant "^_*[A-Z][A-Z\d_]+$"))

; --- Variables / declarations ---
(variable_declarator name: (identifier) @syntax.variable)
(lexical_declaration (variable_declarator name: (identifier) @syntax.variable))

; --- Properties / fields ---
(pair key: (property_identifier) @syntax.property)
(pair key: (string (string_fragment) @syntax.property))
(member_expression property: (property_identifier) @syntax.field)
(subscript_expression index: (string (string_fragment) @syntax.property))

; --- Operators ---
[
  "+" "-" "*" "/" "%" "**" "=" "==" "===" "!=" "!=="
  "<" ">" "<=" ">=" "&&" "||" "!" "&" "|" "^" "~"
  "<<" ">>" ">>>" "+=" "-=" "*=" "/=" "%=" "&&=" "||="
  "??" "??=" "&=" "|=" "^=" "<<=" ">>=" ">>>=" "++" "--"
  "=>" "." "?."
] @syntax.operator

; --- Punctuation ---
[
  ";" "," ":" "?" "(" ")" "[" "]" "{" "}"
] @syntax.punctuation

; --- Fallback (identifier references) ---
(identifier) @syntax.identifier
