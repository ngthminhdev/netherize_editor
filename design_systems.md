# Design System (Dark Editor Style)

## 1. Colors

### Background
- bg: #07080d
- editor: #0d1017
- panel: #121622
- sidebar: #0f1320
- terminal: #0b0f18
- overlay: rgba(5,7,12,0.72)

### Text
- fg: #f2f4f8
- fg-dim: #b7bfcc
- fg-faint: #8f98aa
- fg-ghost: #6d7483
- gutter: #5e6575
- gutter-active: #d8deea

### Accent / Semantic
- accent: #9BE564        # primary / active
- cyan: #2FD3F6          # function / link
- magenta: #E77AE9       # keyword
- amber: #F5B63A         # type / highlight
- success: #67D67C       # success
- warning: #F2B84B       # warning
- info: #49C6F8          # info
- error: #FF7B72         # error

---

## 2. Usage Rules

### Text
- primary text → fg
- secondary → fg-dim
- placeholder → fg-faint
- disabled / comment → fg-ghost

### Background
- app → bg
- main content/editor → editor
- card/panel → panel
- sidebar → sidebar
- logs → terminal

### Interaction
- active / selected → accent
- hover → lighter panel
- focus → accent or cyan

### State
- success → success
- warning → warning
- error → error
- info → info

---

## 3. Syntax Highlight

- keyword → magenta
- function → cyan
- type → amber
- string → success
- number → error
- comment → fg-ghost
- variable → fg

---

## 4. Components

### Card
- bg: panel
- title: fg
- desc: fg-dim
- active badge: accent

### Button
- primary → accent
- secondary → panel + fg
- danger → error
- success → success

### Editor
- bg: editor
- line number: gutter
- active line: gutter-active
- cursor: accent

---

## 5. Principles

- dark UI + low contrast background
- bright accents for meaning only
- 1 block = 1 main color
- avoid overusing colors
- hierarchy = brightness, not color spam