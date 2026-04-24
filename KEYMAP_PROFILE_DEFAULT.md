# Netherize Editor — Unified Neovim Profile (Merged 3 files)
# ============================================================
# Sources:
# - nvim-full
# - nvim
# - default
#
# Goal:
# - keep rich Neovim-style workflow
# - keep stable app/editor commands
# - include safe default fallbacks for terminal/explorer/navigation
# ============================================================

[profile]
name = "nvim-ultimate"
description = "Merged Neovim-style profile with full features, stable defaults, and safe fallback bindings."

[meta]
leader = "space"

# ───────────────── GLOBAL ─────────────────
[[bindings]] key = "mod+s" command = "editor.save_file"
[[bindings]] key = "mod+o" command = "editor.open_file"
[[bindings]] key = "mod+p" command = "app.open_file_picker"
[[bindings]] key = "mod+shift+p" command = "app.open_command_palette"
[[bindings]] key = "mod+b" command = "app.toggle_explorer"
[[bindings]] key = "mod+f" command = "app.focus_explorer"
[[bindings]] key = "mod+backslash" command = "app.toggle_terminal"
[[bindings]] key = "mod+backtick" command = "app.toggle_terminal"
[[bindings]] key = "mod+w" command = "app.focus_back"
[[bindings]] key = "mod+l" command = "app.next_panel_tab"
[[bindings]] key = "mod+h" command = "app.prev_panel_tab"
[[bindings]] key = "Ctrl+=" command = "ui.scale_up"
[[bindings]] key = "Ctrl+-" command = "ui.scale_down"
[[bindings]] key = "Ctrl+0" command = "ui.scale_reset"
[[bindings]] key = "F10" command = "runner.smart_compile_and_run"
[[bindings]] key = "F12" command = "app.focus_terminal"

# ───────────────── INSERT MODE ─────────────────
[[bindings]] mode = "insert", key = "Escape", command = "mode.enter_normal"
[[bindings]] mode = "insert", key = "Backspace", command = "editor.backspace"
[[bindings]] mode = "insert", key = "Enter", command = "editor.newline"
[[bindings]] mode = "insert", key = "ArrowLeft", command = "editor.move_left"
[[bindings]] mode = "insert", key = "ArrowRight", command = "editor.move_right"
[[bindings]] mode = "insert", key = "ArrowUp", command = "editor.move_up"
[[bindings]] mode = "insert", key = "ArrowDown", command = "editor.move_down"
[[bindings]] mode = "insert", key = "Ctrl+j", command = "line.move_down"
[[bindings]] mode = "insert", key = "Ctrl+k", command = "line.move_up"

# ───────────────── NORMAL MODE ─────────────────
[[bindings]] mode = "normal", key = "Escape", command = "mode.enter_normal"
[[bindings]] mode = "normal", key = "i", command = "mode.enter_insert"
[[bindings]] mode = "normal", key = "v", command = "mode.enter_visual"

[[bindings]] mode = "normal", key = "h", command = "editor.move_left"
[[bindings]] mode = "normal", key = "j", command = "editor.move_down"
[[bindings]] mode = "normal", key = "k", command = "editor.move_up"
[[bindings]] mode = "normal", key = "l", command = "editor.move_right"

[[bindings]]
mode = "normal"
key = "w"
command = "editor.move_word_forward"

[[bindings]]
mode = "normal"
key = "b"
command = "editor.move_word_backward"

[[bindings]]
mode = "normal"
key = "e"
command = "editor.move_word_end"

[[bindings]]
mode = "normal"
key = "g e"
command = "editor.move_word_end_backward"

[[bindings]]
mode = "normal"
key = "0"
command = "editor.move_to_line_start"

[[bindings]]
mode = "normal"
key = "$"
command = "editor.move_to_line_end"

[[bindings]]
mode = "normal"
key = "^"
command = "editor.move_to_first_non_whitespace"

[[bindings]] mode = "normal", key = "ArrowLeft", command = "editor.move_left"
[[bindings]] mode = "normal", key = "ArrowRight", command = "editor.move_right"
[[bindings]] mode = "normal", key = "ArrowUp", command = "editor.move_up"
[[bindings]] mode = "normal", key = "ArrowDown", command = "editor.move_down"

# Edit actions
[[bindings]] mode = "normal", key = "I", command = "editor.insert_at_line_start"
[[bindings]] mode = "normal", key = "A", command = "editor.append_at_line_end"
[[bindings]] mode = "normal", key = "o", command = "editor.insert_line_below"
[[bindings]] mode = "normal", key = "O", command = "editor.insert_line_above"
[[bindings]] mode = "normal", key = "S", command = "editor.substitute_line"
[[bindings]] mode = "normal", key = "x", command = "editor.delete_char"

