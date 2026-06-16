use std::{path::PathBuf, time::Duration};

use winit::keyboard::{KeyCode, ModifiersState, NamedKey};

use crate::{
    app::{
        input_map::{InputFocusContext, InputMap, KeybindingContext},
        resolved_keymap::{build, builtin_defaults},
    },
    config::keymap_loader::KeymapLoader,
    core::{
        commands::{Command, Motion, OperationTarget, Operator},
        mode::{EditorMode, ModeEvent},
    },
};

use super::{InputHandler, InputRouteOutcome, NormalizedInput};

#[test]
fn debug_label_includes_key_and_modifiers() {
    let input = NormalizedInput {
        physical_key: Some(winit::keyboard::KeyCode::KeyA),
        named_key: None,
        text: Some("a".to_string()),
        modifiers: ModifiersState::CONTROL | ModifiersState::SHIFT,
    };

    assert_eq!(input.debug_label(), "KeyA + Ctrl+Shift");
}

fn char_input(ch: char, key: KeyCode) -> NormalizedInput {
    NormalizedInput {
        physical_key: Some(key),
        named_key: None,
        text: Some(ch.to_string()),
        modifiers: ModifiersState::empty(),
    }
}

fn ctrl_input(ch: char, key: KeyCode) -> NormalizedInput {
    NormalizedInput {
        physical_key: Some(key),
        named_key: None,
        text: Some(ch.to_string()),
        modifiers: ModifiersState::CONTROL,
    }
}

fn cmd_input(ch: char, key: KeyCode) -> NormalizedInput {
    NormalizedInput {
        physical_key: Some(key),
        named_key: None,
        text: Some(ch.to_string()),
        modifiers: ModifiersState::SUPER,
    }
}

fn named_input(named: NamedKey, physical: Option<KeyCode>) -> NormalizedInput {
    NormalizedInput {
        physical_key: physical,
        named_key: Some(named),
        text: None,
        modifiers: ModifiersState::empty(),
    }
}

fn shift_named_input(named: NamedKey, physical: Option<KeyCode>) -> NormalizedInput {
    NormalizedInput {
        physical_key: physical,
        named_key: Some(named),
        text: None,
        modifiers: ModifiersState::SHIFT,
    }
}

fn completion_context() -> KeybindingContext {
    let mut context = KeybindingContext::for_mode(EditorMode::Insert);
    context.completion_visible = true;
    context
}

fn make_map() -> InputMap {
    InputMap::with_keymap(PathBuf::from("phase7_test.txt"), builtin_defaults())
}

fn make_default_profile_map() -> InputMap {
    let bindings = KeymapLoader::load("default", None);
    InputMap::with_keymap(PathBuf::from("phase7_test.txt"), build(&bindings))
}

#[test]
fn test_runner_field_editing_is_click_only() {
    // Field editing moved to mouse clicks; `i`/Enter no longer open the editor.
    let mut handler = InputHandler::new();
    let map = make_map();
    let mut context =
        KeybindingContext::with_focus(EditorMode::Normal, InputFocusContext::TestRunner);
    context.test_runner_editing = false;
    let now = std::time::Instant::now();

    let edit_enter = handler.route_normalized_input(
        named_input(NamedKey::Enter, Some(KeyCode::Enter)),
        &map,
        context,
        now,
    );
    assert!(edit_enter.is_none(), "Enter must not edit the field anymore");

    let edit_i = handler.route_normalized_input(char_input('i', KeyCode::KeyI), &map, context, now);
    assert!(edit_i.is_none(), "`i` must not edit the field anymore");
}

#[test]
fn test_runner_routes_generate_command() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let mut context =
        KeybindingContext::with_focus(EditorMode::Normal, InputFocusContext::TestRunner);
    context.test_runner_editing = false;
    let now = std::time::Instant::now();

    let generate =
        handler.route_normalized_input(char_input('g', KeyCode::KeyG), &map, context, now);
    assert!(matches!(
        generate,
        Some(InputRouteOutcome::Dispatch(ref translated))
            if translated.command == Command::TestRunnerGenerateCases
    ));
}

#[test]
fn leader_space_f_f_maps_to_open_file_picker() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context = KeybindingContext::for_mode(EditorMode::Normal);
    let t0 = std::time::Instant::now();

    let start = handler.route_normalized_input(
        named_input(NamedKey::Space, Some(KeyCode::Space)),
        &map,
        context,
        t0,
    );
    assert!(matches!(start, Some(InputRouteOutcome::NoDispatch { .. })));

    let follow = handler.route_normalized_input(char_input('f', KeyCode::KeyF), &map, context, t0);
    assert!(matches!(follow, Some(InputRouteOutcome::NoDispatch { .. })));

    let resolved =
        handler.route_normalized_input(char_input('f', KeyCode::KeyF), &map, context, t0);
    match resolved {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(translated.command, Command::OpenFilePicker);
        }
        other => panic!("expected dispatch, got {:?}", other),
    }
}

#[test]
fn leader_space_f_w_maps_to_search_in_files() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context = KeybindingContext::for_mode(EditorMode::Normal);
    let t0 = std::time::Instant::now();

    let start = handler.route_normalized_input(
        named_input(NamedKey::Space, Some(KeyCode::Space)),
        &map,
        context,
        t0,
    );
    assert!(matches!(start, Some(InputRouteOutcome::NoDispatch { .. })));

    let follow = handler.route_normalized_input(char_input('f', KeyCode::KeyF), &map, context, t0);
    assert!(matches!(follow, Some(InputRouteOutcome::NoDispatch { .. })));

    let resolved =
        handler.route_normalized_input(char_input('w', KeyCode::KeyW), &map, context, t0);
    match resolved {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(translated.command, Command::SearchInFiles);
        }
        other => panic!("expected dispatch, got {:?}", other),
    }
}

#[test]
fn leader_space_f_m_maps_to_format_document() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context = KeybindingContext::for_mode(EditorMode::Normal);
    let t0 = std::time::Instant::now();

    let start = handler.route_normalized_input(
        named_input(NamedKey::Space, Some(KeyCode::Space)),
        &map,
        context,
        t0,
    );
    assert!(matches!(start, Some(InputRouteOutcome::NoDispatch { .. })));

    let follow = handler.route_normalized_input(char_input('f', KeyCode::KeyF), &map, context, t0);
    assert!(matches!(follow, Some(InputRouteOutcome::NoDispatch { .. })));

    let resolved =
        handler.route_normalized_input(char_input('m', KeyCode::KeyM), &map, context, t0);
    match resolved {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(translated.command, Command::LspFormatDocument);
        }
        other => panic!("expected dispatch, got {:?}", other),
    }
}

#[test]
fn fuzzy_picker_normal_mode_allows_leader_close_sequence() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context = KeybindingContext::with_focus(EditorMode::Normal, InputFocusContext::FuzzyPicker);
    let t0 = std::time::Instant::now();

    let start = handler.route_normalized_input(
        named_input(NamedKey::Space, Some(KeyCode::Space)),
        &map,
        context,
        t0,
    );
    assert!(matches!(start, Some(InputRouteOutcome::NoDispatch { .. })));

    let resolved =
        handler.route_normalized_input(char_input('x', KeyCode::KeyX), &map, context, t0);
    match resolved {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(translated.command, Command::BufferCloseCurrent);
        }
        other => panic!("expected dispatch, got {:?}", other),
    }
}

#[test]
fn zen_mode_active_allows_leader_z_m_from_ai_chat_focus() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let mut context = KeybindingContext::with_focus(EditorMode::Normal, InputFocusContext::AiChat);
    context.zen_mode_active = true;
    let t0 = std::time::Instant::now();

    let start = handler.route_normalized_input(
        named_input(NamedKey::Space, Some(KeyCode::Space)),
        &map,
        context,
        t0,
    );
    assert!(matches!(start, Some(InputRouteOutcome::NoDispatch { .. })));

    let follow = handler.route_normalized_input(char_input('z', KeyCode::KeyZ), &map, context, t0);
    assert!(matches!(follow, Some(InputRouteOutcome::NoDispatch { .. })));

    let resolved =
        handler.route_normalized_input(char_input('m', KeyCode::KeyM), &map, context, t0);
    match resolved {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(translated.command, Command::ToggleMaximizeFocus);
        }
        other => panic!("expected Zen toggle dispatch, got {:?}", other),
    }
}

