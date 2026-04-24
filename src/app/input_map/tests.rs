use std::path::PathBuf;

use winit::keyboard::{KeyCode, ModifiersState, NamedKey};

use crate::{
    app::{
        command_palette::CommandPaletteMode,
        resolved_keymap::{build, builtin_defaults},
    },
    config::keymap_loader::KeymapLoader,
    core::{
        commands::Command,
        mode::{EditorMode, ModeEvent},
    },
};

use super::{InputFocusContext, InputMap, KeybindingContext, NormalizedInput, SequenceMatch};

fn make_map() -> InputMap {
    InputMap::with_keymap(PathBuf::from("test_open.txt"), builtin_defaults())
}

fn make_default_profile_map() -> InputMap {
    let bindings = KeymapLoader::load("default", None);
    InputMap::with_keymap(PathBuf::from("test_open.txt"), build(&bindings))
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
            name: "normal Shift+C -> ChangeToLineEnd",
            context: KeybindingContext::for_mode(EditorMode::Normal),
            input: NormalizedInput {
                physical_key: Some(KeyCode::KeyC),
                named_key: None,
                text: Some("C".to_string()),
                modifiers: ModifiersState::SHIFT,
            },
            expected: Some(Command::ChangeToLineEnd),
        },
        Case {
            name: "normal Shift+D -> DeleteToLineEnd",
            context: KeybindingContext::for_mode(EditorMode::Normal),
            input: NormalizedInput {
                physical_key: Some(KeyCode::KeyD),
                named_key: None,
                text: Some("D".to_string()),
                modifiers: ModifiersState::SHIFT,
            },
            expected: Some(Command::DeleteToLineEnd),
        },
        Case {
            name: "normal Shift+J -> JoinLines",
            context: KeybindingContext::for_mode(EditorMode::Normal),
            input: NormalizedInput {
                physical_key: Some(KeyCode::KeyJ),
                named_key: None,
                text: Some("J".to_string()),
                modifiers: ModifiersState::SHIFT,
            },
            expected: Some(Command::JoinLines),
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
            name: "normal ctrl+d -> ScrollHalfPageDown",
            context: KeybindingContext::for_mode(EditorMode::Normal),
            input: NormalizedInput {
                physical_key: Some(KeyCode::KeyD),
                named_key: None,
                text: Some("d".to_string()),
                modifiers: ModifiersState::CONTROL,
            },
            expected: Some(Command::ScrollHalfPageDown),
        },
        Case {
            name: "normal { -> MoveParagraphUp",
            context: KeybindingContext::for_mode(EditorMode::Normal),
            input: NormalizedInput {
                physical_key: Some(KeyCode::BracketLeft),
                named_key: None,
                text: Some("{".to_string()),
                modifiers: ModifiersState::SHIFT,
            },
            expected: Some(Command::MoveParagraphUp),
        },
        Case {
            name: "normal } -> MoveParagraphDown",
            context: KeybindingContext::for_mode(EditorMode::Normal),
            input: NormalizedInput {
                physical_key: Some(KeyCode::BracketRight),
                named_key: None,
                text: Some("}".to_string()),
                modifiers: ModifiersState::SHIFT,
            },
            expected: Some(Command::MoveParagraphDown),
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
fn recent_projects_jk_only_work_from_welcome_context() {
    let map = make_default_profile_map();
    let input_j = NormalizedInput {
        physical_key: Some(KeyCode::KeyJ),
        named_key: None,
        text: Some("j".to_string()),
        modifiers: ModifiersState::empty(),
    };
    let input_ctrl_n = NormalizedInput {
        physical_key: Some(KeyCode::KeyN),
        named_key: None,
        text: Some("n".to_string()),
        modifiers: ModifiersState::CONTROL,
    };

    let mut normal_palette_context =
        KeybindingContext::for_mode_with_picker(EditorMode::PaletteFocus, true);
    normal_palette_context.command_palette_mode = Some(CommandPaletteMode::RecentProjects);

    assert_eq!(
        map.translate(&input_j, normal_palette_context),
        Some(Command::FilePickerAppendQuery("j".to_string()))
    );
    assert_eq!(
        map.translate(&input_ctrl_n, normal_palette_context),
        Some(Command::OverlaySelectNext)
    );

    let mut welcome_palette_context = normal_palette_context;
    welcome_palette_context.welcome_visible = true;

    assert_eq!(
        map.translate(&input_j, welcome_palette_context),
        Some(Command::OverlaySelectNext)
    );
    assert_eq!(
        map.translate(&input_ctrl_n, welcome_palette_context),
        Some(Command::OverlaySelectNext)
    );
}

#[test]
fn welcome_palette_modes_support_ctrl_np_navigation() {
    let map = make_default_profile_map();
    let input_ctrl_n = NormalizedInput {
        physical_key: Some(KeyCode::KeyN),
        named_key: None,
        text: Some("n".to_string()),
        modifiers: ModifiersState::CONTROL,
    };
    let input_ctrl_p = NormalizedInput {
        physical_key: Some(KeyCode::KeyP),
        named_key: None,
        text: Some("p".to_string()),
        modifiers: ModifiersState::CONTROL,
    };

    for mode in [
        CommandPaletteMode::FilePicker,
        CommandPaletteMode::ThemeSelector,
        CommandPaletteMode::CommandPalette,
    ] {
        let mut context = KeybindingContext::for_mode_with_picker(EditorMode::PaletteFocus, true);
        context.command_palette_mode = Some(mode);
        context.welcome_visible = true;

        assert_eq!(
            map.translate(&input_ctrl_n, context),
            Some(Command::OverlaySelectNext),
            "Ctrl+n should select next in {mode:?} on welcome page"
        );
        assert_eq!(
            map.translate(&input_ctrl_p, context),
            Some(Command::OverlaySelectPrev),
            "Ctrl+p should select previous in {mode:?} on welcome page"
        );
    }
}

#[test]
fn welcome_recent_projects_use_direct_jk_enter() {
    let map = make_default_profile_map();
    let input_j = NormalizedInput {
        physical_key: Some(KeyCode::KeyJ),
        named_key: None,
        text: Some("j".to_string()),
        modifiers: ModifiersState::empty(),
    };
    let input_k = NormalizedInput {
        physical_key: Some(KeyCode::KeyK),
        named_key: None,
        text: Some("k".to_string()),
        modifiers: ModifiersState::empty(),
    };
    let input_enter = input_from_named(NamedKey::Enter);
    let input_ctrl_n = NormalizedInput {
        physical_key: Some(KeyCode::KeyN),
        named_key: None,
        text: Some("n".to_string()),
        modifiers: ModifiersState::CONTROL,
    };
    let input_ctrl_p = NormalizedInput {
        physical_key: Some(KeyCode::KeyP),
        named_key: None,
        text: Some("p".to_string()),
        modifiers: ModifiersState::CONTROL,
    };

    let mut context = KeybindingContext::for_mode(EditorMode::Insert);
    context.welcome_visible = true;
    context.focus = InputFocusContext::Welcome;

    assert_eq!(
        map.translate(&input_j, context),
        Some(Command::OverlaySelectNext)
    );
    assert_eq!(
        map.translate(&input_k, context),
        Some(Command::OverlaySelectPrev)
    );
    assert_eq!(
        map.translate(&input_enter, context),
        Some(Command::FilePickerConfirmSelection)
    );
    assert_eq!(
        map.translate(&input_ctrl_n, context),
        Some(Command::OverlaySelectNext)
    );
    assert_eq!(
        map.translate(&input_ctrl_p, context),
        Some(Command::OverlaySelectPrev)
    );
}

#[test]
fn welcome_recent_projects_jk_are_not_taken_by_sidebar_focus() {
    let map = make_default_profile_map();
    let input_j = NormalizedInput {
        physical_key: Some(KeyCode::KeyJ),
        named_key: None,
        text: Some("j".to_string()),
        modifiers: ModifiersState::empty(),
    };
    let input_k = NormalizedInput {
        physical_key: Some(KeyCode::KeyK),
        named_key: None,
        text: Some("k".to_string()),
        modifiers: ModifiersState::empty(),
    };

    let mut context = KeybindingContext::for_mode(EditorMode::Insert);
    context.welcome_visible = true;
    context.focus = InputFocusContext::Welcome;

    assert_eq!(
        map.translate(&input_j, context),
        Some(Command::OverlaySelectNext)
    );
    assert_eq!(
        map.translate(&input_k, context),
        Some(Command::OverlaySelectPrev)
    );
}

#[test]
fn welcome_context_can_open_recent_projects_with_leader_sequence() {
    let map = make_default_profile_map();
    let input_space = input_from_named(NamedKey::Space);
    let input_p = NormalizedInput {
        physical_key: Some(KeyCode::KeyP),
        named_key: None,
        text: Some("p".to_string()),
        modifiers: ModifiersState::empty(),
    };
    let input_j = NormalizedInput {
        physical_key: Some(KeyCode::KeyJ),
        named_key: None,
        text: Some("j".to_string()),
        modifiers: ModifiersState::empty(),
    };

    let mut context = KeybindingContext::for_mode(EditorMode::Insert);
    context.welcome_visible = true;

    let SequenceMatch::Pending(sequence) = map
        .resolve_sequence_start(&input_space, context)
        .expect("welcome leader should start a sequence even outside normal mode")
    else {
        panic!("welcome leader should be pending");
    };
    let SequenceMatch::Pending(sequence) = map
        .resolve_sequence_next(&sequence, &input_p, context)
        .expect("welcome leader p should remain pending")
    else {
        panic!("welcome leader p should be pending");
    };
    let SequenceMatch::Dispatch(matched) = map
        .resolve_sequence_next(&sequence, &input_j, context)
        .expect("welcome leader p j should dispatch recent projects")
    else {
        panic!("welcome leader p j should dispatch");
    };

    assert_eq!(matched.command, Command::OpenRecentProjects);
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
fn leader_d_s_maps_to_diagnostics_open_picker() {
    let map = make_default_profile_map();
    let context = KeybindingContext::for_mode(EditorMode::Normal);
    let space = input_from_named(NamedKey::Space);
    let first = map
        .resolve_sequence_start(&space, context)
        .expect("space should start chord");
    let pending = match first {
        SequenceMatch::Pending(pending) => pending,
        other => panic!("expected pending leader sequence, got {:?}", other),
    };

    let follow_d = NormalizedInput {
        physical_key: Some(KeyCode::KeyD),
        named_key: None,
        text: Some("d".to_string()),
        modifiers: ModifiersState::empty(),
    };
    let second = map
        .resolve_sequence_next(&pending, &follow_d, context)
        .expect("leader+d should still be pending");
    let pending = match second {
        SequenceMatch::Pending(pending) => pending,
        other => panic!("expected second pending sequence, got {:?}", other),
    };

    let follow_s = NormalizedInput {
        physical_key: Some(KeyCode::KeyS),
        named_key: None,
        text: Some("s".to_string()),
        modifiers: ModifiersState::empty(),
    };
    let resolved = map
        .resolve_sequence_next(&pending, &follow_s, context)
        .expect("leader+d+s should resolve");
    match resolved {
        SequenceMatch::Dispatch(resolved) => {
            assert_eq!(resolved.command, Command::DiagnosticsOpenPicker);
        }
        other => panic!("expected dispatch for leader d s, got {:?}", other),
    }
}

#[test]
fn leader_sequence_is_not_started_in_insert_mode() {
    let map = make_map();
    let start_input = input_from_named(NamedKey::Space);
    let context = KeybindingContext::for_mode(EditorMode::Insert);
    assert!(map.resolve_sequence_start(&start_input, context).is_none());
}

