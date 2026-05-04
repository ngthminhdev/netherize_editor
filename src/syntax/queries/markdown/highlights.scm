; Headings
(atx_heading
  (inline) @syntax.keyword)

(setext_heading
  (paragraph) @syntax.keyword)

[
  (atx_h1_marker)
  (atx_h2_marker)
  (atx_h3_marker)
  (atx_h4_marker)
  (atx_h5_marker)
  (atx_h6_marker)
  (setext_h1_underline)
  (setext_h2_underline)
] @syntax.punctuation

; Code blocks
[
  (fenced_code_block)
  (indented_code_block)
] @syntax.string

(code_fence_content) @syntax.string

(fenced_code_block_delimiter) @syntax.punctuation

; Links
(link_destination) @syntax.string
(link_label) @syntax.attribute
(link_title) @syntax.string

; Block quotes
(block_quote_marker) @syntax.operator

; Lists
[
  (list_marker_plus)
  (list_marker_minus)
  (list_marker_star)
  (list_marker_dot)
  (list_marker_parenthesis)
] @syntax.operator

; Thematic break
(thematic_break) @syntax.punctuation

; Block continuation
(block_continuation) @syntax.punctuation

; Escapes
(backslash_escape) @syntax.escape
