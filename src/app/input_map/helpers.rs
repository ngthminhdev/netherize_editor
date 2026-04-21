use super::*;

pub(super) fn insert_command_from_text(text: &Option<String>) -> Option<Command> {
    let text = text.as_ref()?;
    if text.is_empty() || text.chars().any(char::is_control) {
        return None;
    }
    if text.chars().count() == 1 {
        return text.chars().next().map(Command::InsertChar);
    }
    Some(Command::InsertText(text.clone()))
}

pub(super) fn palette_query_from_text(text: &Option<String>) -> Option<Command> {
    let text = text.as_ref()?;
    if text.is_empty() || text.chars().any(char::is_control) {
        return None;
    }
    Some(Command::FilePickerAppendQuery(text.clone()))
}
