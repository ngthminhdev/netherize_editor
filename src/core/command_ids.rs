use std::path::PathBuf;

use crate::core::{commands::Command, mode::ModeEvent};

// ── Editor movement & editing ────────────────────────────────────────────────
pub const MOVE_LEFT: &str = "editor.move_left";
pub const MOVE_RIGHT: &str = "editor.move_right";
pub const MOVE_UP: &str = "editor.move_up";
pub const MOVE_DOWN: &str = "editor.move_down";
pub const MOVE_WORD_FORWARD: &str = "editor.move_word_forward";
pub const MOVE_WORD_BACKWARD: &str = "editor.move_word_backward";
pub const MOVE_WORD_END: &str = "editor.move_word_end";
pub const MOVE_TO_LINE_START: &str = "editor.move_to_line_start";
pub const MOVE_TO_LINE_END: &str = "editor.move_to_line_end";
pub const MOVE_TO_FIRST_NON_WHITESPACE: &str = "editor.move_to_first_non_whitespace";
pub const MOVE_TO_FIRST_LINE: &str = "editor.move_to_first_line";
pub const MOVE_TO_LAST_LINE: &str = "editor.move_to_last_line";
pub const MOVE_PARAGRAPH_UP: &str = "editor.move_paragraph_up";
pub const MOVE_PARAGRAPH_DOWN: &str = "editor.move_paragraph_down";
pub const SCROLL_HALF_PAGE_UP: &str = "editor.scroll_half_page_up";
pub const SCROLL_HALF_PAGE_DOWN: &str = "editor.scroll_half_page_down";
pub const CENTER_CURSOR_LINE: &str = "editor.center_cursor_line";
pub const OPEN_IN_FILE_SEARCH: &str = "editor.open_in_file_search";
pub const SEARCH_NEXT: &str = "editor.search_next";
pub const SEARCH_PREV: &str = "editor.search_prev";
pub const SEARCH_WORD_UNDER_CURSOR: &str = "editor.search_word_under_cursor";
pub const CLEAR_SEARCH_HIGHLIGHTS: &str = "editor.clear_search_highlights";
pub const BACKSPACE: &str = "editor.backspace";
pub const NEWLINE: &str = "editor.newline";
pub const INSERT_TAB: &str = "editor.insert_tab";
pub const INSERT_LINE_BELOW: &str = "editor.insert_line_below";
pub const INSERT_LINE_ABOVE: &str = "editor.insert_line_above";
pub const INSERT_AT_LINE_START: &str = "editor.insert_at_line_start";
pub const APPEND_AT_LINE_END: &str = "editor.append_at_line_end";
pub const APPEND_AFTER_CURSOR: &str = "editor.append_after_cursor";
pub const SUBSTITUTE_LINE: &str = "editor.substitute_line";
pub const DELETE_CHAR: &str = "editor.delete_char";
pub const DELETE_SELECTION: &str = "editor.delete_selection";
pub const DELETE_CURRENT_LINE: &str = "editor.delete_current_line";
pub const DELETE_TO_LINE_END: &str = "editor.delete_to_line_end";
pub const TOGGLE_LINE_COMMENT: &str = "editor.toggle_line_comment";
pub const TOGGLE_SELECTION_COMMENT: &str = "editor.toggle_selection_comment";
pub const WRAP_SELECTION_WITH_STAR: &str = "editor.wrap_selection_with_star";
pub const DELETE_WORD_FORWARD: &str = "editor.delete_word_forward";
pub const DELETE_WORD_BACKWARD: &str = "editor.delete_word_backward";
pub const YANK_SELECTION: &str = "editor.yank_selection";
pub const CHANGE_SELECTION: &str = "editor.change_selection";
pub const CHANGE_WORD_FORWARD: &str = "editor.change_word_forward";
pub const CHANGE_WORD_BACKWARD: &str = "editor.change_word_backward";
pub const CHANGE_TO_LINE_END: &str = "editor.change_to_line_end";
pub const YANK_TO_LINE_END: &str = "editor.yank_to_line_end";
pub const JOIN_LINES: &str = "editor.join_lines";
pub const PASTE_AFTER: &str = "editor.paste_after";
pub const PASTE_BEFORE: &str = "editor.paste_before";
pub const EDITOR_PASTE: &str = "editor.paste";
pub const PASTE_SYSTEM_CLIPBOARD: &str = "editor.paste_system_clipboard";
pub const VISUAL_PASTE: &str = "editor.visual_paste";
pub const UNDO: &str = "editor.undo";
pub const REDO: &str = "editor.redo";
pub const SAVE_FILE: &str = "editor.save_file";
pub const OPEN_FILE: &str = "editor.open_file";
pub const NEW_INSTANCE: &str = "app.new_instance";

// ── Leap / EasyMotion navigation ─────────────────────────────────────────────
pub const LEAP_START: &str = "editor.leap_start";

// ── Mode transitions ──────────────────────────────────────────────────────────
pub const ENTER_NORMAL: &str = "mode.enter_normal";
pub const ENTER_INSERT: &str = "mode.enter_insert";
pub const ENTER_VISUAL: &str = "mode.enter_visual";
pub const ENTER_VISUAL_LINE: &str = "mode.enter_visual_line";
pub const ENTER_VISUAL_BLOCK: &str = "mode.enter_visual_block";
pub const ENTER_RESIZE: &str = "mode.resize";
pub const ENTER_TERMINAL_FOCUS: &str = "mode.enter_terminal_focus";
pub const EXIT_FOCUS: &str = "mode.exit_focus";
pub const ESCAPE_MODE: &str = "mode.escape";

// ── Workspace / Project management ───────────────────────────────────────────
pub const OPEN_FOLDER: &str = "editor.open_folder";
pub const OPEN_RECENT_PROJECTS: &str = "projects.recent";

