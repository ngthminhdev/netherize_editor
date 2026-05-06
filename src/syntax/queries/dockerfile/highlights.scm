; Dockerfile keywords
[
  "FROM"
  "AS"
  "RUN"
  "CMD"
  "LABEL"
  "EXPOSE"
  "ENV"
  "ADD"
  "COPY"
  "ENTRYPOINT"
  "VOLUME"
  "USER"
  "WORKDIR"
  "ARG"
  "ONBUILD"
  "STOPSIGNAL"
  "HEALTHCHECK"
  "SHELL"
  "MAINTAINER"
  "CROSS_BUILD"
  (heredoc_marker)
  (heredoc_end)
] @syntax.keyword

; Operators
[
  ":"
  "@"
] @syntax.operator

; Comments
(comment) @syntax.comment

; Image tag/digest punctuation
(image_spec
  (image_tag
    ":" @syntax.punctuation)
  (image_digest
    "@" @syntax.punctuation))

; Strings
[
  (double_quoted_string)
  (single_quoted_string)
  (json_string)
  (heredoc_block)
] @syntax.string

; Escape sequences
(escape_sequence) @syntax.escape

; Variable expansion
(expansion
  [
    "$"
    "{"
    "}"
  ] @syntax.punctuation
) @syntax.constant

; Uppercase variables as constants
((variable) @syntax.constant
 (#match? @syntax.constant "^[A-Z][A-Z_0-9]*$"))

; Property names (arg names, env names, label keys, param names)
(arg_pair name: (unquoted_string) @syntax.property)
(env_pair name: (unquoted_string) @syntax.property)
(label_pair key: (_) @syntax.property)
(param name: (_) @syntax.property)
(mount_param name: (_) @syntax.property)
(mount_param_param) @syntax.property

; Expose ports as numbers
(expose_port) @syntax.number

; Paths
(path) @syntax.string

; JSON array brackets
["[" "]"] @syntax.punctuation

; Flag dashes
"--" @syntax.operator
