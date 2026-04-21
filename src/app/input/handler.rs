use std::time::{Duration, Instant};

use winit::{
    event::{ElementState, KeyEvent, Modifiers},
    keyboard::{ModifiersState, NamedKey},
};

use crate::{
    app::input_map::{InputFocusContext, InputMap, KeybindingContext, SequenceMatch},
    core::{commands::Command, mode::EditorMode},
};

use super::{
    helpers::{
        bracket_chars_from_input, inner_or_around_from_input, is_modifier_only_key,
        numeric_count_digit_from_input, printable_char_from_input, replace_char_from_input,
        should_start_replace_pending, should_start_yank_pending, terminal_input_payload,
    },
    model::{InputRouteOutcome, NormalizedInput, TranslatedInput},
    pending::{PendingInput, PendingState, TextObjectOp, classify_pending_state},
};

/// InputHandler là lớp cầu nối:
/// winit key event -> normalized input -> command.
pub struct InputHandler {
    modifiers: ModifiersState,
    prefix_timeout: Duration,
    pending_input: Option<PendingInput>,
    pending_count: Option<usize>,
    operator_count: Option<usize>,
}

impl InputHandler {
    pub fn new() -> Self {
        Self {
            modifiers: ModifiersState::empty(),
            prefix_timeout: Duration::from_millis(1_500),
            pending_input: None,
            pending_count: None,
            operator_count: None,
        }
    }

    pub fn update_modifiers(&mut self, modifiers: Modifiers) {
        self.modifiers = modifiers.state();
    }

    pub fn on_focus_changed(&mut self, focused: bool) {
        if !focused {
            self.reset_prefix();
        }
    }

    /// Drop any pending prefix sequence when focus surface changes.
    pub fn clear_pending_prefix(&mut self) {
        self.reset_prefix();
    }

    pub fn current_modifiers(&self) -> ModifiersState {
        self.modifiers
    }

