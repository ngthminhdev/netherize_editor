; Netherize Editor — Bash highlight queries

; --- Comments ---
(comment) @syntax.comment

; --- Strings / heredocs ---
[
  (string)
  (raw_string)
  (heredoc_body)
] @syntax.string

(escape_sequence) @syntax.string.escape

; --- Numbers ---
(number) @syntax.number

; --- Keywords ---
[
  "if" "then" "else" "elif" "fi" "for" "do" "done" "while" "until"
  "case" "esac" "select"
] @syntax.keyword.control

[
  "function"
] @syntax.keyword.storage

[
  "in" "time" "coproc"
] @syntax.keyword

; --- Functions / commands ---
(function_definition name: (word) @syntax.function)
(command_name (word) @syntax.function)

; --- Variables / parameters / constants ---
(variable_name) @syntax.variable
(special_variable_name) @syntax.variable.builtin
(positional_variable_name) @syntax.variable.builtin
((variable_name) @syntax.constant
 (#match? @syntax.constant "^_*[A-Z][A-Z\d_]+$"))

; --- Flags / assignment names ---
((word) @syntax.property (#match? @syntax.property "^--?[A-Za-z0-9_-]+$"))
(variable_assignment name: (variable_name) @syntax.property)

; --- Operators ---
[
  "=" "==" "!=" "=~" "-eq" "-ne" "-lt" "-gt" "-le" "-ge"
  "&&" "||" "!" "|" "&" ">" ">>" "<" "<<" "<<<" "$" "$((" "$("
] @syntax.operator

; --- Punctuation ---
[
  ";" "," "(" ")" "[" "]" "{" "}"
] @syntax.punctuation

; --- Fallback ---
(word) @syntax.identifier
(variable_name) @syntax.identifier