// ── App-level UI ──────────────────────────────────────────────────────────────
pub const TOGGLE_TERMINAL: &str = "app.toggle_terminal";
pub const TOGGLE_BOTTOM_DOCK: &str = "app.toggle_bottom_dock";
pub const RUN_TEST_CASES: &str = "runner.run";
pub const NEW_LEETCODE_FILE: &str = "runner.new_leetcode_file";
pub const FETCH_LEETCODE_PROBLEM: &str = "runner.fetch_leetcode_problem";
pub const TOGGLE_LEFT_DOCK: &str = "app.toggle_left_dock";
pub const OPEN_FILE_PICKER: &str = "app.open_file_picker";
pub const OPEN_FILE_FINDER: &str = "app.open_file_finder";
pub const OPEN_COMMAND_PALETTE: &str = "app.open_command_palette";
pub const OPEN_VIM_COMMAND: &str = "app.open_vim_command";
pub const OPEN_WORKSPACE_SYMBOLS: &str = "app.open_workspace_symbols";
pub const OPEN_DOCUMENT_SYMBOLS: &str = "app.open_document_symbols";
pub const SEARCH_IN_FILES: &str = "app.search_in_files";
pub const OPEN_THEME_SELECTOR: &str = "app.open_theme_selector";
pub const OPEN_FILE_HISTORY: &str = "app.open_file_history";
pub const OPEN_SETTINGS: &str = "app.open_settings";
pub const OPEN_HELP: &str = "app.open_help";
pub const OPEN_EXTENSIONS_MANAGER: &str = "app.open_extensions_manager";
pub const GIT_OPEN_LAZYGIT: &str = "git.open_lazygit";
pub const DOCKER_OPEN_LAZYDOCKER: &str = "docker.open_lazydocker";
pub const GIT_BLAME_LINE: &str = "git.blame_line";
pub const TERMINAL_ENTER_NORMAL_MODE: &str = "terminal.enter_normal_mode";
pub const TERMINAL_PASTE: &str = "terminal.paste";
pub const TERMINAL_SEARCH_OPEN: &str = "terminal.search_open";
pub const TERMINAL_TAB_NEW: &str = "terminal.tab_new";
pub const TERMINAL_TAB_CLOSE: &str = "terminal.tab_close";
pub const TERMINAL_TAB_SWITCH_1: &str = "terminal.tab_switch_1";
pub const TERMINAL_TAB_SWITCH_2: &str = "terminal.tab_switch_2";
pub const TERMINAL_TAB_SWITCH_3: &str = "terminal.tab_switch_3";
pub const TERMINAL_TAB_SWITCH_4: &str = "terminal.tab_switch_4";
pub const TERMINAL_TAB_SWITCH_5: &str = "terminal.tab_switch_5";
pub const TERMINAL_TAB_SWITCH_6: &str = "terminal.tab_switch_6";
pub const TERMINAL_TAB_SWITCH_7: &str = "terminal.tab_switch_7";
pub const TERMINAL_TAB_SWITCH_8: &str = "terminal.tab_switch_8";
pub const TERMINAL_TAB_SWITCH_9: &str = "terminal.tab_switch_9";

// ── Right dock tab switch ───────────────────────────────────────────────────
pub const RIGHT_DOCK_SWITCH_1: &str = "right_dock.switch_tab_1";
pub const RIGHT_DOCK_SWITCH_2: &str = "right_dock.switch_tab_2";
pub const RIGHT_DOCK_SWITCH_3: &str = "right_dock.switch_tab_3";
pub const RIGHT_DOCK_SWITCH_4: &str = "right_dock.switch_tab_4";
pub const RIGHT_DOCK_SWITCH_5: &str = "right_dock.switch_tab_5";
pub const RIGHT_DOCK_SWITCH_6: &str = "right_dock.switch_tab_6";
pub const RIGHT_DOCK_SWITCH_7: &str = "right_dock.switch_tab_7";
pub const RIGHT_DOCK_SWITCH_8: &str = "right_dock.switch_tab_8";
pub const RIGHT_DOCK_SWITCH_9: &str = "right_dock.switch_tab_9";

// ── Buffer goto ─────────────────────────────────────────────────────────────
pub const BUFFER_GOTO_1: &str = "buffer.goto_1";
pub const BUFFER_GOTO_2: &str = "buffer.goto_2";
pub const BUFFER_GOTO_3: &str = "buffer.goto_3";
pub const BUFFER_GOTO_4: &str = "buffer.goto_4";
pub const BUFFER_GOTO_5: &str = "buffer.goto_5";
pub const BUFFER_GOTO_6: &str = "buffer.goto_6";
pub const BUFFER_GOTO_7: &str = "buffer.goto_7";
pub const BUFFER_GOTO_8: &str = "buffer.goto_8";
pub const BUFFER_GOTO_9: &str = "buffer.goto_9";

// ── LSP Interactive (Module 10) ──────────────────────────────────────────────────
pub const LSP_HOVER: &str = "lsp.hover";
pub const LSP_GO_TO_DEFINITION: &str = "lsp.go_to_definition";
pub const LSP_PREVIEW_DEFINITION: &str = "lsp.preview_definition";
pub const LSP_REFERENCES: &str = "lsp.references";
pub const LSP_RENAME: &str = "lsp.rename";
pub const LSP_FORMAT_DOCUMENT: &str = "lsp.format_document";
pub const LSP_TRIGGER_COMPLETION: &str = "lsp.trigger_completion";
pub const LSP_CODE_ACTION: &str = "lsp.code_action";
pub const LSP_SELECT_PYTHON_ENV: &str = "lsp.select_python_env";
pub const LSP_SELECT_DART_ENV: &str = "lsp.select_dart_env";
pub const RELOAD_WORKSPACE: &str = "workspace.reload";
pub const RESIZE_DECREASE_WIDTH: &str = "resize.decrease_width";
pub const RESIZE_INCREASE_WIDTH: &str = "resize.increase_width";
pub const RESIZE_DECREASE_HEIGHT: &str = "resize.decrease_height";
pub const RESIZE_INCREASE_HEIGHT: &str = "resize.increase_height";
pub const RESIZE_GROW_LEFT_DOCK: &str = "resize.grow_left_dock";
pub const RESIZE_GROW_RIGHT_DOCK: &str = "resize.grow_right_dock";
pub const COMPLETION_NEXT: &str = "completion.next";
pub const COMPLETION_PREV: &str = "completion.prev";
pub const COMPLETION_ACCEPT: &str = "completion.accept";
pub const COMPLETION_CLOSE: &str = "completion.close";
pub const AI_ACCEPT_INLINE: &str = "ai.accept_inline";
pub const AI_ACCEPT_INLINE_WORD: &str = "ai.accept_inline_word";

// ── AI Chat ──────────────────────────────────────────────────────────────
pub const AI_CHAT_TOGGLE: &str = "ai.chat_toggle";
pub const AI_CHAT_SEND: &str = "ai.chat_send";
pub const AI_CHAT_STOP: &str = "ai.chat_stop";
pub const AI_CHAT_CLOSE: &str = "ai.chat_close";
pub const AI_CHAT_UNFOCUS: &str = "ai.chat_unfocus";
pub const AI_CHAT_FOCUS: &str = "ai.chat_focus";
pub const AI_CHAT_ADD_SELECTION_CONTEXT: &str = "ai.chat_add_selection_context";
pub const DIAGNOSTICS_OPEN_PICKER: &str = "diagnostics.open_picker";

