use std::time::{Duration, Instant};

use winit::{
    event::{ElementState, KeyEvent, Modifiers},
    keyboard::{KeyCode, ModifiersState, NamedKey},
};

use crate::{
    app::input_map::{InputFocusContext, InputMap, KeybindingContext, SequenceMatch},
    core::{
        commands::{Command, Operator},
        mode::EditorMode,
    },
};

use super::{
    helpers::{
        find_motion_prefix_from_input, inner_or_around_from_input, is_modifier_only_key,
        motion_from_input, numeric_count_digit_from_input, printable_char_from_input,
        replace_char_from_input, should_start_replace_pending, should_start_yank_pending,
        terminal_input_payload, text_object_kind_from_input,
    },
    model::{InputRouteOutcome, NormalizedInput, TranslatedInput},
    pending::{PendingInput, PendingState, classify_pending_state},
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
        if key_event.state != ElementState::Pressed {
            return None;
        }

        let normalized = NormalizedInput::from_key_event(key_event, self.modifiers);
        if key_event.repeat {
            return self.route_repeated_normalized_input(normalized, input_map, context);
        }
        self.route_normalized_input(normalized, input_map, context, Instant::now())
    }

    pub(crate) fn route_repeated_normalized_input(
        &mut self,
        normalized: NormalizedInput,
        input_map: &InputMap,
        context: KeybindingContext,
    ) -> Option<InputRouteOutcome> {
        // Repeat events chỉ được phép đi qua cho các command an toàn khi giữ phím.
        // Không để OS auto-repeat vô tình hoàn thành prefix/chord như dd, gcc, gg...
        if self.pending_input.is_some()
            || self.pending_count.is_some()
            || self.operator_count.is_some()
        {
            return None;
        }

        let input_debug = normalized.debug_label();

        if matches!(
            context.focus,
            InputFocusContext::BufferTerminal | InputFocusContext::Terminal
        ) && let Some(payload) = terminal_input_payload(&normalized)
        {
            self.clear_pending_counts();
            return Some(InputRouteOutcome::Dispatch(Self::translate_dispatch(
                input_debug,
                format!(
                    "mode={} focus={} -> repeated terminal raw input",
                    context.mode.as_str(),
                    context.focus.as_str()
                ),
                Command::TerminalWriteInput(payload),
                1,
                false,
            )));
        }

        let resolved = input_map.resolve(&normalized, context)?;
        if !resolved.command.supports_press_and_hold_repeat() {
            return None;
        }

        Some(InputRouteOutcome::Dispatch(Self::translate_dispatch(
            input_debug,
            format!(
                "mode={} focus={} -> key repeat -> {}",
                context.mode.as_str(),
                context.focus.as_str(),
                resolved.reason
            ),
            resolved.command,
            1,
            false,
        )))
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

        // AI Chat input mode: intercept all keys when right sidebar AI Chat is focused.
        // Bypasses vim mode processing entirely — input goes to chat input_box.
        if context.focus == InputFocusContext::AiChat {
            return self.route_ai_chat_input(normalized, input_debug, context);
        }

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
                            "mode={} focus={} -> leap label input {:?}",
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
                PendingState::VisualTextObjectModifier { modifier } => {
                    let modifier = *modifier;
                    if let Some(kind) = text_object_kind_from_input(&normalized) {
                        self.clear_pending_input();
                        let command = Command::TextObjectAction {
                            op: Operator::Visual,
                            modifier,
                            kind,
                        };
                        let (repeat_count, count_ignored) =
                            self.consume_repeat_count_for_command(&command, Some(&pending.state));
                        return Some(InputRouteOutcome::Dispatch(Self::translate_dispatch(
                            input_debug,
                            format!(
                                "mode={} focus={} -> visual text object {:?} {:?}",
                                context.mode.as_str(),
                                context.focus.as_str(),
                                modifier,
                                kind
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
                                "mode={} focus={} -> visual text object: not a text object kind, fallback -> {}",
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
                            "mode={} focus={} -> visual text object: not a text object kind, no fallback",
                            context.mode.as_str(),
                            context.focus.as_str()
                        ),
                    });
                }

                PendingState::OperatorWithObject { op, modifier } => {
                    let op = *op;
                    let modifier = *modifier;
                    if let Some(kind) = text_object_kind_from_input(&normalized) {
                        let command = Command::TextObjectAction { op, modifier, kind };
                        self.clear_pending_input();
                        let (repeat_count, count_ignored) =
                            self.consume_repeat_count_for_command(&command, Some(&pending.state));
                        return Some(InputRouteOutcome::Dispatch(Self::translate_dispatch(
                            input_debug,
                            format!(
                                "mode={} focus={} -> text object {:?} {:?} ({:?})",
                                context.mode.as_str(),
                                context.focus.as_str(),
                                modifier,
                                kind,
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
                            "mode={} focus={} -> operator+object: not a text object kind, reset",
                            context.mode.as_str(),
                            context.focus.as_str()
                        ),
                    });
                }

                PendingState::PendingOperator { op } => {
                    let op = *op;
                    if let Some(find_kind) = find_motion_prefix_from_input(&normalized) {
                        self.pending_input = Some(PendingInput {
                            sequence: None,
                            state: PendingState::OperatorFindChar {
                                op,
                                kind: find_kind,
                            },
                            started_at: now,
                        });
                        return Some(InputRouteOutcome::NoDispatch {
                            input_debug,
                            route_debug: format!(
                                "mode={} focus={} -> {}: waiting find target char",
                                context.mode.as_str(),
                                context.focus.as_str(),
                                pending.state.as_str(),
                            ),
                        });
                    }

                    if let Some(modifier) = inner_or_around_from_input(&normalized) {
                        self.pending_input = Some(PendingInput {
                            state: PendingState::OperatorWithObject { op, modifier },
                            sequence: None,
                            started_at: now,
                        });
                        return Some(InputRouteOutcome::NoDispatch {
                            input_debug,
                            route_debug: format!(
                                "mode={} focus={} -> {}: got modifier '{:?}', waiting text object kind",
                                context.mode.as_str(),
                                context.focus.as_str(),
                                pending.state.as_str(),
                                modifier
                            ),
                        });
                    }

                    let doubled = match op {
                        Operator::Delete
                            if normalized.physical_key == Some(KeyCode::KeyD)
                                || normalized.text.as_deref() == Some("d") =>
                        {
                            true
                        }
                        Operator::Change
                            if normalized.physical_key == Some(KeyCode::KeyC)
                                || normalized.text.as_deref() == Some("c") =>
                        {
                            true
                        }
                        Operator::Yank
                            if normalized.physical_key == Some(KeyCode::KeyY)
                                || normalized.text.as_deref() == Some("y") =>
                        {
                            true
                        }
                        _ => false,
                    };
                    if doubled {
                        let command = Command::Operate {
                            op,
                            target: crate::core::commands::OperationTarget::CurrentLine,
                        };
                        self.clear_pending_input();
                        let (repeat_count, count_ignored) =
                            self.consume_repeat_count_for_command(&command, Some(&pending.state));
                        return Some(InputRouteOutcome::Dispatch(Self::translate_dispatch(
                            input_debug,
                            format!(
                                "mode={} focus={} -> pending operator linewise resolved",
                                context.mode.as_str(),
                                context.focus.as_str()
                            ),
                            command,
                            repeat_count,
                            count_ignored,
                        )));
                    }

                    if normalized.physical_key == Some(KeyCode::KeyG)
                        || normalized.text.as_deref() == Some("g")
                    {
                        self.pending_input = Some(PendingInput {
                            sequence: None,
                            state: PendingState::OperatorG { op },
                            started_at: now,
                        });
                        return Some(InputRouteOutcome::NoDispatch {
                            input_debug,
                            route_debug: format!(
                                "mode={} focus={} -> {}: waiting second g",
                                context.mode.as_str(),
                                context.focus.as_str(),
                                pending.state.as_str(),
                            ),
                        });
                    }

                    let motion = motion_from_input(&normalized);
                    if let Some(motion) = motion {
                        let command = Command::Operate {
                            op,
                            target: crate::core::commands::OperationTarget::Motion(motion),
                        };
                        self.clear_pending_input();
                        let (repeat_count, count_ignored) =
                            self.consume_repeat_count_for_command(&command, Some(&pending.state));
                        return Some(InputRouteOutcome::Dispatch(Self::translate_dispatch(
                            input_debug,
                            format!(
                                "mode={} focus={} -> pending operator motion resolved",
                                context.mode.as_str(),
                                context.focus.as_str()
                            ),
                            command,
                            repeat_count,
                            count_ignored,
                        )));
                    }
                }

                PendingState::OperatorG { op } => {
                    if normalized.physical_key == Some(KeyCode::KeyG)
                        || normalized.text.as_deref() == Some("g")
                    {
                        let command = Command::Operate {
                            op: *op,
                            target: crate::core::commands::OperationTarget::Motion(
                                crate::core::commands::Motion::FirstLine,
                            ),
                        };
                        self.clear_pending_input();
                        let (repeat_count, count_ignored) =
                            self.consume_repeat_count_for_command(&command, Some(&pending.state));
                        return Some(InputRouteOutcome::Dispatch(Self::translate_dispatch(
                            input_debug,
                            format!(
                                "mode={} focus={} -> pending operator gg resolved",
                                context.mode.as_str(),
                                context.focus.as_str()
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
                            "mode={} focus={} -> pending operator g-motion reset",
                            context.mode.as_str(),
                            context.focus.as_str()
                        ),
                    });
                }

                PendingState::OperatorFindChar { op, kind } => {
                    if let Some(ch) = replace_char_from_input(&normalized) {
                        let command = Command::Operate {
                            op: *op,
                            target: crate::core::commands::OperationTarget::Motion(
                                crate::core::commands::Motion::FindChar(*kind, ch),
                            ),
                        };
                        self.clear_pending_input();
                        let (repeat_count, count_ignored) =
                            self.consume_repeat_count_for_command(&command, Some(&pending.state));
                        return Some(InputRouteOutcome::Dispatch(Self::translate_dispatch(
                            input_debug,
                            format!(
                                "mode={} focus={} -> pending operator find-char resolved",
                                context.mode.as_str(),
                                context.focus.as_str()
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
                            "mode={} focus={} -> pending operator find-char reset",
                            context.mode.as_str(),
                            context.focus.as_str()
                        ),
                    });
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
                state: PendingState::PendingOperator { op: Operator::Yank },
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

        if context.focus == InputFocusContext::Editor && context.mode == EditorMode::Normal {
            let op = if !normalized.has_command_modifier()
                && !normalized.modifiers.alt_key()
                && !normalized.modifiers.shift_key()
                && (normalized.physical_key == Some(KeyCode::KeyD)
                    || normalized.text.as_deref() == Some("d"))
            {
                Some(Operator::Delete)
            } else if !normalized.has_command_modifier()
                && !normalized.modifiers.alt_key()
                && !normalized.modifiers.shift_key()
                && (normalized.physical_key == Some(KeyCode::KeyC)
                    || normalized.text.as_deref() == Some("c"))
            {
                Some(Operator::Change)
            } else {
                None
            };

            if let Some(op) = op {
                if self.operator_count.is_none() {
                    self.operator_count = self.pending_count.take();
                }
                self.pending_input = Some(PendingInput {
                    sequence: None,
                    state: PendingState::PendingOperator { op },
                    started_at: now,
                });
                return Some(InputRouteOutcome::NoDispatch {
                    input_debug,
                    route_debug: format!(
                        "mode={} focus={} -> pending operator {:?} start",
                        context.mode.as_str(),
                        context.focus.as_str(),
                        op
                    ),
                });
            }
        }

        if context.focus == InputFocusContext::Editor && context.mode == EditorMode::Visual {
            if let Some(modifier) = inner_or_around_from_input(&normalized) {
                self.pending_input = Some(PendingInput {
                    sequence: None,
                    state: PendingState::VisualTextObjectModifier { modifier },
                    started_at: now,
                });
                return Some(InputRouteOutcome::NoDispatch {
                    input_debug,
                    route_debug: format!(
                        "mode={} focus={} -> visual text object modifier '{:?}' start, waiting text object kind",
                        context.mode.as_str(),
                        context.focus.as_str(),
                        modifier
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

        // BufferTerminal (lazygit, v.v.): bypass keymap hoàn toàn.
        // Mọi input kể cả ESC được forward thẳng vào PTY.
        // Chỉ modifier-only keys (Shift, Ctrl...) bị bỏ qua.
        if context.focus == InputFocusContext::BufferTerminal {
            if normalized.has_command_modifier() && normalized.physical_key == Some(KeyCode::KeyV) {
                self.clear_pending_counts();
                return Some(InputRouteOutcome::Dispatch(Self::translate_dispatch(
                    input_debug,
                    format!(
                        "mode={} focus={} -> buffer terminal paste",
                        context.mode.as_str(),
                        context.focus.as_str()
                    ),
                    Command::TerminalPaste,
                    1,
                    false,
                )));
            }

            if !is_modifier_only_key(&normalized) {
                if let Some(payload) = terminal_input_payload(&normalized) {
                    self.clear_pending_counts();
                    return Some(InputRouteOutcome::Dispatch(Self::translate_dispatch(
                        input_debug,
                        format!(
                            "mode={} focus={} -> buffer terminal raw input",
                            context.mode.as_str(),
                            context.focus.as_str()
                        ),
                        Command::TerminalWriteInput(payload),
                        1,
                        false,
                    )));
                }
            }
            return None;
        }

        if context.focus == InputFocusContext::Editor
            && context.mode == EditorMode::Insert
            && context.completion_visible
        {
            if normalized.named_key == Some(NamedKey::Tab)
                && !normalized.modifiers.control_key()
                && !normalized.modifiers.alt_key()
                && !normalized.modifiers.super_key()
                && !normalized.modifiers.shift_key()
            {
                return Some(InputRouteOutcome::Dispatch(Self::translate_dispatch(
                    input_debug,
                    format!(
                        "mode={} focus={} -> completion accept intercept (Tab)",
                        context.mode.as_str(),
                        context.focus.as_str()
                    ),
                    Command::CompletionAccept,
                    1,
                    false,
                )));
            }

            if normalized.modifiers.control_key()
                && !normalized.modifiers.alt_key()
                && !normalized.modifiers.super_key()
            {
                match normalized.text.as_deref() {
                    Some("n") | Some("N") => {
                        return Some(InputRouteOutcome::Dispatch(Self::translate_dispatch(
                            input_debug,
                            format!(
                                "mode={} focus={} -> completion next intercept (Ctrl+N)",
                                context.mode.as_str(),
                                context.focus.as_str()
                            ),
                            Command::CompletionNext,
                            1,
                            false,
                        )));
                    }
                    Some("p") | Some("P") => {
                        return Some(InputRouteOutcome::Dispatch(Self::translate_dispatch(
                            input_debug,
                            format!(
                                "mode={} focus={} -> completion prev intercept (Ctrl+P)",
                                context.mode.as_str(),
                                context.focus.as_str()
                            ),
                            Command::CompletionPrev,
                            1,
                            false,
                        )));
                    }
                    _ => {}
                }
            }

            match normalized.named_key {
                Some(NamedKey::ArrowDown) => {
                    return Some(InputRouteOutcome::Dispatch(Self::translate_dispatch(
                        input_debug,
                        format!(
                            "mode={} focus={} -> completion next intercept (ArrowDown)",
                            context.mode.as_str(),
                            context.focus.as_str()
                        ),
                        Command::CompletionNext,
                        1,
                        false,
                    )));
                }
                Some(NamedKey::ArrowUp) => {
                    return Some(InputRouteOutcome::Dispatch(Self::translate_dispatch(
                        input_debug,
                        format!(
                            "mode={} focus={} -> completion prev intercept (ArrowUp)",
                            context.mode.as_str(),
                            context.focus.as_str()
                        ),
                        Command::CompletionPrev,
                        1,
                        false,
                    )));
                }
                _ => {}
            }

            if normalized.named_key == Some(NamedKey::Enter) {
                return Some(InputRouteOutcome::Dispatch(Self::translate_dispatch(
                    input_debug,
                    format!(
                        "mode={} focus={} -> completion accept intercept",
                        context.mode.as_str(),
                        context.focus.as_str()
                    ),
                    Command::CompletionAccept,
                    1,
                    false,
                )));
            }

            if normalized.named_key == Some(NamedKey::Escape) {
                return Some(InputRouteOutcome::Dispatch(Self::translate_dispatch(
                    input_debug,
                    format!(
                        "mode={} focus={} -> completion close intercept",
                        context.mode.as_str(),
                        context.focus.as_str()
                    ),
                    Command::CompletionClose,
                    1,
                    false,
                )));
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
            && context.mode == EditorMode::TerminalFocus
            && let Some(payload) = terminal_input_payload(&normalized)
        {
            self.clear_pending_counts();
            return Some(InputRouteOutcome::Dispatch(Self::translate_dispatch(
                input_debug,
                format!(
                    "mode={} focus={} -> strict terminal raw input routing",
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

        // AI Chat: route IME commit to chat input buffer.
        if context.focus == InputFocusContext::AiChat {
            return Some(TranslatedInput {
                input_debug: format!("IME Commit({text:?})"),
                route_debug: format!(
                    "mode={} focus={} -> IME commit -> AiChatInputText",
                    context.mode.as_str(),
                    context.focus.as_str()
                ),
                command: Command::AiChatInputText(text.to_string()),
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

    /// Route input when right sidebar AI Chat tab is focused.
    /// All keys are intercepted — Esc closes, Enter sends, printable chars
    /// append to input buffer, Backspace deletes last char. Everything else
    /// is swallowed so keys don't leak into the editor buffer.
    fn route_ai_chat_input(
        &mut self,
        normalized: NormalizedInput,
        input_debug: String,
        context: KeybindingContext,
    ) -> Option<InputRouteOutcome> {
        // Esc → unfocus AI chat, return focus to editor (keep dock visible)
        if normalized.named_key == Some(NamedKey::Escape) {
            return Some(InputRouteOutcome::Dispatch(Self::translate_dispatch(
                input_debug,
                format!(
                    "mode={} focus={} -> ai chat: unfocus (Esc)",
                    context.mode.as_str(),
                    context.focus.as_str(),
                ),
                Command::AiChatUnfocus,
                1,
                false,
            )));
        }

        // Enter → send message
        if normalized.named_key == Some(NamedKey::Enter)
            && !normalized.has_command_modifier()
        {
            return Some(InputRouteOutcome::Dispatch(Self::translate_dispatch(
                input_debug,
                format!(
                    "mode={} focus={} -> ai chat: send (Enter)",
                    context.mode.as_str(),
                    context.focus.as_str(),
                ),
                Command::AiChatSend,
                1,
                false,
            )));
        }

        // Backspace → delete last char
        if normalized.named_key == Some(NamedKey::Backspace)
            && !normalized.has_command_modifier()
        {
            return Some(InputRouteOutcome::Dispatch(Self::translate_dispatch(
                input_debug,
                format!(
                    "mode={} focus={} -> ai chat: backspace",
                    context.mode.as_str(),
                    context.focus.as_str(),
                ),
                Command::AiChatBackspace,
                1,
                false,
            )));
        }

        // Printable character → append to input buffer
        if let Some(ch) = printable_char_from_input(&normalized) {
            return Some(InputRouteOutcome::Dispatch(Self::translate_dispatch(
                input_debug,
                format!(
                    "mode={} focus={} -> ai chat: input char {:?}",
                    context.mode.as_str(),
                    context.focus.as_str(),
                    ch,
                ),
                Command::AiChatInputChar(ch),
                1,
                false,
            )));
        }

        // Space → append space to input buffer
        if normalized.named_key == Some(NamedKey::Space)
            && !normalized.has_command_modifier()
        {
            return Some(InputRouteOutcome::Dispatch(Self::translate_dispatch(
                input_debug,
                format!(
                    "mode={} focus={} -> ai chat: input char ' '",
                    context.mode.as_str(),
                    context.focus.as_str(),
                ),
                Command::AiChatInputChar(' '),
                1,
                false,
            )));
        }

        // Ignore all other keys while in AI chat (modifier-only, arrows, etc.)
        None
    }
}