#[test]
fn zen_mode_active_blocks_other_leader_commands_from_ai_chat_focus() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let mut context = KeybindingContext::with_focus(EditorMode::Normal, InputFocusContext::AiChat);
    context.zen_mode_active = true;
    let t0 = std::time::Instant::now();

    let start = handler.route_normalized_input(
        named_input(NamedKey::Space, Some(KeyCode::Space)),
        &map,
        context,
        t0,
    );
    assert!(matches!(start, Some(InputRouteOutcome::NoDispatch { .. })));

    let resolved =
        handler.route_normalized_input(char_input('f', KeyCode::KeyF), &map, context, t0);
    assert!(matches!(
        resolved,
        Some(InputRouteOutcome::NoDispatch { .. }) | None
    ));
}

#[test]
fn zen_mode_active_routes_space_to_terminal_focus_input() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let mut context =
        KeybindingContext::with_focus(EditorMode::TerminalFocus, InputFocusContext::Terminal);
    context.zen_mode_active = true;
    let t0 = std::time::Instant::now();

    let start = handler.route_normalized_input(
        named_input(NamedKey::Space, Some(KeyCode::Space)),
        &map,
        context,
        t0,
    );
    match start {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(
                translated.command,
                Command::TerminalWriteInput(" ".to_string())
            );
        }
        other => panic!("expected terminal space dispatch, got {:?}", other),
    }
}

#[test]
fn zen_mode_active_forwards_escape_to_terminal_pty() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let mut context =
        KeybindingContext::with_focus(EditorMode::TerminalFocus, InputFocusContext::Terminal);
    context.zen_mode_active = true;
    let t0 = std::time::Instant::now();

    let resolved = handler.route_normalized_input(
        named_input(NamedKey::Escape, Some(KeyCode::Escape)),
        &map,
        context,
        t0,
    );
    match resolved {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(
                translated.command,
                Command::TerminalWriteInput("\u{1b}".to_string())
            );
        }
        other => panic!("expected raw Esc dispatch in zen terminal, got {:?}", other),
    }
}

#[test]
fn non_zen_terminal_escape_does_not_forward_raw_pty() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context =
        KeybindingContext::with_focus(EditorMode::TerminalFocus, InputFocusContext::Terminal);
    let t0 = std::time::Instant::now();

    let resolved = handler.route_normalized_input(
        named_input(NamedKey::Escape, Some(KeyCode::Escape)),
        &map,
        context,
        t0,
    );
    // Outside Zen Mode, Esc keeps its focus-leaving semantics (FocusBack), never a
    // raw ESC into the PTY.
    if let Some(InputRouteOutcome::Dispatch(translated)) = resolved {
        assert_ne!(
            translated.command,
            Command::TerminalWriteInput("\u{1b}".to_string())
        );
    }
}

#[test]
fn right_sidebar_terminal_ctrl_u_d_scroll_half_page() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let mut context =
        KeybindingContext::with_focus(EditorMode::TerminalFocus, InputFocusContext::Terminal);
    context.right_sidebar_terminal = true;
    let t0 = std::time::Instant::now();

    // Forwarded as opencode's default half-page keybinds (ctrl+alt+u / ctrl+alt+d)
    // because the chat history lives inside opencode's own viewport, not in our
    // grid scrollback.
    let up = handler.route_normalized_input(ctrl_input('u', KeyCode::KeyU), &map, context, t0);
    match up {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(
                translated.command,
                Command::TerminalWriteInput("\u{1b}\u{15}".to_string())
            );
        }
        other => panic!("expected scroll up dispatch, got {:?}", other),
    }

    let down = handler.route_normalized_input(ctrl_input('d', KeyCode::KeyD), &map, context, t0);
    match down {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(
                translated.command,
                Command::TerminalWriteInput("\u{1b}\u{4}".to_string())
            );
        }
        other => panic!("expected scroll down dispatch, got {:?}", other),
    }
}

#[test]
fn right_sidebar_terminal_ctrl_u_d_scroll_even_when_mode_drifted() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let mut context =
        KeybindingContext::with_focus(EditorMode::Normal, InputFocusContext::Inspector);
    context.right_sidebar_terminal = true;
    let t0 = std::time::Instant::now();

    let up = handler.route_normalized_input(ctrl_input('u', KeyCode::KeyU), &map, context, t0);
    match up {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(
                translated.command,
                Command::TerminalWriteInput("\u{1b}\u{15}".to_string())
            );
        }
        other => panic!("expected right-sidebar scroll up dispatch, got {:?}", other),
    }
}

#[test]
fn bottom_terminal_ctrl_u_d_still_forward_raw_to_pty() {
    let mut handler = InputHandler::new();
    let map = make_map();
    // right_sidebar_terminal defaults to false -> bottom-panel shell keeps ^U/^D.
    let context =
        KeybindingContext::with_focus(EditorMode::TerminalFocus, InputFocusContext::Terminal);
    let t0 = std::time::Instant::now();

    let up = handler.route_normalized_input(ctrl_input('u', KeyCode::KeyU), &map, context, t0);
    match up {
        Some(InputRouteOutcome::Dispatch(translated)) => match &translated.command {
            Command::TerminalWriteInput(payload) => {
                assert!(
                    !payload.starts_with('\u{1b}'),
                    "bottom terminal Ctrl+U must stay raw ^U, not the right-dock \
                         ctrl+alt+u forward, got {:?}",
                    payload
                );
            }
            other => panic!("bottom terminal Ctrl+U should forward raw, got {:?}", other),
        },
        other => panic!("expected raw write dispatch, got {:?}", other),
    }
}

#[test]
fn zen_mode_active_allows_leader_z_m_from_terminal_normal_mode() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let mut context =
        KeybindingContext::with_focus(EditorMode::TerminalNormal, InputFocusContext::Terminal);
    context.zen_mode_active = true;
    let t0 = std::time::Instant::now();

    let start = handler.route_normalized_input(
        named_input(NamedKey::Space, Some(KeyCode::Space)),
        &map,
        context,
        t0,
    );
    assert!(matches!(start, Some(InputRouteOutcome::NoDispatch { .. })));

    let follow = handler.route_normalized_input(char_input('z', KeyCode::KeyZ), &map, context, t0);
    assert!(matches!(follow, Some(InputRouteOutcome::NoDispatch { .. })));

    let resolved =
        handler.route_normalized_input(char_input('m', KeyCode::KeyM), &map, context, t0);
    match resolved {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(translated.command, Command::ToggleMaximizeFocus);
        }
        other => panic!("expected Zen toggle dispatch, got {:?}", other),
    }
}

#[test]
fn zen_mode_active_allows_leader_z_m_from_bottom_panel_normal_mode() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let mut context =
        KeybindingContext::with_focus(EditorMode::Normal, InputFocusContext::BottomPanel);
    context.zen_mode_active = true;
    let t0 = std::time::Instant::now();

    let start = handler.route_normalized_input(
        named_input(NamedKey::Space, Some(KeyCode::Space)),
        &map,
        context,
        t0,
    );
    assert!(matches!(start, Some(InputRouteOutcome::NoDispatch { .. })));

    let follow = handler.route_normalized_input(char_input('z', KeyCode::KeyZ), &map, context, t0);
    assert!(matches!(follow, Some(InputRouteOutcome::NoDispatch { .. })));

    let resolved =
        handler.route_normalized_input(char_input('m', KeyCode::KeyM), &map, context, t0);
    match resolved {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(translated.command, Command::ToggleMaximizeFocus);
        }
        other => panic!("expected Zen toggle dispatch, got {:?}", other),
    }
}

#[test]
fn zen_mode_markdown_preview_still_allows_gg_scroll_top() {
    let mut handler = InputHandler::new();
    let map = make_default_profile_map();
    let mut context =
        KeybindingContext::with_focus(EditorMode::Normal, InputFocusContext::MarkdownPreview);
    context.zen_mode_active = true;
    let t0 = std::time::Instant::now();

    let start = handler.route_normalized_input(char_input('g', KeyCode::KeyG), &map, context, t0);
    assert!(matches!(start, Some(InputRouteOutcome::NoDispatch { .. })));

    let resolved =
        handler.route_normalized_input(char_input('g', KeyCode::KeyG), &map, context, t0);
    match resolved {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(translated.command, Command::MarkdownPreviewScrollTop);
        }
        other => panic!("expected markdown preview gg dispatch, got {:?}", other),
    }
}

#[test]
fn markdown_preview_q_closes_current_buffer() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context =
        KeybindingContext::with_focus(EditorMode::Normal, InputFocusContext::MarkdownPreview);
    let t0 = std::time::Instant::now();

    let resolved =
        handler.route_normalized_input(char_input('q', KeyCode::KeyQ), &map, context, t0);
    match resolved {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(translated.command, Command::BufferCloseCurrent);
        }
        other => panic!(
            "expected markdown preview q close dispatch, got {:?}",
            other
        ),
    }
}

