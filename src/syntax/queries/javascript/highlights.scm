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

(escape_sequence) @syntax.string.escape

(template_substitution
  "$" @syntax.punctuation
  "{" @syntax.punctuation
  "}" @syntax.punctuation)

(template_substitution (identifier) @syntax.identifier)
(template_substitution (member_expression property: (property_identifier) @syntax.field))
(template_substitution (call_expression function: (identifier) @syntax.function))
(template_substitution (call_expression function: (member_expression property: (property_identifier) @syntax.function)))
(template_substitution (number) @syntax.number)
[(template_substitution (true)) (template_substitution (false))] @syntax.boolean
(template_substitution (null) @syntax.constant)
(template_substitution (undefined) @syntax.constant)

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
  "await" "break" "case" "catch" "continue" "default" "do" "else"
  "finally" "for" "if" "return" "switch" "throw" "try" "while" "with" "yield"
] @syntax.keyword.control

[
  "class" "const" "function" "get" "let" "set" "static" "var"
] @syntax.keyword.storage

[
  "as" "async" "debugger" "delete" "export" "extends" "from" "import"
  "in" "instanceof" "new" "of" "target" "typeof" "void"
] @syntax.keyword

[
  "this" "super"
] @syntax.variable.builtin

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
