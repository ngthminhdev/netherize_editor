# Markdown Preview Test

## Typography

### Heading Level 3

#### Heading Level 4

##### Heading Level 5

This is a normal paragraph with **bold text**, *italic text*, and `inline code`. You can also use ~~strikethrough~~ for deleted content.

## Links & Images

Here's an [external link](https://github.com) with underline styling.

Autolinks work too: <https://netherize.dev> and <mailto:hello@example.com>

Images render as placeholders: ![Netherize Logo](https://netherize.dev/logo.png)

![Empty alt image](https://example.com/missing.png)

## Code Blocks

```rust
fn main() {
    let greeting = "Hello, Netherize!";
    println!("{}", greeting);

    // Tree-sitter powered syntax highlighting
    let numbers: Vec<i32> = (0..100).collect();
    let sum: i32 = numbers.iter().sum();
}
```

---

```python
def fibonacci(n: int) -> list[int]:
    """Generate Fibonacci sequence."""
    a, b = 0, 1
    result = []
    for _ in range(n):
        result.append(a)
        a, b = b, a + b
    return result
```

---

```json
{
  "editor": "netherize",
  "theme": "dark",
  "features": ["vim-modes", "lsp", "tree-sitter"]
}
```

## Blockquotes

> This is a blockquote with **bold** and *italic* text inside.
> It can span multiple lines and still look great.

> Another blockquote paragraph.
> With a second line of quoted text.

## Lists

### Unordered List

- First item with **bold**
- Second item with `code`
- Third item with [a link](https://example.com)
- ~~Deleted item~~

### Ordered List

1. Clone the repository
2. Install dependencies
3. Run `cargo build --release`
4. Launch the editor

### Task List

- [x] Implement markdown preview
- [x] Add heading font sizes
- [x] Add strikethrough support
- [x] Add link underlines
- [ ] Add image rendering
- [ ] Add math/LaTeX support

### Nested List

- Top level item
  - Nested item one
  - Nested item two
    - Deep nested item
    - Another deep item
  - Back to level 2
- Back to level 1

## Tables

| Feature | Status | Priority |
|---------|--------|----------|
| Bold & Italic | Done | High |
| Code blocks | Done | High |
| Strikethrough | Done | Medium |
| Link underlines | Done | Medium |
| Image placeholders | Done | Medium |
| Horizontal scroll | Done | Low |

| Language | Typing | Speed | Notes |
|----------|--------|-------|-------|
| Rust | Static | Fast | Memory safe |
| Python | Dynamic | Slow | Easy to learn |
| TypeScript | Static | Medium | JS superset |

## Horizontal Rules

Content above the rule.

---

Content below the rule.

## Mixed Formatting

This paragraph has **bold with *nested italic* inside**, `inline code`, and ~~strikethrough text~~.

Check out [this link with **bold** inside](https://example.com) and also ~~[deleted link](https://gone.com)~~.

Here's a sentence with multiple `code` fragments and **bold** mixed with *italic* and ~~strikethrough~~ all in one line.

Nested: **bold *italic* and ~~strike~~ together**.

## Long Table (Horizontal Scroll Test)

| Column A | Column B | Column C | Column D | Column E | Column F | Column G | Column H |
|----------|----------|----------|----------|----------|----------|----------|----------|
| A1 | B1 | C1 | D1 | E1 | F1 | G1 | H1 |
| A2 | B2 | C2 | D2 | E2 | F2 | G2 | H2 |
| A3 | B3 | C3 | D3 | E3 | F3 | G3 | H3 |