#[test]
fn markdown_preview_leader_x_closes_current_buffer() {
    let mut handler = InputHandler::new();
    let map = make_default_profile_map();
    let context =
        KeybindingContext::with_focus(EditorMode::Normal, InputFocusContext::MarkdownPreview);
    let t0 = std::time::Instant::now();

    let start = handler.route_normalized_input(
        named_input(NamedKey::Space, Some(KeyCode::Space)),
        &map,
        context,
        t0,
    );
    assert!(matches!(start, Some(InputRouteOutcome::NoDispatch { .. })));

    let resolved =
        handler.route_normalized_input(char_input('x', KeyCode::KeyX), &map, context, t0);
    match resolved {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(translated.command, Command::BufferCloseCurrent);
        }
        other => panic!(
            "expected markdown preview leader x close dispatch, got {:?}",
            other
        ),
    }
}

#[test]
fn d_d_maps_to_delete_current_line() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context = KeybindingContext::for_mode(EditorMode::Normal);
    let t0 = std::time::Instant::now();

    let first = handler.route_normalized_input(char_input('d', KeyCode::KeyD), &map, context, t0);
    assert!(matches!(first, Some(InputRouteOutcome::NoDispatch { .. })));

    let second = handler.route_normalized_input(char_input('d', KeyCode::KeyD), &map, context, t0);
    match second {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(
                translated.command,
                Command::Operate {
                    op: Operator::Delete,
                    target: OperationTarget::CurrentLine,
                }
            );
        }
        other => panic!("expected dd dispatch, got {:?}", other),
    }
}

#[test]
fn g_c_c_maps_to_toggle_line_comment() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context = KeybindingContext::for_mode(EditorMode::Normal);
    let t0 = std::time::Instant::now();

    let first = handler.route_normalized_input(char_input('g', KeyCode::KeyG), &map, context, t0);
    assert!(matches!(first, Some(InputRouteOutcome::NoDispatch { .. })));

    let second = handler.route_normalized_input(char_input('c', KeyCode::KeyC), &map, context, t0);
    assert!(matches!(second, Some(InputRouteOutcome::NoDispatch { .. })));

    let third = handler.route_normalized_input(char_input('c', KeyCode::KeyC), &map, context, t0);
    match third {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(translated.command, Command::ToggleLineComment);
        }
        other => panic!("expected gcc dispatch, got {:?}", other),
    }
}

#[test]
fn y_y_maps_to_yank_current_line() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context = KeybindingContext::for_mode(EditorMode::Normal);
    let t0 = std::time::Instant::now();

    let first = handler.route_normalized_input(char_input('y', KeyCode::KeyY), &map, context, t0);
    assert!(matches!(first, Some(InputRouteOutcome::NoDispatch { .. })));

    let second = handler.route_normalized_input(char_input('y', KeyCode::KeyY), &map, context, t0);
    match second {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(
                translated.command,
                Command::Operate {
                    op: Operator::Yank,
                    target: OperationTarget::CurrentLine,
                }
            );
        }
        other => panic!("expected yy dispatch, got {:?}", other),
    }
}

#[test]
fn y_e_maps_to_yank_to_word_end() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context = KeybindingContext::for_mode(EditorMode::Normal);
    let t0 = std::time::Instant::now();

    let first = handler.route_normalized_input(char_input('y', KeyCode::KeyY), &map, context, t0);
    assert!(matches!(first, Some(InputRouteOutcome::NoDispatch { .. })));

    let second = handler.route_normalized_input(char_input('e', KeyCode::KeyE), &map, context, t0);
    match second {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(
                translated.command,
                Command::Operate {
                    op: Operator::Yank,
                    target: OperationTarget::Motion(Motion::WordEnd),
                }
            );
        }
        other => panic!("expected ye dispatch, got {:?}", other),
    }
}

#[test]
fn visual_g_c_maps_to_toggle_selection_comment() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context = KeybindingContext::for_mode(EditorMode::Visual);
    let t0 = std::time::Instant::now();

    let first = handler.route_normalized_input(char_input('g', KeyCode::KeyG), &map, context, t0);
    assert!(matches!(first, Some(InputRouteOutcome::NoDispatch { .. })));

    let second = handler.route_normalized_input(char_input('c', KeyCode::KeyC), &map, context, t0);
    match second {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(translated.command, Command::ToggleSelectionComment);
        }
        other => panic!("expected visual gc dispatch, got {:?}", other),
    }
}

#[test]
fn numeric_count_wraps_simple_motion_dispatch() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context = KeybindingContext::for_mode(EditorMode::Normal);
    let t0 = std::time::Instant::now();

    let count = handler.route_normalized_input(char_input('5', KeyCode::Digit5), &map, context, t0);
    assert!(matches!(count, Some(InputRouteOutcome::NoDispatch { .. })));
    assert_eq!(handler.get_pending_keys(), "5");

    let motion = handler.route_normalized_input(char_input('j', KeyCode::KeyJ), &map, context, t0);
    match motion {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(translated.command, Command::MoveDown);
            assert_eq!(translated.repeat_count, 5);
        }
        other => panic!("expected counted move-down dispatch, got {:?}", other),
    }

    let next = handler.route_normalized_input(char_input('k', KeyCode::KeyK), &map, context, t0);
    match next {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(translated.command, Command::MoveUp);
            assert_eq!(translated.repeat_count, 1);
        }
        other => panic!("expected count reset after dispatch, got {:?}", other),
    }
}

#[test]
fn zero_remains_a_regular_key_in_normal_mode() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context = KeybindingContext::for_mode(EditorMode::Normal);
    let t0 = std::time::Instant::now();

    let mapped =
        handler.route_normalized_input(char_input('0', KeyCode::Digit0), &map, context, t0);
    match mapped {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(translated.command, Command::MoveToLineStart);
            assert_eq!(translated.repeat_count, 1);
        }
        other => panic!(
            "expected 0 to resolve as line-start command, got {:?}",
            other
        ),
    }
}

#[test]
fn d_2_w_uses_motion_count_inside_operator_pending() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context = KeybindingContext::for_mode(EditorMode::Normal);
    let t0 = std::time::Instant::now();

    let first = handler.route_normalized_input(char_input('d', KeyCode::KeyD), &map, context, t0);
    assert!(matches!(first, Some(InputRouteOutcome::NoDispatch { .. })));

    let count = handler.route_normalized_input(char_input('2', KeyCode::Digit2), &map, context, t0);
    assert!(matches!(count, Some(InputRouteOutcome::NoDispatch { .. })));
    assert_eq!(handler.get_pending_keys(), "2");

    let second = handler.route_normalized_input(char_input('w', KeyCode::KeyW), &map, context, t0);
    match second {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(
                translated.command,
                Command::Operate {
                    op: Operator::Delete,
                    target: OperationTarget::Motion(Motion::WordForward),
                }
            );
            assert_eq!(translated.repeat_count, 2);
        }
        other => panic!("expected d2w dispatch, got {:?}", other),
    }
}

#[test]
fn three_d_w_preserves_operator_count() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context = KeybindingContext::for_mode(EditorMode::Normal);
    let t0 = std::time::Instant::now();

    let count = handler.route_normalized_input(char_input('3', KeyCode::Digit3), &map, context, t0);
    assert!(matches!(count, Some(InputRouteOutcome::NoDispatch { .. })));

    let first = handler.route_normalized_input(char_input('d', KeyCode::KeyD), &map, context, t0);
    assert!(matches!(first, Some(InputRouteOutcome::NoDispatch { .. })));
    assert_eq!(handler.get_pending_keys(), "3");

    let second = handler.route_normalized_input(char_input('w', KeyCode::KeyW), &map, context, t0);
    match second {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(
                translated.command,
                Command::Operate {
                    op: Operator::Delete,
                    target: OperationTarget::Motion(Motion::WordForward),
                }
            );
            assert_eq!(translated.repeat_count, 3);
        }
        other => panic!("expected 3dw dispatch, got {:?}", other),
    }
}

