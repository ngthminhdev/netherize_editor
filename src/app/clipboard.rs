pub trait ClipboardProvider {
    fn get_text(&mut self) -> Result<String, String>;
    fn set_text(&mut self, text: &str) -> Result<(), String>;
}

#[derive(Default)]
pub struct SystemClipboard {
    clipboard: Option<arboard::Clipboard>,
    init_error_logged: bool,
}

impl SystemClipboard {
    pub fn new() -> Self {
        let mut clipboard = Self::default();
        let _ = clipboard.ensure_initialized();
        clipboard
    }

    fn ensure_initialized(&mut self) -> Result<(), String> {
        if self.clipboard.is_some() {
            return Ok(());
        }

        match arboard::Clipboard::new() {
            Ok(clipboard) => {
                self.clipboard = Some(clipboard);
                Ok(())
            }
            Err(err) => {
                let message = format!("system clipboard init failed: {err}");
                if !self.init_error_logged {
                    eprintln!("[clipboard] {message}");
                    self.init_error_logged = true;
                }
                Err(message)
            }
        }
    }

    fn clipboard_mut(&mut self) -> Result<&mut arboard::Clipboard, String> {
        self.ensure_initialized()?;
        self.clipboard
            .as_mut()
            .ok_or_else(|| "system clipboard unavailable".to_string())
    }
}

impl ClipboardProvider for SystemClipboard {
    fn get_text(&mut self) -> Result<String, String> {
        let clipboard = self.clipboard_mut()?;
        clipboard
            .get_text()
            .map_err(|err| format!("system clipboard read failed: {err}"))
    }

    fn set_text(&mut self, text: &str) -> Result<(), String> {
        let clipboard = self.clipboard_mut()?;
        clipboard
            .set_text(text.to_string())
            .map_err(|err| format!("system clipboard write failed: {err}"))
    }
}