// ── Jump list navigation ─────────────────────────────────────────────────────
pub const JUMP_BACK: &str = "editor.jump_back";
pub const JUMP_FORWARD: &str = "editor.jump_forward";

// ── Multiple Cursors ─────────────────────────────────────────────────────────
pub const MULTI_CURSOR_ADD_NEXT: &str = "multicursor.add_next";
pub const MULTI_CURSOR_SKIP: &str = "multicursor.skip";
pub const MULTI_CURSOR_INSERT_BEFORE: &str = "multicursor.insert_before";
pub const MULTI_CURSOR_APPEND_AFTER: &str = "multicursor.append_after";
pub const MULTI_CURSOR_CHANGE: &str = "multicursor.change";
pub const MULTI_CURSOR_DELETE: &str = "multicursor.delete";
pub const MULTI_CURSOR_SELECT_ALL: &str = "multicursor.select_all";

// ── Code folding ────────────────────────────────────────────────────────────
pub const TOGGLE_FOLD: &str = "editor.toggle_fold";
pub const TOGGLE_FOLD_ALL: &str = "editor.toggle_fold_all";

// ── Markdown Preview ─────────────────────────────────────────────────────────
pub const TOGGLE_MARKDOWN_PREVIEW: &str = "app.toggle_markdown_preview";
pub const CLOSE_SIDEBARS: &str = "app.close_sidebars";
pub const FOCUS_MARKDOWN_PREVIEW: &str = "app.focus_markdown_preview";
pub const MARKDOWN_PREVIEW_SCROLL_UP: &str = "app.markdown_preview_scroll_up";
pub const MARKDOWN_PREVIEW_SCROLL_DOWN: &str = "app.markdown_preview_scroll_down";
pub const MARKDOWN_PREVIEW_SCROLL_HALF_PAGE_UP: &str = "app.markdown_preview_scroll_half_page_up";
pub const MARKDOWN_PREVIEW_SCROLL_HALF_PAGE_DOWN: &str =
    "app.markdown_preview_scroll_half_page_down";
pub const MARKDOWN_PREVIEW_SCROLL_TOP: &str = "app.markdown_preview_scroll_top";
pub const MARKDOWN_PREVIEW_SCROLL_BOTTOM: &str = "app.markdown_preview_scroll_bottom";

// ── Help / Cheat Sheet ───────────────────────────────────────────────────────
pub const HELP_SCROLL_DOWN: &str = "app.help_scroll_down";
pub const HELP_SCROLL_UP: &str = "app.help_scroll_up";
pub const HELP_SCROLL_HALF_PAGE_DOWN: &str = "app.help_scroll_half_page_down";
pub const HELP_SCROLL_HALF_PAGE_UP: &str = "app.help_scroll_half_page_up";

// ── Workbench focus navigation ────────────────────────────────────────────────
pub const FOCUS_EDITOR: &str = "app.focus_editor";
pub const FOCUS_EXPLORER: &str = "app.focus_explorer";
pub const FOCUS_TERMINAL: &str = "app.focus_terminal";
pub const FOCUS_INSPECTOR: &str = "app.focus_inspector";
pub const FOCUS_LEFT: &str = "app.focus_left";
pub const FOCUS_RIGHT: &str = "app.focus_right";
pub const FOCUS_UP: &str = "app.focus_up";
pub const FOCUS_DOWN: &str = "app.focus_down";
pub const MOVE_FOCUS_CYCLE: &str = "app.move_focus_cycle";
pub const FOCUS_BACK: &str = "app.focus_back";
pub const TOGGLE_MAXIMIZE_FOCUS: &str = "app.toggle_maximize_focus";

// ── Panel tabs ────────────────────────────────────────────────────────────────
pub const NEXT_PANEL_TAB: &str = "app.next_panel_tab";
pub const PREV_PANEL_TAB: &str = "app.prev_panel_tab";

// ── Buffer management ────────────────────────────────────────────────────────
pub const BUFFER_NEW: &str = "buffer.new";
pub const BUFFER_NEXT: &str = "buffer.next";
pub const BUFFER_PREV: &str = "buffer.prev";
pub const BUFFER_CLOSE_CURRENT: &str = "buffer.close_current";

// ── Explorer surface ──────────────────────────────────────────────────────────
pub const EXPLORER_MOVE_UP: &str = "explorer.move_up";
pub const EXPLORER_MOVE_DOWN: &str = "explorer.move_down";
pub const EXPLORER_COLLAPSE_NODE: &str = "explorer.collapse_node";
pub const EXPLORER_COLLAPSE_OR_PARENT: &str = "explorer.collapse_or_parent";
pub const EXPLORER_EXPAND_NODE: &str = "explorer.expand_node";
pub const EXPLORER_COLLAPSE_ALL_UNDER_NODE: &str = "explorer.collapse_all_under_node";
pub const EXPLORER_EXPAND_OR_CHILD: &str = "explorer.expand_or_child";
pub const EXPLORER_EXPAND_ALL_UNDER_NODE: &str = "explorer.expand_all_under_node";
pub const EXPLORER_TOGGLE_OR_OPEN: &str = "explorer.toggle_or_open";
pub const EXPLORER_DELETE_NODE: &str = "explorer.delete_node";
pub const EXPLORER_CREATE_FILE: &str = "explorer.create_file";
pub const EXPLORER_CREATE_FOLDER: &str = "explorer.create_folder";
pub const EXPLORER_START_FILTER: &str = "explorer.start_filter";
pub const EXPLORER_CLEAR_FILTER: &str = "explorer.clear_filter";
pub const EXPLORER_TOGGLE_HIDDEN: &str = "explorer.toggle_hidden";
pub const EXPLORER_TOGGLE_IGNORED: &str = "explorer.toggle_ignored";
pub const EXPLORER_TOGGLE_GIT_CHANGES_ONLY: &str = "explorer.toggle_git_changes_only";
pub const EXPLORER_MOVE_TO_TOP: &str = "explorer.move_to_top";
pub const EXPLORER_MOVE_TO_BOTTOM: &str = "explorer.move_to_bottom";
pub const EXPLORER_RENAME_FULL: &str = "explorer.rename_full";
pub const EXPLORER_RENAME_BASE: &str = "explorer.rename_base";
pub const EXPLORER_COPY_FILE: &str = "explorer.copy_file";
pub const EXPLORER_PASTE_FILE: &str = "explorer.paste_file";
// Legacy command IDs.
pub const EXPLORER_EXPAND_COLLAPSE: &str = "explorer.expand_collapse";
pub const EXPLORER_OPEN_FILE: &str = "explorer.open_file";

