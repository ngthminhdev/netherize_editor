; --- Headings ---
; # markers and heading text same keyword color (matches VS Code Dark+).
[
  (atx_h1_marker)
  (atx_h2_marker)
  (atx_h3_marker)
  (atx_h4_marker)
  (atx_h5_marker)
  (atx_h6_marker)
  (setext_h1_underline)
  (setext_h2_underline)
] @syntax.keyword

(atx_heading    (inline) @syntax.keyword)
(setext_heading (paragraph) @syntax.keyword)

; --- Code blocks ---
(fenced_code_block_delimiter) @syntax.punctuation
(info_string)        @syntax.type
(code_fence_content) @syntax.string
(indented_code_block) @syntax.string

; --- Links (block-level reference definitions) ---
(link_reference_definition) @syntax.attribute
(link_destination) @syntax.string
(link_title)       @syntax.string
(link_label)       @syntax.attribute

; --- Block quotes ---
(block_quote)        @syntax.comment
(block_quote_marker) @syntax.comment

; --- Lists ---
[
  (list_marker_plus)
  (list_marker_minus)
  (list_marker_star)
  (list_marker_dot)
  (list_marker_parenthesis)
] @syntax.operator

[
  (task_list_marker_checked)
  (task_list_marker_unchecked)
] @syntax.operator

; --- Pipe tables ---
(pipe_table_header (pipe_table_cell) @syntax.keyword)
(pipe_table_delimiter_cell) @syntax.punctuation
(pipe_table_cell)   @syntax.identifier

; --- HTML blocks ---
(html_block) @syntax.comment

; --- Special characters ---
(backslash_escape)          @syntax.escape
(entity_reference)          @syntax.escape
(numeric_character_reference) @syntax.escape

; --- Structure ---
(thematic_break)    @syntax.punctuation
(block_continuation) @syntax.punctuation
