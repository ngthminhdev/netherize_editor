#[derive(Debug, Clone, PartialEq)]
pub enum SettingItem {
    ThemeSelector { current: String },
    FontFamily { current: String },
    FontSize { current: f32 },
    LineHeight { current: f32 },
    IndentTabWidth { current: u8 },
    IndentInsertSpaces { enabled: bool },
    SidebarWidth { current: i32 },
    RightSidebarWidth { current: i32 },
    BottomPanelHeight { current: i32 },
    UiRounding { enabled: bool, radius_px: f32 },
}

impl SettingItem {
    pub fn title(&self) -> &'static str {
        match self {
            Self::ThemeSelector { .. } => "Theme",
            Self::FontFamily { .. } => "Font Family",
            Self::FontSize { .. } => "Font Size",
            Self::LineHeight { .. } => "Line Height",
            Self::IndentTabWidth { .. } => "Tab Width",
            Self::IndentInsertSpaces { .. } => "Indent Style",
            Self::SidebarWidth { .. } => "Left Dock Width",
            Self::RightSidebarWidth { .. } => "Right Dock Width",
            Self::BottomPanelHeight { .. } => "Bottom Dock Height",
            Self::UiRounding { .. } => "UI Rounding",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SettingsEditingKind {
    FontFamily,
    FontSize,
    LineHeight,
    IndentTabWidth,
    SidebarWidth,
    RightSidebarWidth,
    BottomPanelHeight,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SettingsEditingState {
    pub kind: SettingsEditingKind,
    pub draft: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SettingsState {
    pub selected_index: usize,
    pub items: Vec<SettingItem>,
    pub editing: Option<SettingsEditingState>,
}

impl SettingsState {
    pub fn new(
        theme_profile: impl Into<String>,
        font_family: impl Into<String>,
        font_size: f32,
        line_height: f32,
        tab_width: u8,
        insert_spaces: bool,
        left_width: i32,
        right_width: i32,
        bottom_height: i32,
        ui_rounding_enabled: bool,
        border_radius_px: f32,
    ) -> Self {
        Self {
            selected_index: 0,
            items: vec![
                SettingItem::ThemeSelector {
                    current: theme_profile.into(),
                },
                SettingItem::FontFamily {
                    current: font_family.into(),
                },
                SettingItem::FontSize {
                    current: font_size.max(1.0),
                },
                SettingItem::LineHeight {
                    current: line_height.max(1.0),
                },
                SettingItem::IndentTabWidth {
                    current: tab_width.max(1),
                },
                SettingItem::IndentInsertSpaces {
                    enabled: insert_spaces,
                },
                SettingItem::SidebarWidth {
                    current: left_width.max(0),
                },
                SettingItem::RightSidebarWidth {
                    current: right_width.max(0),
                },
                SettingItem::BottomPanelHeight {
                    current: bottom_height.max(0),
                },
                SettingItem::UiRounding {
                    enabled: ui_rounding_enabled,
                    radius_px: border_radius_px.max(0.0),
                },
            ],
            editing: None,
        }
    }

    pub fn select_next(&mut self) -> bool {
        if self.selected_index + 1 < self.items.len() {
            self.selected_index += 1;
            true
        } else {
            false
        }
    }

    pub fn select_prev(&mut self) -> bool {
        if self.selected_index > 0 {
            self.selected_index -= 1;
            true
        } else {
            false
        }
    }

    pub fn selected_item(&self) -> Option<&SettingItem> {
        self.items.get(self.selected_index)
    }

    pub fn selected_item_mut(&mut self) -> Option<&mut SettingItem> {
        self.items.get_mut(self.selected_index)
    }

    pub fn begin_editing(&mut self) -> bool {
        let Some(item) = self.selected_item() else {
            return false;
        };
        let (kind, draft) = match item {
            SettingItem::FontFamily { current } => {
                (SettingsEditingKind::FontFamily, current.clone())
            }
            SettingItem::FontSize { current } => {
                (SettingsEditingKind::FontSize, format!("{current:.1}"))
            }
            SettingItem::LineHeight { current } => {
                (SettingsEditingKind::LineHeight, format!("{current:.1}"))
            }
            SettingItem::SidebarWidth { current } => {
                (SettingsEditingKind::SidebarWidth, current.to_string())
            }
            SettingItem::RightSidebarWidth { current } => {
                (SettingsEditingKind::RightSidebarWidth, current.to_string())
            }
            SettingItem::BottomPanelHeight { current } => {
                (SettingsEditingKind::BottomPanelHeight, current.to_string())
            }
            SettingItem::IndentTabWidth { current } => {
                (SettingsEditingKind::IndentTabWidth, current.to_string())
            }
            SettingItem::ThemeSelector { .. }
            | SettingItem::UiRounding { .. }
            | SettingItem::IndentInsertSpaces { .. } => return false,
        };
        self.editing = Some(SettingsEditingState { kind, draft });
        true
    }

    pub fn cancel_editing(&mut self) -> bool {
        self.editing.take().is_some()
    }

    pub fn append_editing_text(&mut self, text: &str) -> bool {
        let Some(editing) = &mut self.editing else {
            return false;
        };
        editing.draft.push_str(text);
        true
    }

    pub fn backspace_editing(&mut self) -> bool {
        let Some(editing) = &mut self.editing else {
            return false;
        };
        editing.draft.pop().is_some()
    }
}