#[test]
fn operator_and_motion_counts_multiply() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context = KeybindingContext::for_mode(EditorMode::Normal);
    let t0 = std::time::Instant::now();

    let count = handler.route_normalized_input(char_input('3', KeyCode::Digit3), &map, context, t0);
    assert!(matches!(count, Some(InputRouteOutcome::NoDispatch { .. })));

    let first = handler.route_normalized_input(char_input('d', KeyCode::KeyD), &map, context, t0);
    assert!(matches!(first, Some(InputRouteOutcome::NoDispatch { .. })));

    let motion_count =
        handler.route_normalized_input(char_input('2', KeyCode::Digit2), &map, context, t0);
    assert!(matches!(
        motion_count,
        Some(InputRouteOutcome::NoDispatch { .. })
    ));
    assert_eq!(handler.get_pending_keys(), "3 2");

    let second = handler.route_normalized_input(char_input('w', KeyCode::KeyW), &map, context, t0);
    match second {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(
                translated.command,
                Command::Operate {
                    op: Operator::Delete,
                    target: OperationTarget::Motion(Motion::WordForward),
                }
            );
            assert_eq!(translated.repeat_count, 6);
        }
        other => panic!("expected 3d2w dispatch, got {:?}", other),
    }
}

#[test]
fn c_w_maps_to_change_word_forward() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context = KeybindingContext::for_mode(EditorMode::Normal);
    let t0 = std::time::Instant::now();

    let first = handler.route_normalized_input(char_input('c', KeyCode::KeyC), &map, context, t0);
    assert!(matches!(first, Some(InputRouteOutcome::NoDispatch { .. })));

    let second = handler.route_normalized_input(char_input('w', KeyCode::KeyW), &map, context, t0);
    match second {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(
                translated.command,
                Command::Operate {
                    op: Operator::Change,
                    target: OperationTarget::Motion(Motion::WordForward),
                }
            );
        }
        other => panic!("expected cw dispatch, got {:?}", other),
    }
}

#[test]
fn c_b_maps_to_change_word_backward() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context = KeybindingContext::for_mode(EditorMode::Normal);
    let t0 = std::time::Instant::now();

    let first = handler.route_normalized_input(char_input('c', KeyCode::KeyC), &map, context, t0);
    assert!(matches!(first, Some(InputRouteOutcome::NoDispatch { .. })));

    let second = handler.route_normalized_input(char_input('b', KeyCode::KeyB), &map, context, t0);
    match second {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(
                translated.command,
                Command::Operate {
                    op: Operator::Change,
                    target: OperationTarget::Motion(Motion::WordBackward),
                }
            );
        }
        other => panic!("expected cb dispatch, got {:?}", other),
    }
}

#[test]
fn r_then_printable_key_maps_to_replace_char() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context = KeybindingContext::for_mode(EditorMode::Normal);
    let t0 = std::time::Instant::now();

    let first = handler.route_normalized_input(char_input('r', KeyCode::KeyR), &map, context, t0);
    assert!(matches!(first, Some(InputRouteOutcome::NoDispatch { .. })));

    let second = handler.route_normalized_input(char_input('X', KeyCode::KeyX), &map, context, t0);
    match second {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(translated.command, Command::ReplaceChar('X'));
        }
        other => panic!("expected r<char> dispatch, got {:?}", other),
    }
}

#[test]
fn pending_replace_is_canceled_by_escape() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context = KeybindingContext::for_mode(EditorMode::Normal);
    let t0 = std::time::Instant::now();

    let first = handler.route_normalized_input(char_input('r', KeyCode::KeyR), &map, context, t0);
    assert!(matches!(first, Some(InputRouteOutcome::NoDispatch { .. })));

    let canceled =
        handler.route_normalized_input(named_input(NamedKey::Escape, None), &map, context, t0);
    assert!(matches!(
        canceled,
        Some(InputRouteOutcome::NoDispatch { .. })
    ));

    let next = handler.route_normalized_input(char_input('w', KeyCode::KeyW), &map, context, t0);
    match next {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(translated.command, Command::MoveWordForward);
        }
        other => panic!(
            "pending replace state should reset after Escape, got {:?}",
            other
        ),
    }
}

#[test]
fn pending_delete_prefix_is_canceled_by_escape() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context = KeybindingContext::for_mode(EditorMode::Normal);
    let t0 = std::time::Instant::now();

    let first = handler.route_normalized_input(char_input('d', KeyCode::KeyD), &map, context, t0);
    assert!(matches!(first, Some(InputRouteOutcome::NoDispatch { .. })));

    let canceled =
        handler.route_normalized_input(named_input(NamedKey::Escape, None), &map, context, t0);
    assert!(matches!(
        canceled,
        Some(InputRouteOutcome::NoDispatch { .. })
    ));

    let next = handler.route_normalized_input(char_input('w', KeyCode::KeyW), &map, context, t0);
    match next {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(translated.command, Command::MoveWordForward);
        }
        other => panic!("pending state should reset after Escape, got {:?}", other),
    }
}

#[test]
fn interrupted_prefix_falls_back_and_does_not_stick() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context = KeybindingContext::for_mode(EditorMode::Normal);
    let t0 = std::time::Instant::now();

    let _ = handler.route_normalized_input(
        named_input(NamedKey::Space, Some(KeyCode::Space)),
        &map,
        context,
        t0,
    );

    let interrupted =
        handler.route_normalized_input(char_input('h', KeyCode::KeyH), &map, context, t0);
    match interrupted {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(translated.command, Command::MoveLeft);
        }
        other => panic!("expected fallback dispatch for h, got {:?}", other),
    }

    let after = handler.route_normalized_input(char_input('j', KeyCode::KeyJ), &map, context, t0);
    match after {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(translated.command, Command::MoveDown);
        }
        other => panic!("expected normal dispatch after interrupt, got {:?}", other),
    }
}

#[test]
fn pending_chord_survives_timeout_window() {
    // Chord sequences must NOT silently expire: their state is visible via the
    // which-key overlay, so a continuation key pressed after a long pause must
    // still resolve the chord instead of firing a normal-mode command.
    let mut handler = InputHandler::new();
    let map = make_map();
    let context = KeybindingContext::for_mode(EditorMode::Normal);
    let t0 = std::time::Instant::now();

    let _ = handler.route_normalized_input(
        named_input(NamedKey::Space, Some(KeyCode::Space)),
        &map,
        context,
        t0,
    );
    assert!(handler.pending_chord_sequence().is_some());

    // Well past the old 1.5s prefix timeout — the chord must still be alive.
    let after_timeout = t0 + Duration::from_millis(1_700);

    // 'q' has no continuation in the chord: it cancels the chord and falls
    // back to normal resolution ('q' is unbound -> NoDispatch, not None).
    let interrupted = handler.route_normalized_input(
        char_input('q', KeyCode::KeyQ),
        &map,
        context,
        after_timeout,
    );
    assert!(matches!(
        interrupted,
        Some(InputRouteOutcome::NoDispatch { .. })
    ));
    assert!(handler.pending_chord_sequence().is_none());

    let next = handler.route_normalized_input(
        char_input('j', KeyCode::KeyJ),
        &map,
        context,
        after_timeout,
    );
    match next {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(translated.command, Command::MoveDown);
        }
        other => panic!("expected router recovered after interrupt, got {:?}", other),
    }
}

#[test]
fn escape_cancels_pending_chord_without_fallback_dispatch() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context = KeybindingContext::for_mode(EditorMode::Normal);
    let t0 = std::time::Instant::now();

    let _ = handler.route_normalized_input(
        named_input(NamedKey::Space, Some(KeyCode::Space)),
        &map,
        context,
        t0,
    );
    assert!(handler.pending_chord_sequence().is_some());

    let cancelled = handler.route_normalized_input(
        named_input(NamedKey::Escape, Some(KeyCode::Escape)),
        &map,
        context,
        t0 + Duration::from_millis(500),
    );
    assert!(matches!(
        cancelled,
        Some(InputRouteOutcome::NoDispatch { .. })
    ));
    assert!(handler.pending_chord_sequence().is_none());
    assert_eq!(handler.get_pending_keys(), "");
}

#[test]
fn focus_loss_resets_prefix_safely() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context = KeybindingContext::for_mode(EditorMode::Normal);
    let now = std::time::Instant::now();

    let _ = handler.route_normalized_input(
        named_input(NamedKey::Space, Some(KeyCode::Space)),
        &map,
        context,
        now,
    );
    handler.on_focus_changed(false);

    // 'q' không được bind trong Normal mode ('f' giờ mở find-char pending).
    let after_focus_lost =
        handler.route_normalized_input(char_input('q', KeyCode::KeyQ), &map, context, now);
    assert!(after_focus_lost.is_none());

    let next = handler.route_normalized_input(char_input('j', KeyCode::KeyJ), &map, context, now);
    match next {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(translated.command, Command::MoveDown);
        }
        other => panic!(
            "expected router recovered after focus loss, got {:?}",
            other
        ),
    }
}