// ── File picker ───────────────────────────────────────────────────────────────
pub const OVERLAY_SELECT_NEXT: &str = "overlay.select_next";
pub const OVERLAY_SELECT_PREV: &str = "overlay.select_prev";
pub const TOGGLE_LIVE_GREP_CASE_SENSITIVE: &str = "search.toggle_case_sensitive";
pub const TOGGLE_IN_FILE_SEARCH_CASE_SENSITIVE: &str = "editor.search.toggle_case_sensitive";
pub const SETTINGS_SELECT_NEXT: &str = "settings.select_next";
pub const SETTINGS_SELECT_PREV: &str = "settings.select_prev";
pub const SETTINGS_ADJUST_DECREASE: &str = "settings.adjust_decrease";
pub const SETTINGS_ADJUST_INCREASE: &str = "settings.adjust_increase";
pub const SETTINGS_ACTIVATE: &str = "settings.activate";
pub const FILE_PICKER_CONFIRM: &str = "file_picker.confirm";
pub const FILE_PICKER_CLOSE: &str = "file_picker.close";
pub const FILE_PICKER_SELECT_NEXT: &str = "file_picker.select_next";
pub const FILE_PICKER_SELECT_PREV: &str = "file_picker.select_prev";
pub const FILE_PICKER_BACKSPACE: &str = "file_picker.backspace";

pub const ALL_IDS: &[&str] = &[
    OPEN_FOLDER,
    OPEN_RECENT_PROJECTS,
    MOVE_LEFT,
    MOVE_RIGHT,
    MOVE_UP,
    MOVE_DOWN,
    MOVE_WORD_FORWARD,
    MOVE_WORD_BACKWARD,
    MOVE_WORD_END,
    MOVE_TO_LINE_START,
    MOVE_TO_LINE_END,
    MOVE_TO_FIRST_NON_WHITESPACE,
    MOVE_TO_FIRST_LINE,
    MOVE_TO_LAST_LINE,
    SCROLL_HALF_PAGE_UP,
    SCROLL_HALF_PAGE_DOWN,
    CENTER_CURSOR_LINE,
    OPEN_IN_FILE_SEARCH,
    SEARCH_NEXT,
    SEARCH_PREV,
    SEARCH_WORD_UNDER_CURSOR,
    CLEAR_SEARCH_HIGHLIGHTS,
    BACKSPACE,
    NEWLINE,
    INSERT_TAB,
    INSERT_LINE_BELOW,
    INSERT_LINE_ABOVE,
    INSERT_AT_LINE_START,
    APPEND_AT_LINE_END,
    APPEND_AFTER_CURSOR,
    SUBSTITUTE_LINE,
    DELETE_CHAR,
    DELETE_SELECTION,
    DELETE_CURRENT_LINE,
    TOGGLE_LINE_COMMENT,
    TOGGLE_SELECTION_COMMENT,
    WRAP_SELECTION_WITH_STAR,
    DELETE_WORD_FORWARD,
    DELETE_WORD_BACKWARD,
    YANK_SELECTION,
    CHANGE_SELECTION,
    CHANGE_WORD_FORWARD,
    CHANGE_WORD_BACKWARD,
    PASTE_AFTER,
    PASTE_BEFORE,
    EDITOR_PASTE,
    PASTE_SYSTEM_CLIPBOARD,
    VISUAL_PASTE,
    UNDO,
    REDO,
    SAVE_FILE,
    OPEN_FILE,
    ENTER_NORMAL,
    ENTER_INSERT,
    ENTER_VISUAL,
    ENTER_VISUAL_LINE,
    ENTER_RESIZE,
    RESIZE_DECREASE_WIDTH,
    RESIZE_INCREASE_WIDTH,
    RESIZE_DECREASE_HEIGHT,
    RESIZE_INCREASE_HEIGHT,
    RESIZE_GROW_LEFT_DOCK,
    RESIZE_GROW_RIGHT_DOCK,
    ENTER_TERMINAL_FOCUS,
    EXIT_FOCUS,
    ESCAPE_MODE,
    TOGGLE_TERMINAL,
    TOGGLE_BOTTOM_DOCK,
    RUN_TEST_CASES,
    NEW_LEETCODE_FILE,
    FETCH_LEETCODE_PROBLEM,
    TOGGLE_LEFT_DOCK,
    OPEN_FILE_PICKER,
    OPEN_FILE_FINDER,
    OPEN_COMMAND_PALETTE,
    OPEN_VIM_COMMAND,
    OPEN_WORKSPACE_SYMBOLS,
    OPEN_DOCUMENT_SYMBOLS,
    SEARCH_IN_FILES,
    OPEN_THEME_SELECTOR,
    OPEN_FILE_HISTORY,
    OPEN_SETTINGS,
    OPEN_EXTENSIONS_MANAGER,
    GIT_OPEN_LAZYGIT,
    DOCKER_OPEN_LAZYDOCKER,
    GIT_BLAME_LINE,
    TERMINAL_ENTER_NORMAL_MODE,
    TERMINAL_PASTE,
    TERMINAL_SEARCH_OPEN,
    TERMINAL_TAB_NEW,
    TERMINAL_TAB_CLOSE,
    TERMINAL_TAB_SWITCH_1,
    TERMINAL_TAB_SWITCH_2,
    TERMINAL_TAB_SWITCH_3,
    TERMINAL_TAB_SWITCH_4,
    TERMINAL_TAB_SWITCH_5,
    TERMINAL_TAB_SWITCH_6,
    TERMINAL_TAB_SWITCH_7,
    TERMINAL_TAB_SWITCH_8,
    TERMINAL_TAB_SWITCH_9,
    RIGHT_DOCK_SWITCH_1,
    RIGHT_DOCK_SWITCH_2,
    RIGHT_DOCK_SWITCH_3,
    RIGHT_DOCK_SWITCH_4,
    RIGHT_DOCK_SWITCH_5,
    RIGHT_DOCK_SWITCH_6,
    RIGHT_DOCK_SWITCH_7,
    RIGHT_DOCK_SWITCH_8,
    RIGHT_DOCK_SWITCH_9,
    BUFFER_GOTO_1,
    BUFFER_GOTO_2,
    BUFFER_GOTO_3,
    BUFFER_GOTO_4,
    BUFFER_GOTO_5,
    BUFFER_GOTO_6,
    BUFFER_GOTO_7,
    BUFFER_GOTO_8,
    BUFFER_GOTO_9,
    LSP_HOVER,
    LSP_GO_TO_DEFINITION,
    LSP_PREVIEW_DEFINITION,
    LSP_REFERENCES,
    LSP_RENAME,
    LSP_FORMAT_DOCUMENT,
    LSP_TRIGGER_COMPLETION,
    LSP_CODE_ACTION,
    LSP_SELECT_PYTHON_ENV,
    LSP_SELECT_DART_ENV,
    RELOAD_WORKSPACE,
    COMPLETION_NEXT,
    COMPLETION_PREV,
    COMPLETION_ACCEPT,
    COMPLETION_CLOSE,
    AI_ACCEPT_INLINE,
    AI_ACCEPT_INLINE_WORD,
    AI_CHAT_TOGGLE,
    AI_CHAT_SEND,
    AI_CHAT_CLOSE,
    AI_CHAT_UNFOCUS,
    AI_CHAT_FOCUS,
    AI_CHAT_ADD_SELECTION_CONTEXT,
    DIAGNOSTICS_OPEN_PICKER,
    JUMP_BACK,
    JUMP_FORWARD,
    FOCUS_EDITOR,
    FOCUS_EXPLORER,
    FOCUS_TERMINAL,
    FOCUS_INSPECTOR,
    FOCUS_LEFT,
    FOCUS_RIGHT,
    FOCUS_UP,
    FOCUS_DOWN,
    MOVE_FOCUS_CYCLE,
    FOCUS_BACK,
    NEXT_PANEL_TAB,
    PREV_PANEL_TAB,
    BUFFER_NEW,
    BUFFER_NEXT,
    BUFFER_PREV,
    BUFFER_CLOSE_CURRENT,
    EXPLORER_MOVE_UP,
    EXPLORER_MOVE_DOWN,
    EXPLORER_COLLAPSE_NODE,
    EXPLORER_COLLAPSE_OR_PARENT,
    EXPLORER_EXPAND_NODE,
    EXPLORER_COLLAPSE_ALL_UNDER_NODE,
    EXPLORER_EXPAND_OR_CHILD,
    EXPLORER_EXPAND_ALL_UNDER_NODE,
    EXPLORER_TOGGLE_OR_OPEN,
    EXPLORER_DELETE_NODE,
    EXPLORER_CREATE_FILE,
    EXPLORER_CREATE_FOLDER,
    EXPLORER_START_FILTER,
    EXPLORER_CLEAR_FILTER,
    EXPLORER_TOGGLE_HIDDEN,
    EXPLORER_TOGGLE_IGNORED,
    EXPLORER_TOGGLE_GIT_CHANGES_ONLY,
    EXPLORER_MOVE_TO_TOP,
    EXPLORER_MOVE_TO_BOTTOM,
    EXPLORER_RENAME_FULL,
    EXPLORER_RENAME_BASE,
    EXPLORER_COPY_FILE,
    EXPLORER_PASTE_FILE,
    EXPLORER_EXPAND_COLLAPSE,
    EXPLORER_OPEN_FILE,
    OVERLAY_SELECT_NEXT,
    OVERLAY_SELECT_PREV,
    TOGGLE_LIVE_GREP_CASE_SENSITIVE,
    TOGGLE_IN_FILE_SEARCH_CASE_SENSITIVE,
    SETTINGS_SELECT_NEXT,
    SETTINGS_SELECT_PREV,
    SETTINGS_ADJUST_DECREASE,
    SETTINGS_ADJUST_INCREASE,
    SETTINGS_ACTIVATE,
    FILE_PICKER_CONFIRM,
    FILE_PICKER_CLOSE,
    FILE_PICKER_SELECT_NEXT,
    FILE_PICKER_SELECT_PREV,
    FILE_PICKER_BACKSPACE,
    LEAP_START,
    MULTI_CURSOR_ADD_NEXT,
    MULTI_CURSOR_SKIP,
    MULTI_CURSOR_INSERT_BEFORE,
    MULTI_CURSOR_APPEND_AFTER,
    MULTI_CURSOR_CHANGE,
    MULTI_CURSOR_DELETE,
    MULTI_CURSOR_SELECT_ALL,
    TOGGLE_MARKDOWN_PREVIEW,
    CLOSE_SIDEBARS,
    FOCUS_MARKDOWN_PREVIEW,
    MARKDOWN_PREVIEW_SCROLL_UP,
    MARKDOWN_PREVIEW_SCROLL_DOWN,
    MARKDOWN_PREVIEW_SCROLL_HALF_PAGE_UP,
    MARKDOWN_PREVIEW_SCROLL_HALF_PAGE_DOWN,
    HELP_SCROLL_DOWN,
    HELP_SCROLL_UP,
    HELP_SCROLL_HALF_PAGE_DOWN,
    HELP_SCROLL_HALF_PAGE_UP,
    TOGGLE_MAXIMIZE_FOCUS,
    TOGGLE_FOLD,
    TOGGLE_FOLD_ALL,
    NEW_INSTANCE,
    ENTER_VISUAL_BLOCK,
    CHANGE_TO_LINE_END,
    DELETE_TO_LINE_END,
    YANK_TO_LINE_END,
    JOIN_LINES,
    MOVE_PARAGRAPH_UP,
    MOVE_PARAGRAPH_DOWN,
    AI_CHAT_STOP,
    OPEN_HELP,
    MARKDOWN_PREVIEW_SCROLL_TOP,
    MARKDOWN_PREVIEW_SCROLL_BOTTOM,
];

