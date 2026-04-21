use std::path::PathBuf;

use winit::keyboard::{KeyCode, ModifiersState, NamedKey};

use crate::{
    app::resolved_keymap::builtin_defaults,
    core::{
        commands::Command,
        mode::{EditorMode, ModeEvent},
    },
};

use super::{InputFocusContext, InputMap, KeybindingContext, NormalizedInput, SequenceMatch};

fn make_map() -> InputMap {
    InputMap::with_keymap(PathBuf::from("test_open.txt"), builtin_defaults())
}

fn input_from_named(named_key: NamedKey) -> NormalizedInput {
    NormalizedInput {
        physical_key: None,
        named_key: Some(named_key),
        text: None,
        modifiers: ModifiersState::empty(),
    }
}

#[test]
fn table_driven_keybinding_resolution() {
    struct Case {
        name: &'static str,
        context: KeybindingContext,
        input: NormalizedInput,
        expected: Option<Command>,
    }

    let map = make_map();
    let cases = [
        Case {
            name: "insert j -> InsertChar",
            context: KeybindingContext::for_mode(EditorMode::Insert),
            input: NormalizedInput {
                physical_key: Some(KeyCode::KeyJ),
                named_key: None,
                text: Some("j".to_string()),
                modifiers: ModifiersState::empty(),
            },
            expected: Some(Command::InsertChar('j')),
        },
        Case {
            name: "normal j -> MoveDown",
            context: KeybindingContext::for_mode(EditorMode::Normal),
            input: NormalizedInput {
                physical_key: Some(KeyCode::KeyJ),
                named_key: None,
                text: Some("j".to_string()),
                modifiers: ModifiersState::empty(),
            },
            expected: Some(Command::MoveDown),
        },
        Case {
            name: "insert escape -> SwitchToNormal",
            context: KeybindingContext::for_mode(EditorMode::Insert),
            input: input_from_named(NamedKey::Escape),
            expected: Some(Command::SwitchMode(ModeEvent::EnterNormal)),
        },
        Case {
            name: "cmd/ctrl p -> OpenFilePicker",
            context: KeybindingContext::for_mode(EditorMode::Insert),
            input: NormalizedInput {
                physical_key: Some(KeyCode::KeyP),
                named_key: None,
                text: Some("p".to_string()),
                modifiers: ModifiersState::CONTROL,
            },
            expected: Some(Command::OpenFilePicker),
        },
        Case {
            name: "terminal focus j -> None",
            context: KeybindingContext::with_focus(
                EditorMode::TerminalFocus,
                InputFocusContext::Terminal,
            ),
            input: NormalizedInput {
                physical_key: Some(KeyCode::KeyJ),
                named_key: None,
                text: Some("j".to_string()),
                modifiers: ModifiersState::empty(),
            },
            expected: None,
        },
        Case {
            name: "terminal focus ctrl+s -> None (forward to PTY)",
            context: KeybindingContext::with_focus(
                EditorMode::TerminalFocus,
                InputFocusContext::Terminal,
            ),
            input: NormalizedInput {
                physical_key: Some(KeyCode::KeyS),
                named_key: None,
                text: Some("s".to_string()),
                modifiers: ModifiersState::CONTROL,
            },
            expected: None,
        },
        Case {
            name: "terminal focus escape -> FocusEditor",
            context: KeybindingContext::with_focus(
                EditorMode::TerminalFocus,
                InputFocusContext::Terminal,
            ),
            input: input_from_named(NamedKey::Escape),
            expected: Some(Command::FocusEditor),
        },
        Case {
            name: "normal p without prefix -> None",
            context: KeybindingContext::for_mode(EditorMode::Normal),
            input: NormalizedInput {
                physical_key: Some(KeyCode::KeyP),
                named_key: None,
                text: Some("p".to_string()),
                modifiers: ModifiersState::empty(),
            },
            expected: None,
        },
        Case {
            name: "normal a -> AppendAfterCursor",
            context: KeybindingContext::for_mode(EditorMode::Normal),
            input: NormalizedInput {
                physical_key: Some(KeyCode::KeyA),
                named_key: None,
                text: Some("a".to_string()),
                modifiers: ModifiersState::empty(),
            },
            expected: Some(Command::AppendAfterCursor),
        },
        Case {
            name: "normal Shift+O -> InsertLineAbove",
            context: KeybindingContext::for_mode(EditorMode::Normal),
            input: NormalizedInput {
                physical_key: Some(KeyCode::KeyO),
                named_key: None,
                text: Some("O".to_string()),
                modifiers: ModifiersState::SHIFT,
            },
            expected: Some(Command::InsertLineAbove),
        },
        Case {
            name: "normal Shift+I -> InsertAtLineStart",
            context: KeybindingContext::for_mode(EditorMode::Normal),
            input: NormalizedInput {
                physical_key: Some(KeyCode::KeyI),
                named_key: None,
                text: Some("I".to_string()),
                modifiers: ModifiersState::SHIFT,
            },
            expected: Some(Command::InsertAtLineStart),
        },
        Case {
            name: "normal backtick -> ToggleTerminal",
            context: KeybindingContext::for_mode(EditorMode::Normal),
            input: NormalizedInput {
                physical_key: Some(KeyCode::Backquote),
                named_key: None,
                text: Some("`".to_string()),
                modifiers: ModifiersState::empty(),
            },
            expected: Some(Command::ToggleTerminal),
        },
        Case {
            name: "palette text -> FilePickerAppendQuery",
            context: KeybindingContext::for_mode_with_picker(EditorMode::PaletteFocus, true),
            input: NormalizedInput {
                physical_key: Some(KeyCode::KeyA),
                named_key: None,
                text: Some("a".to_string()),
                modifiers: ModifiersState::empty(),
            },
            expected: Some(Command::FilePickerAppendQuery("a".to_string())),
        },
        Case {
            name: "palette enter -> FilePickerConfirmSelection",
            context: KeybindingContext::for_mode_with_picker(EditorMode::PaletteFocus, true),
            input: input_from_named(NamedKey::Enter),
            expected: Some(Command::FilePickerConfirmSelection),
        },
        Case {
            name: "palette without picker text -> None",
            context: KeybindingContext::for_mode(EditorMode::PaletteFocus),
            input: NormalizedInput {
                physical_key: Some(KeyCode::KeyA),
                named_key: None,
                text: Some("a".to_string()),
                modifiers: ModifiersState::empty(),
            },
            expected: None,
        },
    ];

    for case in cases {
        let actual = map
            .resolve(&case.input, case.context)
            .map(|matched| matched.command);
        assert_eq!(actual, case.expected, "case={}", case.name);
    }
}

