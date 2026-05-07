; Netherize Editor — TypeScript highlight queries

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
  "abstract" "as" "async" "await" "break" "case" "catch" "class"
  "const" "continue" "declare" "debugger" "default" "delete" "do"
  "else" "enum" "export" "extends" "finally" "for" "from" "function"
  "get" "if" "implements" "import" "in" "infer" "instanceof"
  "interface" "keyof" "let" "module" "namespace" "new" "of" "override"
  "private" "protected" "public" "readonly" "require" "return" "satisfies"
  "set" "static" "super" "switch" "target" "this" "throw" "try"
  "type" "typeof" "unique" "using" "var" "void" "while" "with" "yield"
] @syntax.keyword

; --- Types / namespaces / constructors ---
(type_identifier) @syntax.type
(predefined_type) @syntax.type
(type_alias_declaration name: (type_identifier) @syntax.type)
(interface_declaration name: (type_identifier) @syntax.type)
(enum_declaration name: (identifier) @syntax.type)
(class_declaration name: (type_identifier) @syntax.type)
(abstract_class_declaration name: (type_identifier) @syntax.type)
(module name: (identifier) @syntax.namespace)
(internal_module name: (identifier) @syntax.namespace)
(new_expression constructor: (identifier) @syntax.constructor)
(new_expression constructor: (member_expression property: (property_identifier) @syntax.constructor))

; --- Functions ---
(function_declaration name: (identifier) @syntax.function)
(method_definition name: (property_identifier) @syntax.function)
(method_signature name: (property_identifier) @syntax.function)
(abstract_method_signature name: (property_identifier) @syntax.function)
(pair key: (property_identifier) @syntax.function value: [(function) (arrow_function)])
(call_expression function: (identifier) @syntax.function)
(call_expression function: (member_expression property: (property_identifier) @syntax.function))

; --- Parameters / variables ---
(required_parameter pattern: (identifier) @syntax.parameter)
(optional_parameter pattern: (identifier) @syntax.parameter)
(formal_parameters (identifier) @syntax.parameter)
(arrow_function parameter: (identifier) @syntax.parameter)
(variable_declarator name: (identifier) @syntax.variable)

; --- Properties / fields ---
(public_field_definition name: (property_identifier) @syntax.field)
(property_signature name: (property_identifier) @syntax.property)
(pair key: (property_identifier) @syntax.property)
(pair key: (string (string_fragment) @syntax.property))
(member_expression property: (property_identifier) @syntax.field)
(subscript_expression index: (string (string_fragment) @syntax.property))

; --- Attributes / decorators ---
(decorator) @syntax.attribute

; --- Constants ---
((identifier) @syntax.constant
 (#match? @syntax.constant "^_*[A-Z][A-Z\d_]+$"))

; --- Operators ---
[
  "+" "-" "*" "/" "%" "**" "=" "==" "===" "!=" "!=="
  "<" ">" "<=" ">=" "&&" "||" "!" "&" "|" "^" "~"
  "<<" ">>" ">>>" "+=" "-=" "*=" "/=" "%=" "&&=" "||="
  "??" "??=" "&=" "|=" "^=" "<<=" ">>=" ">>>=" "++" "--"
  "=>" "." "?." ":" "|" "&"
] @syntax.operator

; --- Punctuation ---
[
  ";" "," "?" "(" ")" "[" "]" "{" "}" "<" ">"
] @syntax.punctuation

; --- Fallback ---
(identifier) @syntax.identifier
(property_identifier) @syntax.identifier