/// A command id is valid if it is registered in ALL_IDS or resolvable by
/// `parse` — the fallback keeps keymap validation in sync with the dispatcher
/// so an id added to `parse` but forgotten here no longer silently disables
/// its keybinding.
pub fn is_valid(id: &str) -> bool {
    ALL_IDS.contains(&id) || parse(id, None).is_some()
}

pub fn parse(id: &str, open_file_path: Option<&std::path::Path>) -> Option<Command> {
    match id {
        MOVE_LEFT => Some(Command::MoveLeft),
        MOVE_RIGHT => Some(Command::MoveRight),
        MOVE_UP => Some(Command::MoveUp),
        MOVE_DOWN => Some(Command::MoveDown),
        MOVE_WORD_FORWARD => Some(Command::MoveWordForward),
        MOVE_WORD_BACKWARD => Some(Command::MoveWordBackward),
        MOVE_WORD_END => Some(Command::MoveWordEnd),
        MOVE_TO_LINE_START => Some(Command::MoveToLineStart),
        MOVE_TO_LINE_END => Some(Command::MoveToLineEnd),
        MOVE_TO_FIRST_NON_WHITESPACE => Some(Command::MoveToFirstNonWhitespace),
        MOVE_TO_FIRST_LINE => Some(Command::MoveToFirstLine),
        MOVE_TO_LAST_LINE => Some(Command::MoveToLastLine),
        MOVE_PARAGRAPH_UP => Some(Command::MoveParagraphUp),
        MOVE_PARAGRAPH_DOWN => Some(Command::MoveParagraphDown),
        SCROLL_HALF_PAGE_UP => Some(Command::ScrollHalfPageUp),
        SCROLL_HALF_PAGE_DOWN => Some(Command::ScrollHalfPageDown),
        CENTER_CURSOR_LINE => Some(Command::CenterCursorLine),
        OPEN_IN_FILE_SEARCH => Some(Command::OpenInFileSearch),
        SEARCH_NEXT => Some(Command::SearchNext),
        SEARCH_PREV => Some(Command::SearchPrev),
        SEARCH_WORD_UNDER_CURSOR => Some(Command::SearchWordUnderCursor),
        CLEAR_SEARCH_HIGHLIGHTS => Some(Command::ClearSearchHighlights),
        BACKSPACE => Some(Command::Backspace),
        NEWLINE => Some(Command::Newline),
        INSERT_TAB => Some(Command::InsertTab),
        INSERT_LINE_BELOW => Some(Command::InsertLineBelow),
        INSERT_LINE_ABOVE => Some(Command::InsertLineAbove),
        INSERT_AT_LINE_START => Some(Command::InsertAtLineStart),
        APPEND_AT_LINE_END => Some(Command::AppendAtLineEnd),
        APPEND_AFTER_CURSOR => Some(Command::AppendAfterCursor),
        SUBSTITUTE_LINE => Some(Command::SubstituteLine),
        DELETE_CHAR => Some(Command::DeleteChar),
        DELETE_SELECTION => Some(Command::DeleteSelection),
        DELETE_CURRENT_LINE => Some(Command::DeleteCurrentLine),
        DELETE_TO_LINE_END => Some(Command::DeleteToLineEnd),
        TOGGLE_LINE_COMMENT => Some(Command::ToggleLineComment),
        TOGGLE_SELECTION_COMMENT => Some(Command::ToggleSelectionComment),
        WRAP_SELECTION_WITH_STAR => Some(Command::WrapSelectionWithStar),
        DELETE_WORD_FORWARD => Some(Command::DeleteWordForward),
        DELETE_WORD_BACKWARD => Some(Command::DeleteWordBackward),
        YANK_SELECTION => Some(Command::YankSelection),
        CHANGE_SELECTION => Some(Command::ChangeSelection),
        CHANGE_WORD_FORWARD => Some(Command::ChangeWordForward),
        CHANGE_WORD_BACKWARD => Some(Command::ChangeWordBackward),
        CHANGE_TO_LINE_END => Some(Command::ChangeToLineEnd),
        YANK_TO_LINE_END => Some(Command::Operate {
            op: crate::core::commands::Operator::Yank,
            target: crate::core::commands::OperationTarget::Motion(
                crate::core::commands::Motion::LineEnd,
            ),
        }),
        JOIN_LINES => Some(Command::JoinLines),
        PASTE_AFTER => Some(Command::PasteAfter),
        PASTE_BEFORE => Some(Command::PasteBefore),
        EDITOR_PASTE | PASTE_SYSTEM_CLIPBOARD => Some(Command::EditorPaste),
        VISUAL_PASTE => Some(Command::VisualPaste),
        UNDO => Some(Command::Undo),
        REDO => Some(Command::Redo),
        SAVE_FILE => Some(Command::SaveFile),
        OPEN_FILE => Some(Command::OpenFile(
            open_file_path.map(PathBuf::from).unwrap_or_default(),
        )),
        OPEN_FOLDER => Some(Command::OpenFolder),
        OPEN_RECENT_PROJECTS => Some(Command::OpenRecentProjects),
        ENTER_NORMAL => Some(Command::SwitchMode(ModeEvent::EnterNormal)),
        ENTER_INSERT => Some(Command::SwitchMode(ModeEvent::EnterInsert)),
        ENTER_VISUAL => Some(Command::SwitchMode(ModeEvent::EnterVisual)),
        ENTER_VISUAL_LINE => Some(Command::EnterVisualLine),
        ENTER_VISUAL_BLOCK => Some(Command::SwitchMode(ModeEvent::EnterVisualBlock)),
        ENTER_RESIZE => Some(Command::SwitchMode(ModeEvent::EnterResize)),
        ENTER_TERMINAL_FOCUS => Some(Command::SwitchMode(ModeEvent::FocusTerminal)),
        EXIT_FOCUS => Some(Command::SwitchMode(ModeEvent::ExitFocus)),
        ESCAPE_MODE => Some(Command::SwitchMode(ModeEvent::Escape)),
        TERMINAL_ENTER_NORMAL_MODE => Some(Command::SwitchMode(ModeEvent::EnterTerminalNormal)),
        TERMINAL_PASTE => Some(Command::TerminalPaste),
        TERMINAL_SEARCH_OPEN => Some(Command::TerminalSearchOpen),
        TERMINAL_TAB_NEW => Some(Command::TerminalTabNew),
        TERMINAL_TAB_CLOSE => Some(Command::TerminalTabClose),
        TERMINAL_TAB_SWITCH_1 => Some(Command::SwitchTerminalTab(0)),
        TERMINAL_TAB_SWITCH_2 => Some(Command::SwitchTerminalTab(1)),
        TERMINAL_TAB_SWITCH_3 => Some(Command::SwitchTerminalTab(2)),
        TERMINAL_TAB_SWITCH_4 => Some(Command::SwitchTerminalTab(3)),
        TERMINAL_TAB_SWITCH_5 => Some(Command::SwitchTerminalTab(4)),
        TERMINAL_TAB_SWITCH_6 => Some(Command::SwitchTerminalTab(5)),
        TERMINAL_TAB_SWITCH_7 => Some(Command::SwitchTerminalTab(6)),
        TERMINAL_TAB_SWITCH_8 => Some(Command::SwitchTerminalTab(7)),
        TERMINAL_TAB_SWITCH_9 => Some(Command::SwitchTerminalTab(8)),
        RIGHT_DOCK_SWITCH_1 => Some(Command::SwitchRightTab(0)),
        RIGHT_DOCK_SWITCH_2 => Some(Command::SwitchRightTab(1)),
        RIGHT_DOCK_SWITCH_3 => Some(Command::SwitchRightTab(2)),
        RIGHT_DOCK_SWITCH_4 => Some(Command::SwitchRightTab(3)),
        RIGHT_DOCK_SWITCH_5 => Some(Command::SwitchRightTab(4)),
        RIGHT_DOCK_SWITCH_6 => Some(Command::SwitchRightTab(5)),
        RIGHT_DOCK_SWITCH_7 => Some(Command::SwitchRightTab(6)),
        RIGHT_DOCK_SWITCH_8 => Some(Command::SwitchRightTab(7)),
        RIGHT_DOCK_SWITCH_9 => Some(Command::SwitchRightTab(8)),
        BUFFER_GOTO_1 => Some(Command::BufferGoto(0)),
        BUFFER_GOTO_2 => Some(Command::BufferGoto(1)),
        BUFFER_GOTO_3 => Some(Command::BufferGoto(2)),
        BUFFER_GOTO_4 => Some(Command::BufferGoto(3)),
        BUFFER_GOTO_5 => Some(Command::BufferGoto(4)),
        BUFFER_GOTO_6 => Some(Command::BufferGoto(5)),
        BUFFER_GOTO_7 => Some(Command::BufferGoto(6)),
        BUFFER_GOTO_8 => Some(Command::BufferGoto(7)),
        BUFFER_GOTO_9 => Some(Command::BufferGoto(8)),
        TOGGLE_TERMINAL => Some(Command::ToggleTerminal),
        TOGGLE_BOTTOM_DOCK => Some(Command::ToggleBottomDock),
        RUN_TEST_CASES => Some(Command::RunTestCases),
        NEW_LEETCODE_FILE => Some(Command::NewLeetCodeFile),
        FETCH_LEETCODE_PROBLEM => Some(Command::FetchLeetCodeProblem),
        TOGGLE_LEFT_DOCK => Some(Command::ToggleLeftDock),
        OPEN_FILE_PICKER => Some(Command::OpenFilePicker),
        OPEN_FILE_FINDER => Some(Command::OpenFileFinder),
        OPEN_COMMAND_PALETTE => Some(Command::OpenCommandPalette),
        OPEN_VIM_COMMAND => Some(Command::OpenVimCommand),
        OPEN_WORKSPACE_SYMBOLS => Some(Command::OpenWorkspaceSymbols),
        OPEN_DOCUMENT_SYMBOLS => Some(Command::OpenDocumentSymbols),
        SEARCH_IN_FILES => Some(Command::SearchInFiles),
        OPEN_THEME_SELECTOR => Some(Command::OpenThemeSelector),
        OPEN_FILE_HISTORY => Some(Command::OpenFileHistory),
        OPEN_SETTINGS => Some(Command::OpenSettings),
        OPEN_HELP => Some(Command::OpenHelp),
        OPEN_EXTENSIONS_MANAGER => Some(Command::OpenExtensionsManager),
        GIT_OPEN_LAZYGIT => Some(Command::GitOpenLazygit),
        DOCKER_OPEN_LAZYDOCKER => Some(Command::GitOpenLazydocker),
        GIT_BLAME_LINE => Some(Command::GitBlameLine),
        LSP_HOVER => Some(Command::LspHover),
        LSP_GO_TO_DEFINITION => Some(Command::LspGoToDefinition),
        LSP_PREVIEW_DEFINITION => Some(Command::LspPreviewDefinition),
        LSP_REFERENCES => Some(Command::LspReferences),
        LSP_RENAME => Some(Command::LspRename),
        LSP_FORMAT_DOCUMENT => Some(Command::LspFormatDocument),
        LSP_TRIGGER_COMPLETION => Some(Command::TriggerCompletion),
        LSP_CODE_ACTION => Some(Command::CodeAction),
        LSP_SELECT_PYTHON_ENV => Some(Command::LspSelectPythonEnv),
        LSP_SELECT_DART_ENV => Some(Command::LspSelectDartEnv),
        RELOAD_WORKSPACE => Some(Command::ReloadWorkspace),
        RESIZE_DECREASE_WIDTH => Some(Command::ResizeDecreaseWidth),
        RESIZE_INCREASE_WIDTH => Some(Command::ResizeIncreaseWidth),
        RESIZE_DECREASE_HEIGHT => Some(Command::ResizeDecreaseHeight),
        RESIZE_INCREASE_HEIGHT => Some(Command::ResizeIncreaseHeight),
        RESIZE_GROW_LEFT_DOCK => Some(Command::ResizeGrowLeftDock),
        RESIZE_GROW_RIGHT_DOCK => Some(Command::ResizeGrowRightDock),
        COMPLETION_NEXT => Some(Command::CompletionNext),
        COMPLETION_PREV => Some(Command::CompletionPrev),
        COMPLETION_ACCEPT => Some(Command::CompletionAccept),
        COMPLETION_CLOSE => Some(Command::CompletionClose),
        AI_ACCEPT_INLINE => Some(Command::AiAcceptInline),
        AI_ACCEPT_INLINE_WORD => Some(Command::AiAcceptInlineWord),
        AI_CHAT_TOGGLE => Some(Command::AiChatToggle),
        AI_CHAT_SEND => Some(Command::AiChatSend),
        AI_CHAT_STOP => Some(Command::AiChatStop),
        AI_CHAT_CLOSE => Some(Command::AiChatClose),
        AI_CHAT_UNFOCUS => Some(Command::AiChatUnfocus),
        AI_CHAT_FOCUS => Some(Command::AiChatFocus),
        AI_CHAT_ADD_SELECTION_CONTEXT => Some(Command::AiChatAddSelectionContext),
        DIAGNOSTICS_OPEN_PICKER => Some(Command::DiagnosticsOpenPicker),
        JUMP_BACK => Some(Command::JumpBack),
        JUMP_FORWARD => Some(Command::JumpForward),
        FOCUS_EDITOR => Some(Command::FocusEditor),
        FOCUS_EXPLORER => Some(Command::FocusExplorer),
        FOCUS_TERMINAL => Some(Command::FocusTerminal),
        FOCUS_INSPECTOR => Some(Command::FocusInspector),
        FOCUS_LEFT => Some(Command::FocusLeft),
        FOCUS_RIGHT => Some(Command::FocusRight),
        FOCUS_UP => Some(Command::FocusUp),
        FOCUS_DOWN => Some(Command::FocusDown),
        MOVE_FOCUS_CYCLE => Some(Command::MoveFocusCycle),
        FOCUS_BACK => Some(Command::FocusBack),
        NEXT_PANEL_TAB => Some(Command::NextPanelTab),
        PREV_PANEL_TAB => Some(Command::PrevPanelTab),
        NEW_INSTANCE => Some(Command::NewInstance),
        BUFFER_NEW => Some(Command::BufferNew),
        BUFFER_NEXT => Some(Command::BufferNext),
        BUFFER_PREV => Some(Command::BufferPrev),
        BUFFER_CLOSE_CURRENT => Some(Command::BufferCloseCurrent),
        EXPLORER_MOVE_UP => Some(Command::ExplorerMoveUp),
        EXPLORER_MOVE_DOWN => Some(Command::ExplorerMoveDown),
        EXPLORER_COLLAPSE_NODE | EXPLORER_COLLAPSE_OR_PARENT => {
            Some(Command::ExplorerCollapseOrParent)
        }
        EXPLORER_EXPAND_NODE => Some(Command::ExplorerExpandNode),
        EXPLORER_COLLAPSE_ALL_UNDER_NODE => Some(Command::ExplorerCollapseAllUnderNode),
        EXPLORER_EXPAND_OR_CHILD => Some(Command::ExplorerExpandOrChild),
        EXPLORER_EXPAND_ALL_UNDER_NODE => Some(Command::ExplorerExpandAllUnderNode),
        EXPLORER_TOGGLE_OR_OPEN => Some(Command::ExplorerToggleOrOpen),
        EXPLORER_DELETE_NODE => Some(Command::ExplorerDeleteNode),
        EXPLORER_CREATE_FILE => Some(Command::ExplorerCreateFile),
        EXPLORER_CREATE_FOLDER => Some(Command::ExplorerCreateFolder),
        EXPLORER_START_FILTER => Some(Command::ExplorerStartFilter),
        EXPLORER_CLEAR_FILTER => Some(Command::ExplorerClearFilter),
        EXPLORER_TOGGLE_HIDDEN => Some(Command::ExplorerToggleHidden),
        EXPLORER_TOGGLE_IGNORED => Some(Command::ExplorerToggleIgnored),
        EXPLORER_TOGGLE_GIT_CHANGES_ONLY => Some(Command::ExplorerToggleGitChangesOnly),
        EXPLORER_MOVE_TO_TOP => Some(Command::ExplorerMoveToTop),
        EXPLORER_MOVE_TO_BOTTOM => Some(Command::ExplorerMoveToBottom),
        EXPLORER_RENAME_FULL => Some(Command::ExplorerRenameFull),
        EXPLORER_RENAME_BASE => Some(Command::ExplorerRenameBase),
        EXPLORER_COPY_FILE => Some(Command::ExplorerCopyFile),
        EXPLORER_PASTE_FILE => Some(Command::ExplorerPasteFile),
        EXPLORER_EXPAND_COLLAPSE => Some(Command::ExplorerExpandCollapse),
        EXPLORER_OPEN_FILE => Some(Command::ExplorerOpenFile),
        OVERLAY_SELECT_NEXT | FILE_PICKER_SELECT_NEXT => Some(Command::OverlaySelectNext),
        OVERLAY_SELECT_PREV | FILE_PICKER_SELECT_PREV => Some(Command::OverlaySelectPrev),
        TOGGLE_LIVE_GREP_CASE_SENSITIVE => Some(Command::ToggleLiveGrepCaseSensitive),
        TOGGLE_IN_FILE_SEARCH_CASE_SENSITIVE => Some(Command::ToggleInFileSearchCaseSensitive),
        SETTINGS_SELECT_NEXT => Some(Command::SettingsSelectNext),
        SETTINGS_SELECT_PREV => Some(Command::SettingsSelectPrev),
        SETTINGS_ADJUST_DECREASE => Some(Command::SettingsAdjustDecrease),
        SETTINGS_ADJUST_INCREASE => Some(Command::SettingsAdjustIncrease),
        SETTINGS_ACTIVATE => Some(Command::SettingsActivate),
        FILE_PICKER_CONFIRM => Some(Command::FilePickerConfirmSelection),
        FILE_PICKER_CLOSE => Some(Command::CloseFilePicker),
        FILE_PICKER_BACKSPACE => Some(Command::FilePickerBackspaceQuery),
        LEAP_START => Some(Command::LeapStart),
        MULTI_CURSOR_ADD_NEXT => Some(Command::MultiCursorAddNext),
        MULTI_CURSOR_SKIP => Some(Command::MultiCursorSkip),
        MULTI_CURSOR_INSERT_BEFORE => Some(Command::MultiCursorInsertBefore),
        MULTI_CURSOR_APPEND_AFTER => Some(Command::MultiCursorAppendAfter),
        MULTI_CURSOR_CHANGE => Some(Command::MultiCursorChange),
        MULTI_CURSOR_DELETE => Some(Command::MultiCursorDelete),
        MULTI_CURSOR_SELECT_ALL => Some(Command::MultiCursorSelectAll),
        TOGGLE_MARKDOWN_PREVIEW => Some(Command::ToggleMarkdownPreview),
        CLOSE_SIDEBARS => Some(Command::CloseSidebars),
        FOCUS_MARKDOWN_PREVIEW => Some(Command::FocusMarkdownPreview),
        MARKDOWN_PREVIEW_SCROLL_UP => Some(Command::MarkdownPreviewScrollUp),
        MARKDOWN_PREVIEW_SCROLL_DOWN => Some(Command::MarkdownPreviewScrollDown),
        MARKDOWN_PREVIEW_SCROLL_HALF_PAGE_UP => Some(Command::MarkdownPreviewScrollHalfPageUp),
        MARKDOWN_PREVIEW_SCROLL_HALF_PAGE_DOWN => Some(Command::MarkdownPreviewScrollHalfPageDown),
        MARKDOWN_PREVIEW_SCROLL_TOP => Some(Command::MarkdownPreviewScrollTop),
        MARKDOWN_PREVIEW_SCROLL_BOTTOM => Some(Command::MarkdownPreviewScrollBottom),
        HELP_SCROLL_DOWN => Some(Command::HelpScrollDown),
        HELP_SCROLL_UP => Some(Command::HelpScrollUp),
        HELP_SCROLL_HALF_PAGE_DOWN => Some(Command::HelpScrollHalfPageDown),
        HELP_SCROLL_HALF_PAGE_UP => Some(Command::HelpScrollHalfPageUp),
        TOGGLE_MAXIMIZE_FOCUS => Some(Command::ToggleMaximizeFocus),
        TOGGLE_FOLD => Some(Command::ToggleFold),
        TOGGLE_FOLD_ALL => Some(Command::ToggleFoldAll),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_leetcode_file_id_round_trips() {
        assert_eq!(NEW_LEETCODE_FILE, "runner.new_leetcode_file");
        assert!(ALL_IDS.contains(&NEW_LEETCODE_FILE));
        assert!(matches!(
            parse(NEW_LEETCODE_FILE, None),
            Some(Command::NewLeetCodeFile)
        ));
    }

    #[test]
    fn fetch_leetcode_problem_id_round_trips() {
        assert_eq!(FETCH_LEETCODE_PROBLEM, "runner.fetch_leetcode_problem");
        assert!(ALL_IDS.contains(&FETCH_LEETCODE_PROBLEM));
        assert!(matches!(
            parse(FETCH_LEETCODE_PROBLEM, None),
            Some(Command::FetchLeetCodeProblem)
        ));
    }
}