# Operators
[[bindings]] mode = "normal", key = "d d", command = "editor.delete_current_line"
[[bindings]] mode = "normal", key = "d w", command = "editor.delete_word_forward"
[[bindings]]
mode = "normal"
key = "d b"
command = "editor.delete_word_backward"

# Undo / redo
[[bindings]] mode = "normal", key = "u", command = "editor.undo"
[[bindings]] mode = "normal", key = "Ctrl+r", command = "editor.redo"

# Vim navigation / scrolling
[[bindings]] mode = "normal", key = "g g", command = "editor.move_to_first_line"
[[bindings]] mode = "normal", key = "G", command = "editor.move_to_last_line"
[[bindings]] mode = "normal", key = "ctrl+u", command = "editor.scroll_half_page_up"
[[bindings]] mode = "normal", key = "ctrl+d", command = "editor.scroll_half_page_down"
[[bindings]] mode = "normal", key = "z z", command = "editor.center_cursor_line"

[[bindings]] mode = "normal", key = "backtick", command = "app.toggle_terminal"
[[bindings]] mode = "normal", key = ":", command = "app.open_vim_command"

# ───────────────── VISUAL MODE ─────────────────
[[bindings]] mode = "visual", key = "Escape", command = "mode.enter_normal"
[[bindings]] mode = "visual", key = "h", command = "editor.move_left"
[[bindings]] mode = "visual", key = "j", command = "editor.move_down"
[[bindings]] mode = "visual", key = "k", command = "editor.move_up"
[[bindings]] mode = "visual", key = "l", command = "editor.move_right"
[[bindings]] mode = "visual", key = "ArrowLeft", command = "editor.move_left"
[[bindings]] mode = "visual", key = "ArrowRight", command = "editor.move_right"
[[bindings]] mode = "visual", key = "ArrowUp", command = "editor.move_up"
[[bindings]] mode = "visual", key = "ArrowDown", command = "editor.move_down"
[[bindings]] mode = "visual", key = "Ctrl+j", command = "selection.move_down"
[[bindings]] mode = "visual", key = "Ctrl+k", command = "selection.move_up"
[[bindings]] mode = "visual", key = "<leader>c a", command = "lsp.range_code_action"
[[bindings]] mode = "visual", key = "<leader>a a", command = "ai.actions"
[[bindings]] mode = "visual", key = "backtick", command = "app.toggle_terminal"

# ───────────────── TERMINAL MODE ─────────────────
[[bindings]] mode = "terminal", key = "Escape", command = "app.focus_editor"
[[bindings]] mode = "terminal", key = "Ctrl+q", command = "terminal.exit_to_normal_mode"
[[bindings]] mode = "terminal", key = "mod+backslash", command = "app.toggle_terminal"
[[bindings]] mode = "terminal", key = "backtick", command = "app.toggle_terminal"

# ───────────────── EXPLORER MODE ─────────────────
[[bindings]] mode = "explorer", key = "Escape", command = "app.focus_editor"
[[bindings]] mode = "explorer", key = "j", command = "explorer.move_down"
[[bindings]] mode = "explorer", key = "ArrowDown", command = "explorer.move_down"
[[bindings]] mode = "explorer", key = "k", command = "explorer.move_up"
[[bindings]] mode = "explorer", key = "ArrowUp", command = "explorer.move_up"
[[bindings]] mode = "explorer", key = "h", command = "explorer.collapse_or_parent"
[[bindings]] mode = "explorer", key = "ArrowLeft", command = "explorer.collapse_or_parent"
[[bindings]] mode = "explorer", key = "l", command = "explorer.expand_or_child"
[[bindings]] mode = "explorer", key = "ArrowRight", command = "explorer.expand_or_child"
[[bindings]] mode = "explorer", key = "Enter", command = "explorer.toggle_or_open"
[[bindings]] mode = "explorer", key = "g g", command = "explorer.move_to_top"
[[bindings]] mode = "explorer", key = "G", command = "explorer.move_to_bottom"
[[bindings]] mode = "explorer", key = "H", command = "explorer.toggle_hidden"
[[bindings]] mode = "explorer", key = "I", command = "explorer.toggle_ignored"
[[bindings]] mode = "explorer", key = "r", command = "explorer.rename_full"
[[bindings]] mode = "explorer", key = "R", command = "explorer.rename_base"