#[test]
fn bare_find_char_motion_waits_then_dispatches_move_find_char() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context = KeybindingContext::for_mode(EditorMode::Normal);
    let now = std::time::Instant::now();

    let pending =
        handler.route_normalized_input(char_input('f', KeyCode::KeyF), &map, context, now);
    assert!(matches!(
        pending,
        Some(InputRouteOutcome::NoDispatch { .. })
    ));

    let resolved =
        handler.route_normalized_input(char_input('x', KeyCode::KeyX), &map, context, now);
    match resolved {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(
                translated.command,
                Command::MoveFindChar(crate::core::commands::FindMotionKind::ForwardTo, 'x')
            );
        }
        other => panic!("expected find-char dispatch, got {:?}", other),
    }
}

#[test]
fn bare_find_char_motion_cancelled_by_escape() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context = KeybindingContext::for_mode(EditorMode::Normal);
    let now = std::time::Instant::now();

    let _ = handler.route_normalized_input(char_input('t', KeyCode::KeyT), &map, context, now);
    let _ = handler.route_normalized_input(
        named_input(NamedKey::Escape, Some(KeyCode::Escape)),
        &map,
        context,
        now,
    );

    // Sau Esc, 'j' phải dispatch motion bình thường (pending đã bị huỷ).
    let next = handler.route_normalized_input(char_input('j', KeyCode::KeyJ), &map, context, now);
    match next {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(translated.command, Command::MoveDown);
        }
        other => panic!("expected recovery after Esc, got {:?}", other),
    }
}

#[test]
fn repeated_motion_key_dispatches_while_holding() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context = KeybindingContext::for_mode(EditorMode::Normal);
    let now = std::time::Instant::now();

    let first = handler.route_normalized_input(char_input('j', KeyCode::KeyJ), &map, context, now);
    match first {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(translated.command, Command::MoveDown);
            assert_eq!(translated.repeat_count, 1);
        }
        other => panic!("expected initial j dispatch, got {:?}", other),
    }

    let repeated =
        handler.route_repeated_normalized_input(char_input('j', KeyCode::KeyJ), &map, context);
    match repeated {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(translated.command, Command::MoveDown);
            assert_eq!(translated.repeat_count, 1);
        }
        other => panic!("expected repeated j dispatch, got {:?}", other),
    }
}

#[test]
fn repeated_backspace_dispatches_while_holding() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context = KeybindingContext::for_mode(EditorMode::Insert);

    let repeated = handler.route_repeated_normalized_input(
        named_input(NamedKey::Backspace, Some(KeyCode::Backspace)),
        &map,
        context,
    );
    match repeated {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(translated.command, Command::Backspace);
            assert_eq!(translated.repeat_count, 1);
        }
        other => panic!("expected repeated backspace dispatch, got {:?}", other),
    }
}

#[test]
fn repeated_enter_dispatches_newline_while_holding() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context = KeybindingContext::for_mode(EditorMode::Insert);

    let repeated =
        handler.route_repeated_normalized_input(named_input(NamedKey::Enter, None), &map, context);
    match repeated {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(translated.command, Command::Newline);
            assert_eq!(translated.repeat_count, 1);
        }
        other => panic!("expected repeated enter dispatch, got {:?}", other),
    }
}

#[test]
fn repeated_toggle_command_is_ignored_while_holding() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context = KeybindingContext::for_mode(EditorMode::Normal);

    let repeated =
        handler.route_repeated_normalized_input(named_input(NamedKey::F12, None), &map, context);
    assert!(
        repeated.is_none(),
        "held toggle/system keys should be ignored"
    );
}

#[test]
fn repeated_chord_prefix_is_ignored_while_pending() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context = KeybindingContext::for_mode(EditorMode::Normal);
    let now = std::time::Instant::now();

    let first = handler.route_normalized_input(char_input('d', KeyCode::KeyD), &map, context, now);
    assert!(matches!(first, Some(InputRouteOutcome::NoDispatch { .. })));
    assert_eq!(handler.get_pending_keys(), "");

    let repeated =
        handler.route_repeated_normalized_input(char_input('d', KeyCode::KeyD), &map, context);
    assert!(repeated.is_none(), "held d should not auto-complete dd");
    assert_eq!(handler.get_pending_keys(), "");

    let next = handler.route_normalized_input(char_input('w', KeyCode::KeyW), &map, context, now);
    match next {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(
                translated.command,
                Command::Operate {
                    op: Operator::Delete,
                    target: OperationTarget::Motion(Motion::WordForward),
                }
            );
        }
        other => panic!("expected dw dispatch after ignored repeat, got {:?}", other),
    }
}

#[test]
fn leader_space_t_no_longer_maps_to_terminal_toggle() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context = KeybindingContext::for_mode(EditorMode::Normal);
    let now = std::time::Instant::now();

    let _ = handler.route_normalized_input(
        named_input(NamedKey::Space, Some(KeyCode::Space)),
        &map,
        context,
        now,
    );
    let mapped = handler.route_normalized_input(char_input('t', KeyCode::KeyT), &map, context, now);
    assert!(
        !matches!(
            mapped,
            Some(InputRouteOutcome::Dispatch(ref translated))
                if translated.command == Command::ToggleTerminal
        ),
        "leader+t should not resolve to terminal anymore"
    );
}

#[test]
fn get_pending_keys_returns_human_readable_chord_sequence() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context = KeybindingContext::for_mode(EditorMode::Normal);
    let now = std::time::Instant::now();

    let _ = handler.route_normalized_input(
        named_input(NamedKey::Space, Some(KeyCode::Space)),
        &map,
        context,
        now,
    );
    assert_eq!(handler.get_pending_keys(), "<Space>");

    let _ = handler.route_normalized_input(char_input('f', KeyCode::KeyF), &map, context, now);
    assert_eq!(handler.get_pending_keys(), "<Space> f");
}

#[test]
fn leader_d_s_dispatches_diagnostics_picker() {
    let mut handler = InputHandler::new();
    let map = make_default_profile_map();
    let context = KeybindingContext::for_mode(EditorMode::Normal);
    let now = std::time::Instant::now();

    let first = handler.route_normalized_input(
        named_input(NamedKey::Space, Some(KeyCode::Space)),
        &map,
        context,
        now,
    );
    assert!(matches!(first, Some(InputRouteOutcome::NoDispatch { .. })));
    assert_eq!(handler.get_pending_keys(), "<Space>");

    let second = handler.route_normalized_input(char_input('d', KeyCode::KeyD), &map, context, now);
    assert!(matches!(second, Some(InputRouteOutcome::NoDispatch { .. })));
    assert_eq!(handler.get_pending_keys(), "<Space> d");

    let third = handler.route_normalized_input(char_input('s', KeyCode::KeyS), &map, context, now);
    match third {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(translated.command, Command::DiagnosticsOpenPicker);
        }
        other => panic!("expected diagnostics picker dispatch, got {:?}", other),
    }
    assert_eq!(handler.get_pending_keys(), "");
}

#[test]
fn get_pending_keys_returns_operator_prefix() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context = KeybindingContext::for_mode(EditorMode::Normal);
    let now = std::time::Instant::now();

    let _ = handler.route_normalized_input(char_input('d', KeyCode::KeyD), &map, context, now);
    assert_eq!(handler.get_pending_keys(), "");
}

#[test]
fn terminal_focus_routes_printable_text_through_command_path() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context =
        KeybindingContext::with_focus(EditorMode::TerminalFocus, InputFocusContext::Terminal);
    let now = std::time::Instant::now();

    let mapped = handler.route_normalized_input(char_input('j', KeyCode::KeyJ), &map, context, now);
    match mapped {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(
                translated.command,
                Command::TerminalWriteInput("j".to_string())
            );
        }
        other => panic!("expected terminal dispatch, got {:?}", other),
    }
}

#[test]
fn terminal_focus_routes_arrow_keys_as_ansi_sequences() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context =
        KeybindingContext::with_focus(EditorMode::TerminalFocus, InputFocusContext::Terminal);
    let now = std::time::Instant::now();

    let mapped = handler.route_normalized_input(
        named_input(NamedKey::ArrowUp, Some(KeyCode::ArrowUp)),
        &map,
        context,
        now,
    );
    match mapped {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(
                translated.command,
                Command::TerminalWriteInput("\u{1b}[A".to_string())
            );
        }
        other => panic!("expected terminal arrow dispatch, got {:?}", other),
    }
}

