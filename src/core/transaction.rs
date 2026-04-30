use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CursorState {
    pub char_idx: usize,
    pub target_col: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditTransaction {
    pub start_char_idx: usize,
    pub deleted_text: String,
    pub inserted_text: String,
}

impl EditTransaction {
    pub fn new(start_char_idx: usize, deleted_text: String, inserted_text: String) -> Self {
        Self {
            start_char_idx,
            deleted_text,
            inserted_text,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.deleted_text.is_empty() && self.inserted_text.is_empty()
    }

    pub fn deleted_len_chars(&self) -> usize {
        self.deleted_text.chars().count()
    }

    pub fn inserted_len_chars(&self) -> usize {
        self.inserted_text.chars().count()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transaction {
    pub edit: EditTransaction,
    pub before_cursor: CursorState,
    pub after_cursor: CursorState,
}

impl Transaction {
    pub fn new(
        edit: EditTransaction,
        before_cursor: CursorState,
        after_cursor: CursorState,
    ) -> Self {
        Self {
            before_cursor,
            after_cursor,
            edit,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.edit.is_empty()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EditHistory {
    pub undo_stack: Vec<Transaction>,
    pub redo_stack: Vec<Transaction>,
}

impl EditHistory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }
}
