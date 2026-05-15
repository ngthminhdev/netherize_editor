; Netherize Editor — Python highlight queries

; --- Comments ---
(comment) @syntax.comment

; --- Strings ---
(string) @syntax.string
(string (string_start) @syntax.string)
(string (string_end) @syntax.string)
(string (interpolation "{" @syntax.punctuation "}" @syntax.punctuation))
(escape_sequence) @syntax.string.escape

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

; --- Constructors / constants by naming convention ---
((identifier) @syntax.constructor
  (#match? @syntax.constructor "^[A-Z]"))

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
  "and" "or" "not" "is" "in"
  "&" "|" "^" "~" "<<" ">>"
] @syntax.operator

; --- Punctuation ---
[
  "(" ")" "[" "]" "{" "}"
  "." "," ":" ";"
  "->"
] @syntax.punctuation