#[test]
fn completion_popup_tab_accepts_selected_item() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context = completion_context();
    let now = std::time::Instant::now();

    let mapped = handler.route_normalized_input(
        named_input(NamedKey::Tab, Some(KeyCode::Tab)),
        &map,
        context,
        now,
    );
    match mapped {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(translated.command, Command::CompletionAccept);
        }
        other => panic!("expected completion accept dispatch, got {:?}", other),
    }
}

#[test]
fn completion_popup_ctrl_n_selects_next_item() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context = completion_context();
    let now = std::time::Instant::now();

    let mapped = handler.route_normalized_input(ctrl_input('n', KeyCode::KeyN), &map, context, now);
    match mapped {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(translated.command, Command::CompletionNext);
        }
        other => panic!("expected completion next dispatch, got {:?}", other),
    }
}

#[test]
fn completion_popup_ctrl_p_selects_prev_item() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context = completion_context();
    let now = std::time::Instant::now();

    let mapped = handler.route_normalized_input(ctrl_input('p', KeyCode::KeyP), &map, context, now);
    match mapped {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(translated.command, Command::CompletionPrev);
        }
        other => panic!("expected completion prev dispatch, got {:?}", other),
    }
}

#[test]
fn completion_popup_arrow_keys_still_navigate_items() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context = completion_context();
    let now = std::time::Instant::now();

    let down = handler.route_normalized_input(
        named_input(NamedKey::ArrowDown, Some(KeyCode::ArrowDown)),
        &map,
        context,
        now,
    );
    match down {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(translated.command, Command::CompletionNext);
        }
        other => panic!(
            "expected completion next dispatch for ArrowDown, got {:?}",
            other
        ),
    }

    let up = handler.route_normalized_input(
        named_input(NamedKey::ArrowUp, Some(KeyCode::ArrowUp)),
        &map,
        context,
        now,
    );
    match up {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(translated.command, Command::CompletionPrev);
        }
        other => panic!(
            "expected completion prev dispatch for ArrowUp, got {:?}",
            other
        ),
    }
}

#[test]
fn settings_focus_arrow_keys_adjust_values() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context = KeybindingContext::with_focus(EditorMode::Normal, InputFocusContext::SettingsTab);
    let now = std::time::Instant::now();

    let left = handler.route_normalized_input(
        named_input(NamedKey::ArrowLeft, Some(KeyCode::ArrowLeft)),
        &map,
        context,
        now,
    );
    match left {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(translated.command, Command::SettingsAdjustDecrease);
        }
        other => panic!("expected settings decrease dispatch, got {:?}", other),
    }

    let right = handler.route_normalized_input(
        named_input(NamedKey::ArrowRight, Some(KeyCode::ArrowRight)),
        &map,
        context,
        now,
    );
    match right {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(translated.command, Command::SettingsAdjustIncrease);
        }
        other => panic!("expected settings increase dispatch, got {:?}", other),
    }
}

#[test]
fn settings_focus_text_input_routes_to_editing_append() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context = KeybindingContext::with_focus(EditorMode::Insert, InputFocusContext::SettingsTab);
    let now = std::time::Instant::now();

    let typed = handler.route_normalized_input(char_input('a', KeyCode::KeyA), &map, context, now);
    match typed {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(
                translated.command,
                Command::FilePickerAppendQuery("a".to_string())
            );
        }
        other => panic!("expected settings text append dispatch, got {:?}", other),
    }
}

#[test]
fn palette_focus_text_input_routes_to_query_append() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let mut context =
        KeybindingContext::with_focus(EditorMode::PaletteFocus, InputFocusContext::Editor);
    context.command_palette_visible = true;
    let now = std::time::Instant::now();

    let typed = handler.route_normalized_input(char_input('x', KeyCode::KeyX), &map, context, now);
    match typed {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(
                translated.command,
                Command::FilePickerAppendQuery("x".to_string())
            );
        }
        other => panic!("expected palette text append dispatch, got {:?}", other),
    }
}

#[test]
fn settings_focus_j_and_k_navigate_in_normal_mode() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context = KeybindingContext::with_focus(EditorMode::Normal, InputFocusContext::SettingsTab);
    let now = std::time::Instant::now();

    let down = handler.route_normalized_input(char_input('j', KeyCode::KeyJ), &map, context, now);
    match down {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(translated.command, Command::SettingsSelectNext);
        }
        other => panic!("expected settings next dispatch, got {:?}", other),
    }

    let up = handler.route_normalized_input(char_input('k', KeyCode::KeyK), &map, context, now);
    match up {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(translated.command, Command::SettingsSelectPrev);
        }
        other => panic!("expected settings prev dispatch, got {:?}", other),
    }
}

#[test]
fn completion_popup_shift_tab_is_not_intercepted() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context = completion_context();
    let now = std::time::Instant::now();

    let mapped = handler.route_normalized_input(
        shift_named_input(NamedKey::Tab, Some(KeyCode::Tab)),
        &map,
        context,
        now,
    );
    assert!(
        !matches!(
            mapped,
            Some(InputRouteOutcome::Dispatch(ref translated))
                if translated.command == Command::CompletionPrev
        ),
        "Shift+Tab should no longer be intercepted as completion previous"
    );
}

#[test]
fn normal_mode_ctrl_d_dispatches_scroll_half_page_down() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context = KeybindingContext::for_mode(EditorMode::Normal);
    let now = std::time::Instant::now();

    let mapped = handler.route_normalized_input(ctrl_input('d', KeyCode::KeyD), &map, context, now);
    match mapped {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(translated.command, Command::ScrollHalfPageDown);
        }
        other => panic!("expected ctrl+d dispatch, got {:?}", other),
    }
}

#[test]
fn normal_mode_shift_c_dispatches_change_to_line_end() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context = KeybindingContext::for_mode(EditorMode::Normal);
    let now = std::time::Instant::now();

    let mapped = handler.route_normalized_input(
        NormalizedInput {
            physical_key: Some(KeyCode::KeyC),
            named_key: None,
            text: Some("C".to_string()),
            modifiers: ModifiersState::SHIFT,
        },
        &map,
        context,
        now,
    );
    match mapped {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(translated.command, Command::ChangeToLineEnd);
        }
        other => panic!("expected Shift+C dispatch, got {:?}", other),
    }
}

#[test]
fn normal_mode_shift_d_dispatches_delete_to_line_end() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context = KeybindingContext::for_mode(EditorMode::Normal);
    let now = std::time::Instant::now();

    let mapped = handler.route_normalized_input(
        NormalizedInput {
            physical_key: Some(KeyCode::KeyD),
            named_key: None,
            text: Some("D".to_string()),
            modifiers: ModifiersState::SHIFT,
        },
        &map,
        context,
        now,
    );
    match mapped {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(translated.command, Command::DeleteToLineEnd);
        }
        other => panic!("expected Shift+D dispatch, got {:?}", other),
    }
}

#[test]
fn terminal_focus_ctrl_q_enters_terminal_normal_mode() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context =
        KeybindingContext::with_focus(EditorMode::TerminalFocus, InputFocusContext::Terminal);
    let now = std::time::Instant::now();

    let mapped = handler.route_normalized_input(ctrl_input('q', KeyCode::KeyQ), &map, context, now);
    match mapped {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(
                translated.command,
                Command::SwitchMode(ModeEvent::EnterTerminalNormal)
            );
        }
        other => panic!("expected terminal normal dispatch, got {:?}", other),
    }
}

#[test]
fn terminal_focus_cmd_v_routes_terminal_paste() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context =
        KeybindingContext::with_focus(EditorMode::TerminalFocus, InputFocusContext::Terminal);
    let now = std::time::Instant::now();

    let mapped = handler.route_normalized_input(cmd_input('v', KeyCode::KeyV), &map, context, now);
    match mapped {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(translated.command, Command::TerminalPaste);
        }
        other => panic!("expected terminal paste dispatch, got {:?}", other),
    }
}

#[test]
fn buffer_terminal_cmd_v_routes_terminal_paste() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context =
        KeybindingContext::with_focus(EditorMode::TerminalFocus, InputFocusContext::BufferTerminal);
    let now = std::time::Instant::now();

    let mapped = handler.route_normalized_input(cmd_input('v', KeyCode::KeyV), &map, context, now);
    match mapped {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(translated.command, Command::TerminalPaste);
        }
        other => panic!("expected buffer terminal paste dispatch, got {:?}", other),
    }
}

