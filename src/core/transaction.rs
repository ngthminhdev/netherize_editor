#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CursorState {
    pub char_idx: usize,
    pub target_col: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditAction {
    Insert { index: usize, text: String },
    Delete { index: usize, text: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transaction {
    pub actions: Vec<EditAction>,
    pub before_cursor: CursorState,
    pub after_cursor: CursorState,
}

impl Transaction {
    pub fn new(before_cursor: CursorState) -> Self {
        Self {
            actions: Vec::new(),
            before_cursor,
            after_cursor: before_cursor,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }
}

#[derive(Debug, Clone, Default)]
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
