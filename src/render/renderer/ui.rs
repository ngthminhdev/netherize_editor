//! Panel UI rendering modules.

mod markdown_preview;
mod popups;
mod sidebar;
mod statusbar;
mod terminal;
pub(crate) mod test_runner;
mod topbar;
pub(super) mod utils;
mod welcome;
mod whichkey;
// Re-exported so the parent module can forward it to the app layer, which
// builds per-tab status values for `build_bottom_tab_strip`.
pub(crate) use terminal::TerminalTabDot;