#[test]
fn buffer_terminal_shifted_letter_without_text_routes_uppercase_raw_input() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context =
        KeybindingContext::with_focus(EditorMode::TerminalFocus, InputFocusContext::BufferTerminal);
    let now = std::time::Instant::now();

    let mapped = handler.route_normalized_input(
        NormalizedInput {
            physical_key: Some(KeyCode::KeyR),
            named_key: None,
            text: None,
            modifiers: ModifiersState::SHIFT,
        },
        &map,
        context,
        now,
    );
    match mapped {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(
                translated.command,
                Command::TerminalWriteInput("R".to_string())
            );
        }
        other => panic!(
            "expected shifted buffer terminal key to forward uppercase raw input, got {:?}",
            other
        ),
    }
}

#[test]
fn buffer_terminal_shifted_letter_with_lowercase_text_routes_uppercase_raw_input() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context =
        KeybindingContext::with_focus(EditorMode::TerminalFocus, InputFocusContext::BufferTerminal);
    let now = std::time::Instant::now();

    let mapped = handler.route_normalized_input(
        NormalizedInput {
            physical_key: Some(KeyCode::KeyR),
            named_key: None,
            text: Some("r".to_string()),
            modifiers: ModifiersState::SHIFT,
        },
        &map,
        context,
        now,
    );
    match mapped {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(
                translated.command,
                Command::TerminalWriteInput("R".to_string())
            );
        }
        other => panic!(
            "expected shifted buffer terminal text to forward uppercase raw input, got {:?}",
            other
        ),
    }
}

#[test]
fn buffer_terminal_repeated_printable_input_still_routes_raw_input() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context =
        KeybindingContext::with_focus(EditorMode::TerminalFocus, InputFocusContext::BufferTerminal);

    let mapped = handler.route_repeated_normalized_input(
        NormalizedInput {
            physical_key: Some(KeyCode::KeyR),
            named_key: None,
            text: Some("R".to_string()),
            modifiers: ModifiersState::SHIFT,
        },
        &map,
        context,
    );
    match mapped {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(
                translated.command,
                Command::TerminalWriteInput("R".to_string())
            );
        }
        other => panic!(
            "expected repeated buffer terminal key to forward raw input, got {:?}",
            other
        ),
    }
}

#[test]
fn terminal_focus_ctrl_h_routes_raw_control_char() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context =
        KeybindingContext::with_focus(EditorMode::TerminalFocus, InputFocusContext::Terminal);
    let now = std::time::Instant::now();

    let mapped = handler.route_normalized_input(ctrl_input('h', KeyCode::KeyH), &map, context, now);
    match mapped {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(
                translated.command,
                Command::TerminalWriteInput("\u{08}".to_string())
            );
        }
        other => panic!("expected raw ctrl+h terminal dispatch, got {:?}", other),
    }
}

#[test]
fn terminal_focus_ctrl_w_stays_raw_instead_of_triggering_global_focus_back() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context =
        KeybindingContext::with_focus(EditorMode::TerminalFocus, InputFocusContext::Terminal);
    let now = std::time::Instant::now();

    let mapped = handler.route_normalized_input(ctrl_input('w', KeyCode::KeyW), &map, context, now);
    match mapped {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(
                translated.command,
                Command::TerminalWriteInput("\u{17}".to_string())
            );
        }
        other => panic!("expected raw ctrl+w terminal dispatch, got {:?}", other),
    }
}

#[test]
fn terminal_normal_routes_vim_motions_without_forwarding_raw_input() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context =
        KeybindingContext::with_focus(EditorMode::TerminalNormal, InputFocusContext::Terminal);
    let now = std::time::Instant::now();

    let mapped = handler.route_normalized_input(char_input('j', KeyCode::KeyJ), &map, context, now);
    match mapped {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(translated.command, Command::MoveDown);
        }
        other => panic!("expected terminal normal motion dispatch, got {:?}", other),
    }
}

#[test]
fn terminal_focus_f12_maps_to_focus_terminal() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context =
        KeybindingContext::with_focus(EditorMode::TerminalFocus, InputFocusContext::Terminal);
    let now = std::time::Instant::now();

    let mapped =
        handler.route_normalized_input(named_input(NamedKey::F12, None), &map, context, now);
    match mapped {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(translated.command, Command::FocusTerminal);
        }
        other => panic!("expected F12 dispatch in terminal focus, got {:?}", other),
    }
}

#[test]
fn terminal_normal_does_not_forward_unbound_text_to_pty() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context =
        KeybindingContext::with_focus(EditorMode::TerminalNormal, InputFocusContext::Terminal);
    let now = std::time::Instant::now();

    let mapped = handler.route_normalized_input(char_input('x', KeyCode::KeyX), &map, context, now);
    assert!(
        mapped.is_none(),
        "unbound terminal-normal key should be ignored"
    );
}

#[test]
fn ime_commit_is_redirected_to_file_picker_when_palette_is_open() {
    let handler = InputHandler::new();
    let translated = handler
        .translate_ime_commit(
            "src",
            KeybindingContext::for_mode_with_picker(EditorMode::PaletteFocus, true),
        )
        .expect("palette ime commit should translate");

    assert_eq!(
        translated.command,
        Command::FilePickerAppendQuery("src".to_string())
    );
}

#[test]
fn ai_agent_picker_j_k_navigate() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let mut context = KeybindingContext::with_focus(EditorMode::Normal, InputFocusContext::AiChat);
    context.ai_agent_picker_active = true;
    let now = std::time::Instant::now();
    let j = NormalizedInput {
        physical_key: Some(KeyCode::KeyJ),
        named_key: None,
        text: Some("j".to_string()),
        modifiers: ModifiersState::empty(),
    };

    match handler.route_normalized_input(j, &map, context, now) {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(translated.command, Command::AiAgentPickerNext);
        }
        other => panic!("expected agent picker next, got {:?}", other),
    }
}

#[test]
fn ai_agent_picker_enter_launches() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let mut context = KeybindingContext::with_focus(EditorMode::Normal, InputFocusContext::AiChat);
    context.ai_agent_picker_active = true;
    let now = std::time::Instant::now();

    match handler.route_normalized_input(named_input(NamedKey::Enter, None), &map, context, now) {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(translated.command, Command::AiAgentPickerLaunch);
        }
        other => panic!("expected agent picker launch, got {:?}", other),
    }
}

#[test]
fn buffer_terminal_f12_maps_to_focus_terminal() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context =
        KeybindingContext::with_focus(EditorMode::TerminalFocus, InputFocusContext::BufferTerminal);
    let now = std::time::Instant::now();

    let mapped =
        handler.route_normalized_input(named_input(NamedKey::F12, None), &map, context, now);
    match mapped {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(translated.command, Command::FocusTerminal);
        }
        other => panic!(
            "expected F12 dispatch in buffer terminal focus, got {:?}",
            other
        ),
    }
}

#[test]
fn buffer_terminal_cmd_r_maps_to_focus_inspector() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context =
        KeybindingContext::with_focus(EditorMode::TerminalFocus, InputFocusContext::BufferTerminal);
    let now = std::time::Instant::now();

    let input = NormalizedInput {
        physical_key: Some(KeyCode::KeyR),
        named_key: None,
        text: None,
        modifiers: ModifiersState::SUPER,
    };

    let mapped = handler.route_normalized_input(input, &map, context, now);
    match mapped {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(translated.command, Command::FocusInspector);
        }
        other => panic!(
            "expected Cmd-R dispatch in buffer terminal focus, got {:?}",
            other
        ),
    }
}

