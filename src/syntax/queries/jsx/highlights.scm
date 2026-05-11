; Netherize Editor — JSX highlight queries

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

; --- Numbers / booleans / null ---
(number) @syntax.number
[(true) (false)] @syntax.boolean
(null) @syntax.constant
(undefined) @syntax.constant

; --- Keywords ---
[
  "as" "async" "await" "break" "case" "catch" "class" "const"
  "continue" "debugger" "default" "delete" "do" "else" "export"
  "extends" "finally" "for" "from" "function" "if" "import"
  "in" "instanceof" "let" "new" "of" "return" "static" "super"
  "switch" "this" "throw" "try" "typeof" "var" "void" "while"
  "with" "yield"
] @syntax.keyword

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
(jsx_expression (identifier) @syntax.variable)
(spread_element (identifier) @syntax.variable)

; --- Types / constructors ---
(class_declaration name: (identifier) @syntax.type)
(class name: (identifier) @syntax.type)
(new_expression constructor: (identifier) @syntax.constructor)
(new_expression constructor: (member_expression property: (property_identifier) @syntax.constructor))

; --- Functions ---
(function_declaration name: (identifier) @syntax.function)
(generator_function_declaration name: (identifier) @syntax.function)
(method_definition name: (property_identifier) @syntax.function)
(pair key: (property_identifier) @syntax.function value: [(function) (arrow_function) (generator_function)])
(call_expression function: (identifier) @syntax.function)
(call_expression function: (member_expression property: (property_identifier) @syntax.function))

; --- Parameters / variables ---
(formal_parameters (identifier) @syntax.parameter)
(arrow_function parameter: (identifier) @syntax.parameter)
(variable_declarator name: (identifier) @syntax.variable)

; --- Constants ---
((identifier) @syntax.constant
 (#match? @syntax.constant "^_*[A-Z][A-Z\d_]+$"))

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
  "??" "??=" "&=" "|=" "^=" "<<=" ">>=" "\u003e\u003e\u003e=" "++" "--"
  "=>" "." "?." "/>" "</"
] @syntax.operator

; --- Punctuation ---
[
  ";" "," ":" "?" "(" ")" "[" "]" "{" "}" "<" ">"
] @syntax.punctuation

; --- Fallback ---
(identifier) @syntax.identifier
(property_identifier) @syntax.identifier