# ───────────────── WINDOW / PANEL FOCUS ─────────────────
[[bindings]] mode = "normal", key = "<leader>h", command = "app.focus_explorer"
[[bindings]] mode = "normal", key = "<leader>l", command = "app.focus_inspector"
[[bindings]] mode = "normal", key = "<leader>j", command = "app.focus_terminal"
[[bindings]] mode = "normal", key = "<leader>k", command = "app.focus_editor"

[[bindings]] mode = "normal", key = "<leader>e", command = "app.focus_explorer"
[[bindings]] mode = "normal", key = "<leader>i", command = "app.focus_inspector"
[[bindings]] mode = "normal", key = "<leader>b", command = "app.focus_terminal"

# ───────────────── FILE / SEARCH / TELESCOPE-LIKE ─────────────────
[[bindings]] mode = "normal", key = "<leader>p", command = "app.open_command_palette"
[[bindings]] mode = "normal", key = "<leader>p j", command = "projects.recent"

[[bindings]] mode = "normal", key = "<leader>f f", command = "app.open_file_finder"
[[bindings]] mode = "normal", key = "<leader>f a", command = "files.find_all_hidden_no_ignore"
[[bindings]] mode = "normal", key = "<leader>f w", command = "app.search_in_files"
[[bindings]] mode = "normal", key = "<leader>f b", command = "search.buffers"
[[bindings]] mode = "normal", key = "<leader>f h", command = "search.help_tags"
[[bindings]] mode = "normal", key = "<leader>f o", command = "search.old_files"
[[bindings]] mode = "normal", key = "<leader>f p", command = "app.open_workspace_symbols"

# ───────────────── BUFFER / TAB ─────────────────
[[bindings]] mode = "normal", key = "<leader>b", command = "buffer.new"
[[bindings]] mode = "normal", key = "Ctrl+l", command = "buffer.next"
[[bindings]] mode = "normal", key = "Ctrl+h", command = "buffer.prev"
[[bindings]] mode = "normal", key = "<leader>x", command = "buffer.close_current"
[[bindings]] mode = "normal", key = "<leader>t n", command = "app.next_panel_tab"

# ───────────────── TERMINAL / SAVE ─────────────────
[[bindings]] mode = "normal", key = "<leader>t", command = "app.toggle_terminal"
[[bindings]] mode = "normal", key = "<leader>w", command = "editor.save_file"

# ───────────────── LSP / DIAGNOSTICS ─────────────────
[[bindings]] mode = "normal", key = "K", command = "lsp.hover"
[[bindings]] mode = "normal", key = "g d", command = "lsp.definition"
[[bindings]] mode = "normal", key = "g i", command = "lsp.implementation"
[[bindings]] mode = "normal", key = "g r", command = "lsp.references"
[[bindings]] mode = "normal", key = "Ctrl+i", command = "lsp.signature_help"
[[bindings]] mode = "insert", key = "Ctrl+Space", command = "lsp.trigger_completion"
[[bindings]] mode = "insert", key = "mod+Space", command = "lsp.trigger_completion"
[[bindings]] mode = "normal", key = "<leader>D", command = "lsp.type_definition"
[[bindings]] mode = "normal", key = "<leader>r n", command = "lsp.rename"
[[bindings]] mode = "normal", key = "<leader>c a", command = "lsp.code_action"
[[bindings]] mode = "normal", key = "[ d", command = "diagnostics.prev"
[[bindings]] mode = "normal", key = "] d", command = "diagnostics.next"
[[bindings]] mode = "normal", key = "<leader>q", command = "diagnostics.to_loclist"
[[bindings]] mode = "normal", key = "<leader>d s", command = "diagnostics.open_picker"

# ───────────────── GIT ─────────────────
[[bindings]] mode = "normal", key = "<leader>g f", command = "git.open_lazygit"
[[bindings]] mode = "normal", key = "<leader>g b", command = "git.blame"
[[bindings]] mode = "normal", key = "<leader>g l", command = "git.blame_line"
[[bindings]] mode = "normal", key = "<leader>g t", command = "git.search_status"
[[bindings]] mode = "normal", key = "<leader>p h", command = "git.preview_hunk"
[[bindings]] mode = "normal", key = "<leader>r h", command = "git.reset_hunk"
[[bindings]] mode = "normal", key = "<leader>c m", command = "git.search_commits"

# ───────────────── AI ─────────────────
[[bindings]] mode = "normal", key = "<leader>a a", command = "ai.actions"
[[bindings]] mode = "normal", key = "<leader>a i", command = "ai.inline"
[[bindings]] mode = "normal", key = "<leader>a c", command = "ai.chat_toggle"
[[bindings]] mode = "normal", key = "<leader>a m", command = "ai.command"