    pub fn get_pending_keys(&self) -> String {
        let pending_state = self.pending_input.as_ref().map(|pending| &pending.state);
        let sequence = self
            .pending_input
            .as_ref()
            .and_then(|pending| pending.sequence.as_ref())
            .map(|sequence| {
                sequence
                    .steps
                    .iter()
                    .map(crate::app::resolved_keymap::KeySpec::display_token)
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default();
        let mut parts = Vec::new();

        if let Some(operator_count) = self.operator_count {
            parts.push(operator_count.to_string());
        } else if let Some(pending_count) = self.pending_count
            && !pending_state.is_some_and(PendingState::uses_operator_count)
        {
            parts.push(pending_count.to_string());
        }

        if !sequence.is_empty() {
            parts.push(sequence);
        }

        if pending_state.is_some_and(PendingState::uses_operator_count)
            && let Some(pending_count) = self.pending_count
        {
            parts.push(pending_count.to_string());
        }

        parts.join(" ")
    }

    fn accumulate_pending_count(&mut self, digit: usize) {
        let next = self
            .pending_count
            .unwrap_or(0)
            .saturating_mul(10)
            .saturating_add(digit);
        self.pending_count = Some(next.max(1));
    }

    fn pending_count_label(&self) -> String {
        match (self.operator_count, self.pending_count) {
            (Some(operator_count), Some(motion_count)) => {
                format!("{operator_count} x {motion_count}")
            }
            (Some(operator_count), None) => operator_count.to_string(),
            (None, Some(pending_count)) => pending_count.to_string(),
            (None, None) => "1".to_string(),
        }
    }

    fn clear_pending_counts(&mut self) {
        self.pending_count = None;
        self.operator_count = None;
    }

    fn clear_pending_input(&mut self) {
        self.pending_input = None;
    }

    fn consume_repeat_count_for_command(
        &mut self,
        command: &Command,
        pending_state: Option<&PendingState>,
    ) -> (usize, bool) {
        let had_count = self.pending_count.is_some() || self.operator_count.is_some();
        let raw_count = if pending_state.is_some_and(PendingState::uses_operator_count)
            || self.operator_count.is_some()
        {
            self.operator_count
                .unwrap_or(1)
                .saturating_mul(self.pending_count.unwrap_or(1))
        } else {
            self.pending_count.unwrap_or(1)
        };
        self.clear_pending_counts();

        if command.supports_numeric_count() {
            (raw_count.max(1), false)
        } else {
            (1, had_count)
        }
    }

    fn translate_dispatch(
        input_debug: String,
        route_debug: String,
        command: Command,
        repeat_count: usize,
        count_ignored: bool,
    ) -> TranslatedInput {
        let route_debug = if repeat_count > 1 {
            format!("{route_debug} (count={repeat_count})")
        } else if count_ignored {
            format!("{route_debug} (count ignored)")
        } else {
            route_debug
        };

        TranslatedInput {
            input_debug,
            route_debug,
            command,
            repeat_count,
        }
    }

    pub fn translate_key_event(
        &mut self,
        key_event: &KeyEvent,
        input_map: &InputMap,
        context: KeybindingContext,
    ) -> Option<InputRouteOutcome> {
        if key_event.state != ElementState::Pressed || key_event.repeat {
            return None;
        }

        let normalized = NormalizedInput::from_key_event(key_event, self.modifiers);
        self.route_normalized_input(normalized, input_map, context, Instant::now())
    }

    // crate-visible for clock-injected router tests.
    pub(crate) fn route_normalized_input(
        &mut self,
        normalized: NormalizedInput,
        input_map: &InputMap,
        context: KeybindingContext,
        now: Instant,
    ) -> Option<InputRouteOutcome> {
        let input_debug = normalized.debug_label();
        self.reset_prefix_if_timed_out(now);

        if let Some(pending) = self.pending_input.clone() {
            if is_modifier_only_key(&normalized) {
                self.pending_input = Some(PendingInput {
                    started_at: now,
                    ..pending.clone()
                });
                return Some(InputRouteOutcome::NoDispatch {
                    input_debug,
                    route_debug: format!(
                        "mode={} focus={} -> modifier key pass-through (state={})",
                        context.mode.as_str(),
                        context.focus.as_str(),
                        pending.state.as_str(),
                    ),
                });
            }

            if matches!(pending.state, PendingState::LeapChar) {
                self.reset_prefix();
                if normalized.named_key == Some(NamedKey::Escape) {
                    return Some(InputRouteOutcome::Dispatch(Self::translate_dispatch(
                        input_debug,
                        format!(
                            "mode={} focus={} -> leap char: ESC cancel",
                            context.mode.as_str(),
                            context.focus.as_str(),
                        ),
                        Command::LeapCancel,
                        1,
                        false,
                    )));
                }
                if let Some(ch) = printable_char_from_input(&normalized) {
                    return Some(InputRouteOutcome::Dispatch(Self::translate_dispatch(
                        input_debug,
                        format!(
                            "mode={} focus={} -> leap activate target {:?}",
                            context.mode.as_str(),
                            context.focus.as_str(),
                            ch,
                        ),
                        Command::LeapActivate(ch),
                        1,
                        false,
                    )));
                }
                if normalized.named_key == Some(NamedKey::Space) {
                    return Some(InputRouteOutcome::Dispatch(Self::translate_dispatch(
                        input_debug,
                        format!(
                            "mode={} focus={} -> leap activate target ' '",
                            context.mode.as_str(),
                            context.focus.as_str(),
                        ),
                        Command::LeapActivate(' '),
                        1,
                        false,
                    )));
                }
                return Some(InputRouteOutcome::Dispatch(Self::translate_dispatch(
                    input_debug,
                    format!(
                        "mode={} focus={} -> leap char: non-printable, cancel",
                        context.mode.as_str(),
                        context.focus.as_str(),
                    ),
                    Command::LeapCancel,
                    1,
                    false,
                )));
            }

            if matches!(pending.state, PendingState::LeapLabel) {
                self.reset_prefix();
                if normalized.named_key == Some(NamedKey::Escape) {
                    return Some(InputRouteOutcome::Dispatch(Self::translate_dispatch(
                        input_debug,
                        format!(
                            "mode={} focus={} -> leap label: ESC cancel",
                            context.mode.as_str(),
                            context.focus.as_str(),
                        ),
                        Command::LeapCancel,
                        1,
                        false,
                    )));
                }
                if let Some(ch) = printable_char_from_input(&normalized) {
                    return Some(InputRouteOutcome::Dispatch(Self::translate_dispatch(
                        input_debug,
                        format!(
                            "mode={} focus={} -> leap jump label {:?}",
                            context.mode.as_str(),
                            context.focus.as_str(),
                            ch,
                        ),
                        Command::LeapJump(ch),
                        1,
                        false,
                    )));
                }
                return Some(InputRouteOutcome::Dispatch(Self::translate_dispatch(
                    input_debug,
                    format!(
                        "mode={} focus={} -> leap label: non-printable, cancel",
                        context.mode.as_str(),
                        context.focus.as_str(),
                    ),
                    Command::LeapCancel,
                    1,
                    false,
                )));
            }

            if let Some(digit) =
                numeric_count_digit_from_input(&normalized, context, Some(&pending.state))
            {
                self.accumulate_pending_count(digit);
                self.pending_input = Some(PendingInput {
                    started_at: now,
                    ..pending.clone()
                });
                return Some(InputRouteOutcome::NoDispatch {
                    input_debug,
                    route_debug: format!(
                        "mode={} focus={} -> pending count {}",
                        context.mode.as_str(),
                        context.focus.as_str(),
                        self.pending_count_label(),
                    ),
                });
            }

            if normalized.named_key == Some(NamedKey::Escape) {
                self.reset_prefix();
                return Some(InputRouteOutcome::NoDispatch {
                    input_debug,
                    route_debug: format!(
                        "mode={} focus={} -> chord cancelled by Esc",
                        context.mode.as_str(),
                        context.focus.as_str(),
                    ),
                });
            }

            if matches!(pending.state, PendingState::ReplaceChar) {
                if let Some(ch) = replace_char_from_input(&normalized) {
                    self.reset_prefix();
                    return Some(InputRouteOutcome::Dispatch(Self::translate_dispatch(
                        input_debug,
                        format!(
                            "mode={} focus={} -> pending replace char commit ({ch:?})",
                            context.mode.as_str(),
                            context.focus.as_str(),
                        ),
                        Command::ReplaceChar(ch),
                        1,
                        false,
                    )));
                }
                self.pending_input = Some(PendingInput {
                    started_at: now,
                    ..pending
                });
                return Some(InputRouteOutcome::NoDispatch {
                    input_debug,
                    route_debug: format!(
                        "mode={} focus={} -> pending replace char (waiting printable key)",
                        context.mode.as_str(),
                        context.focus.as_str(),
                    ),
                });
            }

            match &pending.state {
                PendingState::VisualTextObjectModifier { inner } => {
                    let inner = *inner;
                    if let Some((open, close)) = bracket_chars_from_input(&normalized) {
                        self.clear_pending_input();
                        let command = Command::SelectTextObject {
                            open_char: open,
                            close_char: close,
                            inner,
                        };
                        let (repeat_count, count_ignored) =
                            self.consume_repeat_count_for_command(&command, Some(&pending.state));
                        return Some(InputRouteOutcome::Dispatch(Self::translate_dispatch(
                            input_debug,
                            format!(
                                "mode={} focus={} -> visual text object {open}{close} {}",
                                context.mode.as_str(),
                                context.focus.as_str(),
                                if inner { "inner" } else { "around" }
                            ),
                            command,
                            repeat_count,
                            count_ignored,
                        )));
                    }
                    self.reset_prefix();
                    if let Some(resolved) = input_map.resolve(&normalized, context) {
                        let (repeat_count, count_ignored) =
                            self.consume_repeat_count_for_command(&resolved.command, None);
                        return Some(InputRouteOutcome::Dispatch(Self::translate_dispatch(
                            input_debug,
                            format!(
                                "mode={} focus={} -> visual text object: not bracket, fallback -> {}",
                                context.mode.as_str(),
                                context.focus.as_str(),
                                resolved.reason
                            ),
                            resolved.command,
                            repeat_count,
                            count_ignored,
                        )));
                    }
                    return Some(InputRouteOutcome::NoDispatch {
                        input_debug,
                        route_debug: format!(
                            "mode={} focus={} -> visual text object: not a bracket, no fallback",
                            context.mode.as_str(),
                            context.focus.as_str()
                        ),
                    });
                }

                PendingState::OperatorWithObject { op, inner } => {
                    let op = *op;
                    let inner = *inner;
                    if let Some((open, close)) = bracket_chars_from_input(&normalized) {
                        let command = match op {
                            TextObjectOp::Delete => Command::DeleteTextObject {
                                open_char: open,
                                close_char: close,
                                inner,
                            },
                            TextObjectOp::Change => Command::ChangeTextObject {
                                open_char: open,
                                close_char: close,
                                inner,
                            },
                            TextObjectOp::Yank => Command::YankTextObject {
                                open_char: open,
                                close_char: close,
                                inner,
                            },
                        };
                        self.clear_pending_input();
                        let (repeat_count, count_ignored) =
                            self.consume_repeat_count_for_command(&command, Some(&pending.state));
                        return Some(InputRouteOutcome::Dispatch(Self::translate_dispatch(
                            input_debug,
                            format!(
                                "mode={} focus={} -> text object {open}{close} {} ({:?})",
                                context.mode.as_str(),
                                context.focus.as_str(),
                                if inner { "inner" } else { "around" },
                                op
                            ),
                            command,
                            repeat_count,
                            count_ignored,
                        )));
                    }
                    self.reset_prefix();
                    return Some(InputRouteOutcome::NoDispatch {
                        input_debug,
                        route_debug: format!(
                            "mode={} focus={} -> operator+object: not a bracket, reset",
                            context.mode.as_str(),
                            context.focus.as_str()
                        ),
                    });
                }

                PendingState::OperatorDelete
                | PendingState::OperatorChange
                | PendingState::OperatorYank => {
                    if let Some(inner) = inner_or_around_from_input(&normalized) {
                        let op = match &pending.state {
                            PendingState::OperatorDelete => TextObjectOp::Delete,
                            PendingState::OperatorChange => TextObjectOp::Change,
                            PendingState::OperatorYank => TextObjectOp::Yank,
                            _ => unreachable!(),
                        };
                        self.pending_input = Some(PendingInput {
                            state: PendingState::OperatorWithObject { op, inner },
                            sequence: None,
                            started_at: now,
                        });
                        return Some(InputRouteOutcome::NoDispatch {
                            input_debug,
                            route_debug: format!(
                                "mode={} focus={} -> {}: got modifier '{}', waiting bracket",
                                context.mode.as_str(),
                                context.focus.as_str(),
                                pending.state.as_str(),
                                if inner { "i" } else { "a" }
                            ),
                        });
                    }
                }

                _ => {}
            }

            let Some(sequence) = pending.sequence.as_ref() else {
                self.reset_prefix();
                return Some(InputRouteOutcome::NoDispatch {
                    input_debug,
                    route_debug: format!(
                        "mode={} focus={} -> invalid pending state reset",
                        context.mode.as_str(),
                        context.focus.as_str(),
                    ),
                });
            };

            if let Some(matched) = input_map.resolve_sequence_next(sequence, &normalized, context) {
                match matched {
                    SequenceMatch::Dispatch(resolved) => {
                        let pending_state = pending.state.clone();
                        self.clear_pending_input();
                        let (repeat_count, count_ignored) = self.consume_repeat_count_for_command(
                            &resolved.command,
                            Some(&pending_state),
                        );
                        return Some(InputRouteOutcome::Dispatch(Self::translate_dispatch(
                            input_debug,
                            format!(
                                "mode={} focus={} -> {}",
                                context.mode.as_str(),
                                context.focus.as_str(),
                                resolved.reason
                            ),
                            resolved.command,
                            repeat_count,
                            count_ignored,
                        )));
                    }
                    SequenceMatch::Pending(next_sequence) => {
                        let next_state = classify_pending_state(&next_sequence);
                        let step_count = next_sequence.steps.len();
                        self.pending_input = Some(PendingInput {
                            state: next_state.clone(),
                            sequence: Some(next_sequence),
                            started_at: now,
                        });
                        return Some(InputRouteOutcome::NoDispatch {
                            input_debug,
                            route_debug: format!(
                                "mode={} focus={} -> {} ({} step(s))",
                                context.mode.as_str(),
                                context.focus.as_str(),
                                next_state.as_str(),
                                step_count
                            ),
                        });
                    }
                }
            }

            self.reset_prefix();
            if let Some(resolved) = input_map.resolve(&normalized, context) {
                return Some(InputRouteOutcome::Dispatch(Self::translate_dispatch(
                    input_debug,
                    format!(
                        "mode={} focus={} -> chord interrupted, fallback -> {}",
                        context.mode.as_str(),
                        context.focus.as_str(),
                        resolved.reason
                    ),
                    resolved.command,
                    1,
                    false,
                )));
            }

            return Some(InputRouteOutcome::NoDispatch {
                input_debug,
                route_debug: format!(
                    "mode={} focus={} -> chord interrupted and no fallback mapping",
                    context.mode.as_str(),
                    context.focus.as_str()
                ),
            });
        }

        if let Some(digit) = numeric_count_digit_from_input(&normalized, context, None) {
            self.accumulate_pending_count(digit);
            return Some(InputRouteOutcome::NoDispatch {
                input_debug,
                route_debug: format!(
                    "mode={} focus={} -> pending count {}",
                    context.mode.as_str(),
                    context.focus.as_str(),
                    self.pending_count_label(),
                ),
            });
        }

        if should_start_replace_pending(&normalized, context) {
            self.pending_input = Some(PendingInput {
                sequence: None,
                state: PendingState::ReplaceChar,
                started_at: now,
            });
            return Some(InputRouteOutcome::NoDispatch {
                input_debug,
                route_debug: format!(
                    "mode={} focus={} -> pending replace char start",
                    context.mode.as_str(),
                    context.focus.as_str(),
                ),
            });
        }

        if should_start_yank_pending(&normalized, context) {
            if self.operator_count.is_none() {
                self.operator_count = self.pending_count.take();
            }
            self.pending_input = Some(PendingInput {
                sequence: None,
                state: PendingState::OperatorYank,
                started_at: now,
            });
            return Some(InputRouteOutcome::NoDispatch {
                input_debug,
                route_debug: format!(
                    "mode={} focus={} -> pending operator yank start",
                    context.mode.as_str(),
                    context.focus.as_str(),
                ),
            });
        }

        if context.focus == InputFocusContext::Editor && context.mode == EditorMode::Visual {
            if let Some(inner) = inner_or_around_from_input(&normalized) {
                self.pending_input = Some(PendingInput {
                    sequence: None,
                    state: PendingState::VisualTextObjectModifier { inner },
                    started_at: now,
                });
                return Some(InputRouteOutcome::NoDispatch {
                    input_debug,
                    route_debug: format!(
                        "mode={} focus={} -> visual text object modifier '{}' start, waiting bracket",
                        context.mode.as_str(),
                        context.focus.as_str(),
                        if inner { "i" } else { "a" }
                    ),
                });
            }
        }

        if let Some(matched) = input_map.resolve_sequence_start(&normalized, context) {
            match matched {
                SequenceMatch::Dispatch(resolved) => {
                    let (repeat_count, count_ignored) =
                        self.consume_repeat_count_for_command(&resolved.command, None);
                    return Some(InputRouteOutcome::Dispatch(Self::translate_dispatch(
                        input_debug,
                        format!(
                            "mode={} focus={} -> {}",
                            context.mode.as_str(),
                            context.focus.as_str(),
                            resolved.reason
                        ),
                        resolved.command,
                        repeat_count,
                        count_ignored,
                    )));
                }
                SequenceMatch::Pending(sequence) => {
                    if let Some(resolved) = input_map.resolve(&normalized, context) {
                        self.reset_prefix();
                        return Some(InputRouteOutcome::Dispatch(Self::translate_dispatch(
                            input_debug,
                            format!(
                                "mode={} focus={} -> single-key mapping preferred over chord prefix -> {}",
                                context.mode.as_str(),
                                context.focus.as_str(),
                                resolved.reason
                            ),
                            resolved.command,
                            1,
                            false,
                        )));
                    }

                    let next_state = classify_pending_state(&sequence);
                    if next_state.uses_operator_count() && self.operator_count.is_none() {
                        self.operator_count = self.pending_count.take();
                    }
                    self.pending_input = Some(PendingInput {
                        state: next_state,
                        sequence: Some(sequence),
                        started_at: now,
                    });
                    return Some(InputRouteOutcome::NoDispatch {
                        input_debug,
                        route_debug: format!(
                            "mode={} focus={} -> {} start",
                            context.mode.as_str(),
                            context.focus.as_str(),
                            self.pending_input
                                .as_ref()
                                .map(|pending| pending.state.as_str())
                                .unwrap_or("pending sequence")
                        ),
                    });
                }
            }
        }

        if let Some(resolved) = input_map.resolve(&normalized, context) {
            let (repeat_count, count_ignored) =
                self.consume_repeat_count_for_command(&resolved.command, None);
            return Some(InputRouteOutcome::Dispatch(Self::translate_dispatch(
                input_debug,
                format!(
                    "mode={} focus={} -> {}",
                    context.mode.as_str(),
                    context.focus.as_str(),
                    resolved.reason
                ),
                resolved.command,
                repeat_count,
                count_ignored,
            )));
        }

        if context.focus == InputFocusContext::Terminal
            && let Some(payload) = terminal_input_payload(&normalized)
        {
            self.clear_pending_counts();
            return Some(InputRouteOutcome::Dispatch(Self::translate_dispatch(
                input_debug,
                format!(
                    "mode={} focus={} -> terminal raw input routing",
                    context.mode.as_str(),
                    context.focus.as_str()
                ),
                Command::TerminalWriteInput(payload),
                1,
                false,
            )));
        }

        if self.pending_count.is_some() || self.operator_count.is_some() {
            self.clear_pending_counts();
            return Some(InputRouteOutcome::NoDispatch {
                input_debug,
                route_debug: format!(
                    "mode={} focus={} -> pending count cleared (no mapping)",
                    context.mode.as_str(),
                    context.focus.as_str(),
                ),
            });
        }

        None
    }

    /// IME commit path cho ngôn ngữ cần composition (ví dụ Vietnamese, Japanese...).
    pub fn translate_ime_commit(
        &self,
        text: &str,
        context: KeybindingContext,
    ) -> Option<TranslatedInput> {
        if text.is_empty() || text.chars().any(char::is_control) {
            return None;
        }

        if context.mode == EditorMode::PaletteFocus && context.command_palette_visible {
            return Some(TranslatedInput {
                input_debug: format!("IME Commit({text:?})"),
                route_debug: format!(
                    "mode={} focus={} -> IME commit -> CommandPaletteAppendQuery",
                    context.mode.as_str(),
                    context.focus.as_str()
                ),
                command: Command::FilePickerAppendQuery(text.to_string()),
                repeat_count: 1,
            });
        }

        if context.focus != InputFocusContext::Editor {
            return None;
        }

        if context.mode != EditorMode::Insert {
            return None;
        }

        Some(TranslatedInput {
            input_debug: format!("IME Commit({text:?})"),
            route_debug: format!(
                "mode={} focus={} -> IME commit -> InsertText",
                context.mode.as_str(),
                context.focus.as_str()
            ),
            command: Command::InsertText(text.to_string()),
            repeat_count: 1,
        })
    }

    fn reset_prefix_if_timed_out(&mut self, now: Instant) {
        if let Some(pending) = &self.pending_input {
            if matches!(
                pending.state,
                PendingState::LeapChar | PendingState::LeapLabel
            ) {
                return;
            }
            if now.duration_since(pending.started_at) > self.prefix_timeout {
                self.reset_prefix();
            }
        }
    }

    fn reset_prefix(&mut self) {
        self.clear_pending_input();
        self.clear_pending_counts();
    }

    /// Đặt InputHandler vào trạng thái chờ target char cho Leap.
    /// Gọi bởi AppShell sau khi handle Command::LeapStart.
    pub fn set_pending_leap_char(&mut self) {
        self.pending_input = Some(PendingInput {
            sequence: None,
            state: PendingState::LeapChar,
            started_at: Instant::now(),
        });
    }

    /// Đặt InputHandler vào trạng thái chờ label char cho Leap.
    /// Gọi bởi AppShell sau khi handle Command::LeapActivate và sinh labels thành công.
    pub fn set_pending_leap_label(&mut self) {
        self.pending_input = Some(PendingInput {
            sequence: None,
            state: PendingState::LeapLabel,
            started_at: Instant::now(),
        });
    }
}