#[test]
fn ctrl_s_maps_to_save_file() {
    let map = make_map();
    let input = NormalizedInput {
        physical_key: Some(KeyCode::KeyS),
        named_key: None,
        text: Some("s".to_string()),
        modifiers: ModifiersState::CONTROL,
    };
    assert_eq!(
        map.translate(&input, KeybindingContext::for_mode(EditorMode::Insert)),
        Some(Command::SaveFile)
    );
}

#[test]
fn ctrl_backslash_maps_to_toggle_terminal() {
    let map = make_map();
    let input = NormalizedInput {
        physical_key: Some(KeyCode::Backslash),
        named_key: None,
        text: Some("\\".to_string()),
        modifiers: ModifiersState::CONTROL,
    };
    assert_eq!(
        map.translate(&input, KeybindingContext::for_mode(EditorMode::Normal)),
        Some(Command::ToggleTerminal)
    );
}

#[test]
fn char_without_modifiers_maps_to_insert_char_in_insert_mode() {
    let map = make_map();
    let input = NormalizedInput {
        physical_key: Some(KeyCode::KeyA),
        named_key: None,
        text: Some("a".to_string()),
        modifiers: ModifiersState::empty(),
    };
    assert_eq!(
        map.translate(&input, KeybindingContext::for_mode(EditorMode::Insert)),
        Some(Command::InsertChar('a'))
    );
}

#[test]
fn named_space_maps_to_insert_space() {
    let map = make_map();
    let input = NormalizedInput {
        physical_key: Some(KeyCode::Space),
        named_key: Some(NamedKey::Space),
        text: None,
        modifiers: ModifiersState::empty(),
    };
    assert_eq!(
        map.translate(&input, KeybindingContext::for_mode(EditorMode::Insert)),
        Some(Command::InsertChar(' '))
    );
}

