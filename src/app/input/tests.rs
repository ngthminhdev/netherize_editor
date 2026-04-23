use std::{path::PathBuf, time::Duration};

use winit::keyboard::{KeyCode, ModifiersState, NamedKey};

use crate::{
    app::{
        input_map::{InputFocusContext, InputMap, KeybindingContext},
        resolved_keymap::builtin_defaults,
    },
    core::{
        commands::Command,
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

fn named_input(named: NamedKey, physical: Option<KeyCode>) -> NormalizedInput {
    NormalizedInput {
        physical_key: physical,
        named_key: Some(named),
        text: None,
        modifiers: ModifiersState::empty(),
    }
}

fn make_map() -> InputMap {
    InputMap::with_keymap(PathBuf::from("phase7_test.txt"), builtin_defaults())
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
            assert_eq!(translated.command, Command::DeleteCurrentLine);
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
            assert_eq!(translated.command, Command::YankCurrentLine);
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
            assert_eq!(translated.command, Command::YankToWordEnd);
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
    assert_eq!(handler.get_pending_keys(), "d 2");

    let second = handler.route_normalized_input(char_input('w', KeyCode::KeyW), &map, context, t0);
    match second {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(translated.command, Command::DeleteWordForward);
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
    assert_eq!(handler.get_pending_keys(), "3 d");

    let second = handler.route_normalized_input(char_input('w', KeyCode::KeyW), &map, context, t0);
    match second {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(translated.command, Command::DeleteWordForward);
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
    assert_eq!(handler.get_pending_keys(), "3 d 2");

    let second = handler.route_normalized_input(char_input('w', KeyCode::KeyW), &map, context, t0);
    match second {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(translated.command, Command::DeleteWordForward);
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
            assert_eq!(translated.command, Command::ChangeWordForward);
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
            assert_eq!(translated.command, Command::ChangeWordBackward);
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
fn prefix_timeout_resets_safely() {
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

    let after_timeout = t0 + Duration::from_millis(1_700);
    let expired = handler.route_normalized_input(
        char_input('f', KeyCode::KeyF),
        &map,
        context,
        after_timeout,
    );
    assert!(expired.is_none());

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
        other => panic!("expected router recovered after timeout, got {:?}", other),
    }
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

    let after_focus_lost =
        handler.route_normalized_input(char_input('f', KeyCode::KeyF), &map, context, now);
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
fn repeated_chord_prefix_is_ignored_while_pending() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context = KeybindingContext::for_mode(EditorMode::Normal);
    let now = std::time::Instant::now();

    let first = handler.route_normalized_input(char_input('d', KeyCode::KeyD), &map, context, now);
    assert!(matches!(first, Some(InputRouteOutcome::NoDispatch { .. })));
    assert_eq!(handler.get_pending_keys(), "d");

    let repeated =
        handler.route_repeated_normalized_input(char_input('d', KeyCode::KeyD), &map, context);
    assert!(repeated.is_none(), "held d should not auto-complete dd");
    assert_eq!(handler.get_pending_keys(), "d");

    let next = handler.route_normalized_input(char_input('w', KeyCode::KeyW), &map, context, now);
    match next {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(translated.command, Command::DeleteWordForward);
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
fn get_pending_keys_returns_operator_prefix() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context = KeybindingContext::for_mode(EditorMode::Normal);
    let now = std::time::Instant::now();

    let _ = handler.route_normalized_input(char_input('d', KeyCode::KeyD), &map, context, now);
    assert_eq!(handler.get_pending_keys(), "d");
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
fn terminal_focus_mod_v_routes_terminal_paste() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context =
        KeybindingContext::with_focus(EditorMode::TerminalFocus, InputFocusContext::Terminal);
    let now = std::time::Instant::now();

    let mapped = handler.route_normalized_input(ctrl_input('v', KeyCode::KeyV), &map, context, now);
    match mapped {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(translated.command, Command::TerminalPaste);
        }
        other => panic!("expected terminal paste dispatch, got {:?}", other),
    }
}

#[test]
fn buffer_terminal_mod_v_routes_terminal_paste() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let context =
        KeybindingContext::with_focus(EditorMode::TerminalFocus, InputFocusContext::BufferTerminal);
    let now = std::time::Instant::now();

    let mapped = handler.route_normalized_input(ctrl_input('v', KeyCode::KeyV), &map, context, now);
    match mapped {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(translated.command, Command::TerminalPaste);
        }
        other => panic!("expected buffer terminal paste dispatch, got {:?}", other),
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