#[test]
fn test_right_dock_switching_under_various_focuses() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let now = std::time::Instant::now();

    // Helper to build Cmd-1 input
    let cmd_1 = NormalizedInput {
        physical_key: Some(KeyCode::Digit1),
        named_key: None,
        text: None,
        modifiers: ModifiersState::SUPER,
    };

    // Helper to build Cmd-2 input
    let cmd_2 = NormalizedInput {
        physical_key: Some(KeyCode::Digit2),
        named_key: None,
        text: None,
        modifiers: ModifiersState::SUPER,
    };

    // 1. Focused on Center Editor (Normal mode) -> Cmd-1 switches right dock tab
    let context_editor =
        KeybindingContext::with_focus(EditorMode::Normal, InputFocusContext::Editor);
    let mapped = handler.route_normalized_input(cmd_1.clone(), &map, context_editor, now);
    assert!(
        matches!(mapped, Some(InputRouteOutcome::Dispatch(ref trans)) if trans.command == Command::SwitchRightTab(0)),
        "Expected SwitchRightTab(0) from editor normal mode focus, got {:?}",
        mapped
    );

    // 2. Focused on AiChat -> Cmd-2 switches right dock tab
    let context_aichat =
        KeybindingContext::with_focus(EditorMode::Normal, InputFocusContext::AiChat);
    let mapped = handler.route_normalized_input(cmd_2.clone(), &map, context_aichat, now);
    assert!(
        matches!(mapped, Some(InputRouteOutcome::Dispatch(ref trans)) if trans.command == Command::SwitchRightTab(1)),
        "Expected SwitchRightTab(1) from AiChat focus, got {:?}",
        mapped
    );

    // 3. Focused on TestRunner -> Cmd-2 switches right dock tab
    let context_runner =
        KeybindingContext::with_focus(EditorMode::Normal, InputFocusContext::TestRunner);
    let mapped = handler.route_normalized_input(cmd_2.clone(), &map, context_runner, now);
    assert!(
        matches!(mapped, Some(InputRouteOutcome::Dispatch(ref trans)) if trans.command == Command::SwitchRightTab(1)),
        "Expected SwitchRightTab(1) from TestRunner focus, got {:?}",
        mapped
    );

    // 3-bug. Focused on TestRunner with a drifted TerminalFocus mode (left over
    // from switching off the AI Chat terminal tab) -> Cmd-1 must still switch the
    // right dock tab, not fall through to the terminal-tab bindings.
    let mut context_runner_drifted =
        KeybindingContext::with_focus(EditorMode::TerminalFocus, InputFocusContext::TestRunner);
    context_runner_drifted.right_sidebar_terminal = false;
    let mapped =
        handler.route_normalized_input(cmd_1.clone(), &map, context_runner_drifted, now);
    assert!(
        matches!(mapped, Some(InputRouteOutcome::Dispatch(ref trans)) if trans.command == Command::SwitchRightTab(0)),
        "Expected SwitchRightTab(0) from TestRunner focus with drifted terminal mode, got {:?}",
        mapped
    );

    // 3a. Focused on Explorer -> Cmd-2 switches Left Dock tab
    let context_explorer =
        KeybindingContext::with_focus(EditorMode::Normal, InputFocusContext::Explorer);
    let mapped_left_2 = handler.route_normalized_input(cmd_2.clone(), &map, context_explorer, now);
    assert!(
        matches!(mapped_left_2, Some(InputRouteOutcome::Dispatch(ref trans)) if trans.command == Command::SwitchLeftTab(1)),
        "Expected SwitchLeftTab(1) from Explorer focus, got {:?}",
        mapped_left_2
    );

    // 3b. Focused on Outline -> Cmd-1 switches Left Dock tab
    let context_outline =
        KeybindingContext::with_focus(EditorMode::Normal, InputFocusContext::Outline);
    let mapped_left_1 = handler.route_normalized_input(cmd_1.clone(), &map, context_outline, now);
    assert!(
        matches!(mapped_left_1, Some(InputRouteOutcome::Dispatch(ref trans)) if trans.command == Command::SwitchLeftTab(0)),
        "Expected SwitchLeftTab(0) from Outline focus, got {:?}",
        mapped_left_1
    );

    // 4. Focused on bottom terminal (TerminalFocus mode, right_sidebar_terminal: false) -> Cmd-2 switches terminal tab
    let mut context_bottom_terminal =
        KeybindingContext::with_focus(EditorMode::TerminalFocus, InputFocusContext::Terminal);
    context_bottom_terminal.right_sidebar_terminal = false;
    let mapped = handler.route_normalized_input(cmd_2.clone(), &map, context_bottom_terminal, now);
    assert!(
        matches!(mapped, Some(InputRouteOutcome::Dispatch(ref trans)) if trans.command == Command::SwitchTerminalTab(1)),
        "Expected SwitchTerminalTab(1) from bottom terminal focus, got {:?}",
        mapped
    );

    // 5. Focused on right sidebar terminal (TerminalFocus mode, right_sidebar_terminal: true) -> Cmd-1 switches right dock tab
    let mut context_right_terminal =
        KeybindingContext::with_focus(EditorMode::TerminalFocus, InputFocusContext::Terminal);
    context_right_terminal.right_sidebar_terminal = true;
    let mapped = handler.route_normalized_input(cmd_1.clone(), &map, context_right_terminal, now);
    assert!(
        matches!(mapped, Some(InputRouteOutcome::Dispatch(ref trans)) if trans.command == Command::SwitchRightTab(0)),
        "Expected SwitchRightTab(0) from right sidebar terminal focus, got {:?}",
        mapped
    );
}

#[test]
fn test_outline_navigation_routing() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let now = std::time::Instant::now();
    let context = KeybindingContext::with_focus(EditorMode::Normal, InputFocusContext::Outline);

    // 1. Pressing 'j' -> OutlineNext
    let j_input = NormalizedInput {
        physical_key: Some(KeyCode::KeyJ),
        named_key: None,
        text: Some("j".to_string()),
        modifiers: ModifiersState::empty(),
    };
    let mapped = handler.route_normalized_input(j_input, &map, context, now);
    assert!(
        matches!(mapped, Some(InputRouteOutcome::Dispatch(ref trans)) if trans.command == Command::OutlineNext),
        "Expected OutlineNext command, got {:?}",
        mapped
    );

    // 2. Pressing 'k' -> OutlinePrev
    let k_input = NormalizedInput {
        physical_key: Some(KeyCode::KeyK),
        named_key: None,
        text: Some("k".to_string()),
        modifiers: ModifiersState::empty(),
    };
    let mapped = handler.route_normalized_input(k_input, &map, context, now);
    assert!(
        matches!(mapped, Some(InputRouteOutcome::Dispatch(ref trans)) if trans.command == Command::OutlinePrev),
        "Expected OutlinePrev command, got {:?}",
        mapped
    );

    // 3. Pressing Enter -> OutlineConfirm
    let enter_input = NormalizedInput {
        physical_key: Some(KeyCode::Enter),
        named_key: Some(NamedKey::Enter),
        text: None,
        modifiers: ModifiersState::empty(),
    };
    let mapped = handler.route_normalized_input(enter_input, &map, context, now);
    assert!(
        matches!(mapped, Some(InputRouteOutcome::Dispatch(ref trans)) if trans.command == Command::OutlineConfirm),
        "Expected OutlineConfirm command, got {:?}",
        mapped
    );

    // 4. Pressing Esc -> FocusEditor
    let esc_input = NormalizedInput {
        physical_key: Some(KeyCode::Escape),
        named_key: Some(NamedKey::Escape),
        text: None,
        modifiers: ModifiersState::empty(),
    };
    let mapped = handler.route_normalized_input(esc_input, &map, context, now);
    assert!(
        matches!(mapped, Some(InputRouteOutcome::Dispatch(ref trans)) if trans.command == Command::FocusEditor),
        "Expected FocusEditor command, got {:?}",
        mapped
    );
}

#[test]
fn outline_navigation_supports_key_repeat_and_first_last_shortcuts() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let now = std::time::Instant::now();
    let context = KeybindingContext::with_focus(EditorMode::Normal, InputFocusContext::Outline);

    let repeated_j =
        handler.route_repeated_normalized_input(char_input('j', KeyCode::KeyJ), &map, context);
    assert!(
        matches!(repeated_j, Some(InputRouteOutcome::Dispatch(ref translated)) if translated.command == Command::OutlineNext),
        "held j should continue moving the Outline selection, got {repeated_j:?}"
    );

    let repeated_up = handler.route_repeated_normalized_input(
        named_input(NamedKey::ArrowUp, Some(KeyCode::ArrowUp)),
        &map,
        context,
    );
    assert!(
        matches!(repeated_up, Some(InputRouteOutcome::Dispatch(ref translated)) if translated.command == Command::OutlinePrev),
        "held Up should continue moving the Outline selection, got {repeated_up:?}"
    );

    let first_g =
        handler.route_normalized_input(char_input('g', KeyCode::KeyG), &map, context, now);
    assert!(matches!(
        first_g,
        Some(InputRouteOutcome::NoDispatch { .. })
    ));
    assert_eq!(handler.get_pending_keys(), "g");

    let second_g =
        handler.route_normalized_input(char_input('g', KeyCode::KeyG), &map, context, now);
    assert!(
        matches!(second_g, Some(InputRouteOutcome::Dispatch(ref translated)) if translated.command == Command::OutlineFirst),
        "gg should select the first Outline symbol, got {second_g:?}"
    );

    let uppercase_g =
        handler.route_normalized_input(char_input('G', KeyCode::KeyG), &map, context, now);
    assert!(
        matches!(uppercase_g, Some(InputRouteOutcome::Dispatch(ref translated)) if translated.command == Command::OutlineLast),
        "G should select the last Outline symbol, got {uppercase_g:?}"
    );
}