#[test]
fn named_enter_maps_to_newline() {
    let map = make_map();
    let input = NormalizedInput {
        physical_key: Some(KeyCode::Enter),
        named_key: Some(NamedKey::Enter),
        text: None,
        modifiers: ModifiersState::empty(),
    };
    assert_eq!(
        map.translate(&input, KeybindingContext::for_mode(EditorMode::Insert)),
        Some(Command::Newline)
    );
}

#[test]
fn multi_codepoint_text_maps_to_insert_text() {
    let map = make_map();
    let multi = "e\u{0302}".to_string();
    let input = NormalizedInput {
        physical_key: None,
        named_key: None,
        text: Some(multi.clone()),
        modifiers: ModifiersState::empty(),
    };
    assert_eq!(
        map.translate(&input, KeybindingContext::for_mode(EditorMode::Insert)),
        Some(Command::InsertText(multi))
    );
}

#[test]
fn resolve_contains_reason_for_dispatch_trace() {
    let map = make_map();
    let input = NormalizedInput {
        physical_key: Some(KeyCode::KeyJ),
        named_key: None,
        text: Some("j".to_string()),
        modifiers: ModifiersState::empty(),
    };
    let resolved = map
        .resolve(&input, KeybindingContext::for_mode(EditorMode::Normal))
        .expect("should resolve");
    assert_eq!(resolved.command, Command::MoveDown);
    assert!(!resolved.reason.is_empty());
}

#[test]
fn leader_and_chord_resolution_work() {
    let map = make_map();
    let context = KeybindingContext::for_mode(EditorMode::Normal);
    let space = input_from_named(NamedKey::Space);
    let first = map
        .resolve_sequence_start(&space, context)
        .expect("space should start chord");

    let follow_f = NormalizedInput {
        physical_key: Some(KeyCode::KeyF),
        named_key: None,
        text: Some("f".to_string()),
        modifiers: ModifiersState::empty(),
    };
    let pending = match first {
        SequenceMatch::Pending(pending) => pending,
        other => panic!(
            "expected pending sequence after leader start, got {:?}",
            other
        ),
    };

    let second = map
        .resolve_sequence_next(&pending, &follow_f, context)
        .expect("leader+f should still be pending");
    let pending = match second {
        SequenceMatch::Pending(pending) => pending,
        other => panic!("expected second pending sequence, got {:?}", other),
    };

    let third = map
        .resolve_sequence_next(&pending, &follow_f, context)
        .expect("leader+f+f should resolve");
    match third {
        SequenceMatch::Dispatch(resolved) => {
            assert_eq!(resolved.command, Command::OpenFileFinder);
        }
        other => panic!("expected dispatch for leader f f, got {:?}", other),
    }
}

#[test]
fn leader_sequence_is_not_started_in_insert_mode() {
    let map = make_map();
    let start_input = input_from_named(NamedKey::Space);
    let context = KeybindingContext::for_mode(EditorMode::Insert);
    assert!(map.resolve_sequence_start(&start_input, context).is_none());
}

#[test]
fn dw_sequence_maps_to_delete_word_forward() {
    let map = make_map();
    let context = KeybindingContext::for_mode(EditorMode::Normal);
    let d = NormalizedInput {
        physical_key: Some(KeyCode::KeyD),
        named_key: None,
        text: Some("d".to_string()),
        modifiers: ModifiersState::empty(),
    };
    let w = NormalizedInput {
        physical_key: Some(KeyCode::KeyW),
        named_key: None,
        text: Some("w".to_string()),
        modifiers: ModifiersState::empty(),
    };

    let first = map
        .resolve_sequence_start(&d, context)
        .expect("first d should start sequence");
    let pending = match first {
        SequenceMatch::Pending(pending) => pending,
        other => panic!("expected pending after d, got {:?}", other),
    };

    let second = map
        .resolve_sequence_next(&pending, &w, context)
        .expect("d w should resolve");
    match second {
        SequenceMatch::Dispatch(resolved) => {
            assert_eq!(resolved.command, Command::DeleteWordForward);
        }
        other => panic!("expected dispatch for d w, got {:?}", other),
    }
}

