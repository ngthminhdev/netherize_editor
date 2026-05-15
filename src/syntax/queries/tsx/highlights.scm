; Netherize Editor — TSX highlight queries

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

; --- Numbers / booleans / null ---
(number) @syntax.number
[(true) (false)] @syntax.boolean
(null) @syntax.constant
(undefined) @syntax.constant

; --- Keywords ---
[
  "await" "break" "case" "catch" "continue" "default" "do" "else"
  "finally" "for" "if" "return" "switch" "throw" "try" "while" "with" "yield"
] @syntax.keyword.control

[
  "abstract" "class" "const" "declare" "enum" "function" "interface" "let"
  "namespace" "override" "private" "protected" "public" "readonly" "static"
  "type" "using" "var"
] @syntax.keyword.storage

[
  "as" "async" "debugger" "delete" "export" "extends" "from" "implements"
  "import" "in" "infer" "instanceof" "keyof" "new" "of" "satisfies"
  "typeof" "void"
] @syntax.keyword

[
  "this" "super"
] @syntax.variable.builtin

; --- JSX tags / attributes ---
((jsx_opening_element
   name: (identifier) @syntax.constructor)
 (#match? @syntax.constructor "^[A-Z]"))
((jsx_closing_element
   name: (identifier) @syntax.constructor)
 (#match? @syntax.constructor "^[A-Z]"))
((jsx_self_closing_element
   name: (identifier) @syntax.constructor)
 (#match? @syntax.constructor "^[A-Z]"))

(jsx_opening_element name: (identifier) @syntax.tag)
(jsx_closing_element name: (identifier) @syntax.tag)
(jsx_self_closing_element name: (identifier) @syntax.tag)
(jsx_attribute (property_identifier) @syntax.attribute)

; --- Types / namespaces / constructors ---
(type_identifier) @syntax.type
(predefined_type) @syntax.type.builtin
(type_alias_declaration name: (type_identifier) @syntax.type)
(interface_declaration name: (type_identifier) @syntax.type)
(class_declaration name: (type_identifier) @syntax.type)
(abstract_class_declaration name: (type_identifier) @syntax.type)
(enum_declaration name: (identifier) @syntax.type)
(new_expression constructor: (identifier) @syntax.constructor)
(new_expression constructor: (member_expression property: (property_identifier) @syntax.constructor))

; --- Functions ---
(function_declaration name: (identifier) @syntax.function)
(method_definition name: (property_identifier) @syntax.function)
(method_signature name: (property_identifier) @syntax.function)
(pair key: (property_identifier) @syntax.function value: [(function) (arrow_function)])
(call_expression function: (identifier) @syntax.function)
(call_expression function: (member_expression property: (property_identifier) @syntax.function))

; --- Parameters / variables ---
(required_parameter pattern: (identifier) @syntax.parameter)
(optional_parameter pattern: (identifier) @syntax.parameter)
(formal_parameters (identifier) @syntax.parameter)
(arrow_function parameter: (identifier) @syntax.parameter)
(variable_declarator name: (identifier) @syntax.variable)
(jsx_expression (identifier) @syntax.variable)
(spread_element (identifier) @syntax.variable)

; --- Properties / fields ---
(public_field_definition name: (property_identifier) @syntax.field)
(property_signature name: (property_identifier) @syntax.property)
(pair key: (property_identifier) @syntax.property)
(pair key: (string (string_fragment) @syntax.property))
(member_expression property: (property_identifier) @syntax.field)
(subscript_expression index: (string (string_fragment) @syntax.property))

; --- Decorators / constants ---
(decorator) @syntax.attribute
((identifier) @syntax.constant
 (#match? @syntax.constant "^_*[A-Z][A-Z\d_]+$"))

; --- Operators ---
[
  "+" "-" "*" "/" "%" "**" "=" "==" "===" "!=" "!=="
  "<" ">" "<=" ">=" "&&" "||" "!" "&" "|" "^" "~"
  "<<" ">>" ">>>" "+=" "-=" "*=" "/=" "%=" "&&=" "||="
  "??" "??=" "&=" "|=" "^=" "<<=" ">>=" ">>>=" "++" "--"
  "=>" "." "?." ":" "|" "&" "/>" "</"
] @syntax.operator

; --- Punctuation ---
[
  ";" "," "?" "(" ")" "[" "]" "{" "}" "<" ">"
] @syntax.punctuation

; --- Fallback ---
(identifier) @syntax.identifier
(property_identifier) @syntax.identifier
