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