#[test]
fn dd_sequence_maps_to_delete_current_line() {
    let map = make_map();
    let context = KeybindingContext::for_mode(EditorMode::Normal);
    let d = NormalizedInput {
        physical_key: Some(KeyCode::KeyD),
        named_key: None,
        text: Some("d".to_string()),
        modifiers: ModifiersState::empty(),
    };

    let first = map
        .resolve_sequence_start(&d, context)
        .expect("first d should start sequence");
    let pending = match first {
        SequenceMatch::Pending(pending) => pending,
        other => panic!("expected pending for first d, got {:?}", other),
    };

    let second = map
        .resolve_sequence_next(&pending, &d, context)
        .expect("second d should resolve");
    match second {
        SequenceMatch::Dispatch(resolved) => {
            assert_eq!(resolved.command, Command::DeleteCurrentLine);
        }
        other => panic!("expected dispatch for d d, got {:?}", other),
    }
}

#[test]
fn db_sequence_maps_to_delete_word_backward() {
    let map = make_map();
    let context = KeybindingContext::for_mode(EditorMode::Normal);
    let d = NormalizedInput {
        physical_key: Some(KeyCode::KeyD),
        named_key: None,
        text: Some("d".to_string()),
        modifiers: ModifiersState::empty(),
    };
    let b = NormalizedInput {
        physical_key: Some(KeyCode::KeyB),
        named_key: None,
        text: Some("b".to_string()),
        modifiers: ModifiersState::empty(),
    };

    let first = map
        .resolve_sequence_start(&d, context)
        .expect("first d should start sequence");
    let pending = match first {
        SequenceMatch::Pending(pending) => pending,
        other => panic!("expected pending after first d, got {:?}", other),
    };

    let second = map
        .resolve_sequence_next(&pending, &b, context)
        .expect("d b should resolve");
    match second {
        SequenceMatch::Dispatch(resolved) => {
            assert_eq!(resolved.command, Command::DeleteWordBackward);
        }
        other => panic!("expected dispatch for d b, got {:?}", other),
    }
}

#[test]
fn cw_sequence_maps_to_change_word_forward() {
    let map = make_map();
    let context = KeybindingContext::for_mode(EditorMode::Normal);
    let c = NormalizedInput {
        physical_key: Some(KeyCode::KeyC),
        named_key: None,
        text: Some("c".to_string()),
        modifiers: ModifiersState::empty(),
    };
    let w = NormalizedInput {
        physical_key: Some(KeyCode::KeyW),
        named_key: None,
        text: Some("w".to_string()),
        modifiers: ModifiersState::empty(),
    };

    let first = map
        .resolve_sequence_start(&c, context)
        .expect("first c should start sequence");
    let pending = match first {
        SequenceMatch::Pending(pending) => pending,
        other => panic!("expected pending after c, got {:?}", other),
    };

    let second = map
        .resolve_sequence_next(&pending, &w, context)
        .expect("c w should resolve");
    match second {
        SequenceMatch::Dispatch(resolved) => {
            assert_eq!(resolved.command, Command::ChangeWordForward);
        }
        other => panic!("expected dispatch for c w, got {:?}", other),
    }
}

#[test]
fn cb_sequence_maps_to_change_word_backward() {
    let map = make_map();
    let context = KeybindingContext::for_mode(EditorMode::Normal);
    let c = NormalizedInput {
        physical_key: Some(KeyCode::KeyC),
        named_key: None,
        text: Some("c".to_string()),
        modifiers: ModifiersState::empty(),
    };
    let b = NormalizedInput {
        physical_key: Some(KeyCode::KeyB),
        named_key: None,
        text: Some("b".to_string()),
        modifiers: ModifiersState::empty(),
    };

    let first = map
        .resolve_sequence_start(&c, context)
        .expect("first c should start sequence");
    let pending = match first {
        SequenceMatch::Pending(pending) => pending,
        other => panic!("expected pending after c, got {:?}", other),
    };

    let second = map
        .resolve_sequence_next(&pending, &b, context)
        .expect("c b should resolve");
    match second {
        SequenceMatch::Dispatch(resolved) => {
            assert_eq!(resolved.command, Command::ChangeWordBackward);
        }
        other => panic!("expected dispatch for c b, got {:?}", other),
    }
}
