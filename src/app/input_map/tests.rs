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
            name: "insert cmd+v -> EditorPaste",
            context: KeybindingContext::for_mode(EditorMode::Insert),
            input: NormalizedInput {
                physical_key: Some(KeyCode::KeyV),
                named_key: None,
                text: Some("v".to_string()),
                modifiers: ModifiersState::CONTROL,
            },
            expected: Some(Command::EditorPaste),
        },
        Case {
            name: "normal escape -> ClearSearchHighlights",
            context: KeybindingContext::for_mode(EditorMode::Normal),
            input: input_from_named(NamedKey::Escape),
            expected: Some(Command::ClearSearchHighlights),
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
            name: "terminal focus cmd+v -> TerminalPaste",
            context: KeybindingContext::with_focus(
                EditorMode::TerminalFocus,
                InputFocusContext::Terminal,
            ),
            input: NormalizedInput {
                physical_key: Some(KeyCode::KeyV),
                named_key: None,
                text: Some("v".to_string()),
                modifiers: ModifiersState::CONTROL,
            },
            expected: Some(Command::TerminalPaste),
        },
        Case {
            name: "terminal focus ctrl+h -> None (strict PTY input)",
            context: KeybindingContext::with_focus(
                EditorMode::TerminalFocus,
                InputFocusContext::Terminal,
            ),
            input: NormalizedInput {
                physical_key: Some(KeyCode::KeyH),
                named_key: None,
                text: Some("h".to_string()),
                modifiers: ModifiersState::CONTROL,
            },
            expected: None,
        },
        Case {
            name: "terminal focus ctrl+w -> None (global focus_back blocked)",
            context: KeybindingContext::with_focus(
                EditorMode::TerminalFocus,
                InputFocusContext::Terminal,
            ),
            input: NormalizedInput {
                physical_key: Some(KeyCode::KeyW),
                named_key: None,
                text: Some("w".to_string()),
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
            name: "terminal focus ctrl+q -> EnterTerminalNormal",
            context: KeybindingContext::with_focus(
                EditorMode::TerminalFocus,
                InputFocusContext::Terminal,
            ),
            input: NormalizedInput {
                physical_key: Some(KeyCode::KeyQ),
                named_key: None,
                text: Some("q".to_string()),
                modifiers: ModifiersState::CONTROL,
            },
            expected: Some(Command::SwitchMode(ModeEvent::EnterTerminalNormal)),
        },
        Case {
            name: "terminal normal j -> MoveDown",
            context: KeybindingContext::with_focus(
                EditorMode::TerminalNormal,
                InputFocusContext::Terminal,
            ),
            input: NormalizedInput {
                physical_key: Some(KeyCode::KeyJ),
                named_key: None,
                text: Some("j".to_string()),
                modifiers: ModifiersState::empty(),
            },
            expected: Some(Command::MoveDown),
        },
        Case {
            name: "terminal normal escape -> EnterTerminalFocus",
            context: KeybindingContext::with_focus(
                EditorMode::TerminalNormal,
                InputFocusContext::Terminal,
            ),
            input: input_from_named(NamedKey::Escape),
            expected: Some(Command::SwitchMode(ModeEvent::FocusTerminal)),
        },
        Case {
            name: "terminal normal cmd+v -> TerminalPaste",
            context: KeybindingContext::with_focus(
                EditorMode::TerminalNormal,
                InputFocusContext::Terminal,
            ),
            input: NormalizedInput {
                physical_key: Some(KeyCode::KeyV),
                named_key: None,
                text: Some("v".to_string()),
                modifiers: ModifiersState::CONTROL,
            },
            expected: Some(Command::TerminalPaste),
        },
        Case {
            name: "normal p -> PasteAfter",
            context: KeybindingContext::for_mode(EditorMode::Normal),
            input: NormalizedInput {
                physical_key: Some(KeyCode::KeyP),
                named_key: None,
                text: Some("p".to_string()),
                modifiers: ModifiersState::empty(),
            },
            expected: Some(Command::PasteAfter),
        },
        Case {
            name: "normal cmd+v -> EditorPaste",
            context: KeybindingContext::for_mode(EditorMode::Normal),
            input: NormalizedInput {
                physical_key: Some(KeyCode::KeyV),
                named_key: None,
                text: Some("v".to_string()),
                modifiers: ModifiersState::CONTROL,
            },
            expected: Some(Command::EditorPaste),
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
            name: "global F12 -> FocusTerminal",
            context: KeybindingContext::for_mode(EditorMode::Normal),
            input: input_from_named(NamedKey::F12),
            expected: Some(Command::FocusTerminal),
        },
        Case {
            name: "global cmd+backslash -> ToggleBottomDock",
            context: KeybindingContext::for_mode(EditorMode::Normal),
            input: NormalizedInput {
                physical_key: Some(KeyCode::Backslash),
                named_key: None,
                text: Some("\\".to_string()),
                modifiers: ModifiersState::CONTROL,
            },
            expected: Some(Command::ToggleBottomDock),
        },
        Case {
            name: "normal ctrl+l -> BufferNext",
            context: KeybindingContext::for_mode(EditorMode::Normal),
            input: NormalizedInput {
                physical_key: Some(KeyCode::KeyL),
                named_key: None,
                text: Some("l".to_string()),
                modifiers: ModifiersState::CONTROL,
            },
            expected: Some(Command::BufferNext),
        },
        Case {
            name: "normal ctrl+h -> BufferPrev",
            context: KeybindingContext::for_mode(EditorMode::Normal),
            input: NormalizedInput {
                physical_key: Some(KeyCode::KeyH),
                named_key: None,
                text: Some("h".to_string()),
                modifiers: ModifiersState::CONTROL,
            },
            expected: Some(Command::BufferPrev),
        },
        Case {
            name: "explorer w -> ExplorerCollapseOrParent",
            context: KeybindingContext::with_focus(EditorMode::Normal, InputFocusContext::Explorer),
            input: NormalizedInput {
                physical_key: Some(KeyCode::KeyW),
                named_key: None,
                text: Some("w".to_string()),
                modifiers: ModifiersState::empty(),
            },
            expected: Some(Command::ExplorerCollapseOrParent),
        },
        Case {
            name: "explorer W -> ExplorerCollapseAllUnderNode",
            context: KeybindingContext::with_focus(EditorMode::Normal, InputFocusContext::Explorer),
            input: NormalizedInput {
                physical_key: Some(KeyCode::KeyW),
                named_key: None,
                text: Some("W".to_string()),
                modifiers: ModifiersState::SHIFT,
            },
            expected: Some(Command::ExplorerCollapseAllUnderNode),
        },
        Case {
            name: "explorer d -> ExplorerDeleteNode",
            context: KeybindingContext::with_focus(EditorMode::Normal, InputFocusContext::Explorer),
            input: NormalizedInput {
                physical_key: Some(KeyCode::KeyD),
                named_key: None,
                text: Some("d".to_string()),
                modifiers: ModifiersState::empty(),
            },
            expected: Some(Command::ExplorerDeleteNode),
        },
        Case {
            name: "explorer e -> ExplorerExpandNode",
            context: KeybindingContext::with_focus(EditorMode::Normal, InputFocusContext::Explorer),
            input: NormalizedInput {
                physical_key: Some(KeyCode::KeyE),
                named_key: None,
                text: Some("e".to_string()),
                modifiers: ModifiersState::empty(),
            },
            expected: Some(Command::ExplorerExpandNode),
        },
        Case {
            name: "explorer E -> ExplorerExpandAllUnderNode",
            context: KeybindingContext::with_focus(EditorMode::Normal, InputFocusContext::Explorer),
            input: NormalizedInput {
                physical_key: Some(KeyCode::KeyE),
                named_key: None,
                text: Some("E".to_string()),
                modifiers: ModifiersState::SHIFT,
            },
            expected: Some(Command::ExplorerExpandAllUnderNode),
        },
        Case {
            name: "explorer f -> ExplorerStartFilter",
            context: KeybindingContext::with_focus(EditorMode::Normal, InputFocusContext::Explorer),
            input: NormalizedInput {
                physical_key: Some(KeyCode::KeyF),
                named_key: None,
                text: Some("f".to_string()),
                modifiers: ModifiersState::empty(),
            },
            expected: Some(Command::ExplorerStartFilter),
        },
        Case {
            name: "explorer F -> ExplorerClearFilter",
            context: KeybindingContext::with_focus(EditorMode::Normal, InputFocusContext::Explorer),
            input: NormalizedInput {
                physical_key: Some(KeyCode::KeyF),
                named_key: None,
                text: Some("F".to_string()),
                modifiers: ModifiersState::SHIFT,
            },
            expected: Some(Command::ExplorerClearFilter),
        },
        Case {
            name: "references arrow down -> ReferencesSelectNext",
            context: KeybindingContext::with_focus(
                EditorMode::Normal,
                InputFocusContext::References,
            ),
            input: input_from_named(NamedKey::ArrowDown),
            expected: Some(Command::ReferencesSelectNext),
        },
        Case {
            name: "references ctrl+p -> ReferencesSelectPrev",
            context: KeybindingContext::with_focus(
                EditorMode::Normal,
                InputFocusContext::References,
            ),
            input: NormalizedInput {
                physical_key: Some(KeyCode::KeyP),
                named_key: None,
                text: Some("p".to_string()),
                modifiers: ModifiersState::CONTROL,
            },
            expected: Some(Command::ReferencesSelectPrev),
        },
        Case {
            name: "references enter -> ReferencesOpenSelection",
            context: KeybindingContext::with_focus(
                EditorMode::Normal,
                InputFocusContext::References,
            ),
            input: input_from_named(NamedKey::Enter),
            expected: Some(Command::ReferencesOpenSelection),
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
            name: "visual y -> YankSelection",
            context: KeybindingContext::for_mode(EditorMode::Visual),
            input: NormalizedInput {
                physical_key: Some(KeyCode::KeyY),
                named_key: None,
                text: Some("y".to_string()),
                modifiers: ModifiersState::empty(),
            },
            expected: Some(Command::YankSelection),
        },
        Case {
            name: "visual cmd+v -> EditorPaste",
            context: KeybindingContext::for_mode(EditorMode::Visual),
            input: NormalizedInput {
                physical_key: Some(KeyCode::KeyV),
                named_key: None,
                text: Some("v".to_string()),
                modifiers: ModifiersState::CONTROL,
            },
            expected: Some(Command::EditorPaste),
        },
        Case {
            name: "palette cmd+v -> EditorPaste",
            context: KeybindingContext::for_mode_with_picker(EditorMode::PaletteFocus, true),
            input: NormalizedInput {
                physical_key: Some(KeyCode::KeyV),
                named_key: None,
                text: Some("v".to_string()),
                modifiers: ModifiersState::CONTROL,
            },
            expected: Some(Command::EditorPaste),
        },
        Case {
            name: "fuzzy picker insert cmd+v -> EditorPaste",
            context: KeybindingContext::with_focus(
                EditorMode::Insert,
                InputFocusContext::FuzzyPicker,
            ),
            input: NormalizedInput {
                physical_key: Some(KeyCode::KeyV),
                named_key: None,
                text: Some("v".to_string()),
                modifiers: ModifiersState::CONTROL,
            },
            expected: Some(Command::EditorPaste),
        },
        Case {
            name: "palette ctrl+n -> OverlaySelectNext",
            context: KeybindingContext::for_mode_with_picker(EditorMode::PaletteFocus, true),
            input: NormalizedInput {
                physical_key: Some(KeyCode::KeyN),
                named_key: None,
                text: Some("n".to_string()),
                modifiers: ModifiersState::CONTROL,
            },
            expected: Some(Command::OverlaySelectNext),
        },
        Case {
            name: "palette ctrl+p -> OverlaySelectPrev",
            context: KeybindingContext::for_mode_with_picker(EditorMode::PaletteFocus, true),
            input: NormalizedInput {
                physical_key: Some(KeyCode::KeyP),
                named_key: None,
                text: Some("p".to_string()),
                modifiers: ModifiersState::CONTROL,
            },
            expected: Some(Command::OverlaySelectPrev),
        },
        Case {
            name: "palette arrow down -> OverlaySelectNext",
            context: KeybindingContext::for_mode_with_picker(EditorMode::PaletteFocus, true),
            input: input_from_named(NamedKey::ArrowDown),
            expected: Some(Command::OverlaySelectNext),
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
fn f12_maps_to_focus_terminal_in_terminal_focus() {
    let map = make_map();
    let input = input_from_named(NamedKey::F12);
    assert_eq!(
        map.translate(
            &input,
            KeybindingContext::with_focus(EditorMode::TerminalFocus, InputFocusContext::Terminal)
        ),
        Some(Command::FocusTerminal)
    );
}

#[test]
fn bare_backtick_no_longer_maps_to_terminal_command() {
    let map = make_map();
    let input = NormalizedInput {
        physical_key: Some(KeyCode::Backquote),
        named_key: None,
        text: Some("`".to_string()),
        modifiers: ModifiersState::empty(),
    };
    assert_eq!(
        map.translate(&input, KeybindingContext::for_mode(EditorMode::Normal)),
        None
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
            assert_eq!(resolved.command, Command::OpenFilePicker);
        }
        other => panic!("expected dispatch for leader f f, got {:?}", other),
    }
}

#[test]
fn leader_f_w_sequence_maps_to_search_in_files() {
    let map = make_map();
    let context = KeybindingContext::for_mode(EditorMode::Normal);
    let space = input_from_named(NamedKey::Space);
    let first = map
        .resolve_sequence_start(&space, context)
        .expect("space should start chord");
    let pending = match first {
        SequenceMatch::Pending(pending) => pending,
        other => panic!("expected pending leader sequence, got {:?}", other),
    };

    let follow_f = NormalizedInput {
        physical_key: Some(KeyCode::KeyF),
        named_key: None,
        text: Some("f".to_string()),
        modifiers: ModifiersState::empty(),
    };
    let second = map
        .resolve_sequence_next(&pending, &follow_f, context)
        .expect("leader+f should still be pending");
    let pending = match second {
        SequenceMatch::Pending(pending) => pending,
        other => panic!("expected second pending sequence, got {:?}", other),
    };

    let follow_w = NormalizedInput {
        physical_key: Some(KeyCode::KeyW),
        named_key: None,
        text: Some("w".to_string()),
        modifiers: ModifiersState::empty(),
    };
    let resolved = map
        .resolve_sequence_next(&pending, &follow_w, context)
        .expect("leader+f+w should resolve");
    match resolved {
        SequenceMatch::Dispatch(resolved) => {
            assert_eq!(resolved.command, Command::SearchInFiles);
        }
        other => panic!("expected dispatch for leader f w, got {:?}", other),
    }
}

#[test]
fn fuzzy_picker_insert_text_appends_query() {
    let map = make_map();
    let resolved = map.resolve(
        &NormalizedInput {
            physical_key: Some(KeyCode::KeyA),
            named_key: None,
            text: Some("a".to_string()),
            modifiers: ModifiersState::empty(),
        },
        KeybindingContext::with_focus(EditorMode::Insert, InputFocusContext::FuzzyPicker),
    );

    assert_eq!(
        resolved.map(|matched| matched.command),
        Some(Command::FilePickerAppendQuery("a".to_string()))
    );
}

#[test]
fn fuzzy_picker_normal_q_closes_buffer() {
    let map = make_map();
    let resolved = map.resolve(
        &NormalizedInput {
            physical_key: Some(KeyCode::KeyQ),
            named_key: None,
            text: Some("q".to_string()),
            modifiers: ModifiersState::empty(),
        },
        KeybindingContext::with_focus(EditorMode::Normal, InputFocusContext::FuzzyPicker),
    );

    assert_eq!(
        resolved.map(|matched| matched.command),
        Some(Command::BufferCloseCurrent)
    );
}

#[test]
fn fuzzy_picker_insert_ctrl_n_selects_next() {
    let map = make_map();
    let resolved = map.resolve(
        &NormalizedInput {
            physical_key: Some(KeyCode::KeyN),
            named_key: None,
            text: Some("n".to_string()),
            modifiers: ModifiersState::CONTROL,
        },
        KeybindingContext::with_focus(EditorMode::Insert, InputFocusContext::FuzzyPicker),
    );

    assert_eq!(
        resolved.map(|matched| matched.command),
        Some(Command::OverlaySelectNext)
    );
}

#[test]
fn fuzzy_picker_normal_j_selects_next() {
    let map = make_map();
    let resolved = map.resolve(
        &NormalizedInput {
            physical_key: Some(KeyCode::KeyJ),
            named_key: None,
            text: Some("j".to_string()),
            modifiers: ModifiersState::empty(),
        },
        KeybindingContext::with_focus(EditorMode::Normal, InputFocusContext::FuzzyPicker),
    );

    assert_eq!(
        resolved.map(|matched| matched.command),
        Some(Command::OverlaySelectNext)
    );
}

#[test]
fn leader_space_x_maps_to_close_current_buffer() {
    let map = make_map();
    let context = KeybindingContext::for_mode(EditorMode::Normal);
    let space = input_from_named(NamedKey::Space);
    let first = map
        .resolve_sequence_start(&space, context)
        .expect("space should start chord");
    let pending = match first {
        SequenceMatch::Pending(pending) => pending,
        other => panic!("expected pending leader sequence, got {:?}", other),
    };

    let follow_x = NormalizedInput {
        physical_key: Some(KeyCode::KeyX),
        named_key: None,
        text: Some("x".to_string()),
        modifiers: ModifiersState::empty(),
    };
    let resolved = map
        .resolve_sequence_next(&pending, &follow_x, context)
        .expect("leader+x should resolve");
    match resolved {
        SequenceMatch::Dispatch(resolved) => {
            assert_eq!(resolved.command, Command::BufferCloseCurrent);
        }
        other => panic!("expected dispatch for leader x, got {:?}", other),
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
