; Netherize Editor — Python highlight queries

; --- Comments ---
(comment) @syntax.comment

; --- Strings ---
; Capture the delimiters and literal text as string, but deliberately do NOT
; capture the whole (string) node, so embedded expressions inside f-strings
; ({ ... } interpolations) keep their own syntax highlighting.
(string (string_start) @syntax.string)
(string (string_content) @syntax.string)
(string (string_end) @syntax.string)
(escape_sequence) @syntax.string.escape

; --- f-string interpolation ---
(interpolation "{" @syntax.punctuation)
(interpolation "}" @syntax.punctuation)

; --- Numbers ---
(integer) @syntax.number
(float) @syntax.number

; --- Booleans & None ---
(true) @syntax.boolean
(false) @syntax.boolean
(none) @syntax.constant

; --- Keywords ---
[
  "await" "break" "continue" "elif" "else" "except" "finally" "for"
  "if" "raise" "return" "try" "while" "with" "yield"
] @syntax.keyword.control

[
  "class" "def" "global" "lambda" "nonlocal"
] @syntax.keyword.storage

[
  "and" "as" "assert" "async" "del" "exec" "from" "import" "in"
  "is" "not" "or" "pass" "print"
] @syntax.keyword

; --- Pattern matching (soft keywords, Python 3.10+) ---
(match_statement "match" @syntax.keyword.control)
(case_clause "case" @syntax.keyword.control)

; --- Decorators ---
(decorator) @syntax.function
(decorator
  (identifier) @syntax.function)

; --- Function definitions ---
(function_definition
  name: (identifier) @syntax.function)

; --- Method definitions ---
(class_definition
  body: (block
    (function_definition
      name: (identifier) @syntax.function)))

; --- Class definitions ---
(class_definition
  name: (identifier) @syntax.constructor)

; --- Parameters ---
(parameters (identifier) @syntax.parameter)
(lambda_parameters (identifier) @syntax.parameter)
(default_parameter
  name: (identifier) @syntax.parameter)

; --- Types (type annotations) ---
(type (identifier) @syntax.type)
(call
  function: (identifier) @syntax.type.builtin
  (#match? @syntax.type.builtin "^(bool|int|float|str|bytes|list|dict|set|tuple|frozenset|type|object)$"))

; --- Namespaces / imports / member chains ---
(import_statement
  name: (dotted_name
    (identifier) @syntax.namespace))

(import_from_statement
  module_name: (dotted_name
    (identifier) @syntax.namespace))

(import_from_statement
  name: (dotted_name
    (identifier) @syntax.namespace))

(aliased_import
  name: (dotted_name
    (identifier) @syntax.namespace))

; --- Function calls ---
(call
  function: (attribute
    attribute: (identifier) @syntax.function))
(call
  function: (identifier) @syntax.function)

; --- Builtin functions ---
((call
  function: (identifier) @syntax.function.builtin)
 (#match? @syntax.function.builtin "^(print|len|range|enumerate|zip|map|filter|sorted|reversed|min|max|sum|any|all|isinstance|issubclass|hasattr|getattr|setattr|delattr|type|super|vars|dir|id|hash|repr|chr|ord|bin|oct|hex|input|open|abs|round|pow|divmod|callable|compile|eval|exec|format|globals|locals|iter|next|property|staticmethod|classmethod)$"))

; --- Attributes ---
(attribute
  attribute: (identifier) @syntax.property)

; --- Constructors / classes by naming convention (CamelCase only) ---
; Requires at least one lowercase letter so ALL_CAPS constants are NOT matched
; here and fall through to the @syntax.constant rule below.
((identifier) @syntax.constructor
  (#match? @syntax.constructor "^[A-Z][A-Za-z0-9_]*[a-z]"))

; --- Variables / Identifiers ---
(identifier) @syntax.identifier

; --- UPPER_CASE constants ---
((identifier) @syntax.constant
  (#match? @syntax.constant "^_*[A-Z][A-Z\\d_]+$"))

; --- self / cls as parameter ---
((identifier) @syntax.variable.builtin
  (#match? @syntax.variable.builtin "^(self|cls)$"))

; --- Operators ---
[
  "+" "-" "*" "/" "//" "%" "**"
  "=" "+=" "-=" "*=" "/=" "//=" "%=" "**="
  "&=" "|=" "^=" "<<=" ">>="
  "==" "!=" "<" ">" "<=" ">="
  ":="
  "and" "or" "not" "is" "in"
  "&" "|" "^" "~" "<<" ">>"
] @syntax.operator

; --- Punctuation ---
[
  "(" ")" "[" "]" "{" "}"
  "." "," ":" ";"
  "->"
] @syntax.punctuation
