;; ── XML highlights ──────────────────────────────────────────────────────
;; tree-sitter-xml v0.7.0 grammar captures

;; ── Comments ──────────────────────────────────────────────────────────────
(Comment) @syntax.comment

;; ── Tags ─────────────────────────────────────────────────────────────────
(STag (Name) @syntax.tag)
(ETag (Name) @syntax.tag)
(EmptyElemTag (Name) @syntax.tag)

;; ── Delimiters ───────────────────────────────────────────────────────────
[
  "<?"  "?>"
  "<!"  "]]>"
  "<"   ">"
  "</"  "/>"
] @syntax.punctuation

[
  "("  ")"  "["  "]"
] @syntax.punctuation

[
  "\""  "'"
] @syntax.punctuation

[
  ","  "|"  "="
] @syntax.operator

;; ── Attributes ───────────────────────────────────────────────────────────
(Attribute (Name) @syntax.attribute)
(Attribute (AttValue) @syntax.string)

;; ── Doctype ───────────────────────────────────────────────────────────────
(doctypedecl "DOCTYPE" @syntax.keyword)
(doctypedecl (Name) @syntax.type)

;; ── Entities ──────────────────────────────────────────────────────────────
(EntityRef) @syntax.escape
(CharRef) @syntax.escape

;; ── CDATA ────────────────────────────────────────────────────────────────
(CDSect
  (CDStart) @syntax.comment
  (CData) @syntax.string
  "]]>" @syntax.comment)

;; ── Text content — intentionally no highlight, default foreground ─────────
