; Netherize Editor — Python highlight queries

; --- Comments ---
(comment) @syntax.comment

; --- Strings ---
(string) @syntax.string
(string (string_start) @syntax.string)
(string (string_end) @syntax.string)
(string (interpolation "{" @syntax.punctuation "}" @syntax.punctuation))
(escape_sequence) @syntax.escape

; --- Numbers ---
(integer) @syntax.number
(float) @syntax.number

; --- Booleans & None ---
(true) @syntax.boolean
(false) @syntax.boolean
(none) @syntax.constant

; --- Keywords ---
[
  "and" "as" "assert" "async" "await" "break"
  "class" "continue" "def" "del" "elif" "else"
  "except" "exec" "finally" "for" "from" "global"
  "if" "import" "in" "is" "lambda" "nonlocal"
  "not" "or" "pass" "print" "raise" "return"
  "try" "while" "with" "yield"
] @syntax.keyword

; --- Decorators ---
(decorator) @syntax.attribute

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
  function: (identifier) @syntax.type
  (#match? @syntax.type "^(bool|int|float|str|bytes|list|dict|set|tuple|frozenset|type|object)$"))

; --- Attributes ---
(attribute
  attribute: (identifier) @syntax.property)

; --- Variables / Identifiers ---
(identifier) @syntax.identifier

; --- Builtin functions ---
(call
  function: (identifier) @syntax.function
  (#match? @syntax.function "^(print|len|range|enumerate|zip|map|filter|sorted|reversed|min|max|sum|any|all|isinstance|issubclass|hasattr|getattr|setattr|delattr|type|super|vars|dir|id|hash|repr|chr|ord|bin|oct|hex|input|open|abs|round|pow|divmod|callable|compile|eval|exec|format|globals|locals|iter|next|property|staticmethod|classmethod)$"))

; --- UPPER_CASE constants ---
((identifier) @syntax.constant
  (#match? @syntax.constant "^_*[A-Z][A-Z\\d_]+$"))

; --- self / cls as parameter ---
((identifier) @syntax.type
  (#match? @syntax.type "^(self|cls)$"))

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
