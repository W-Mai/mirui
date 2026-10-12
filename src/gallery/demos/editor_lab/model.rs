use crate::ui::widgets::{TextDocument, TextEditError};

pub(super) const TOOLS: [&str; 4] = ["Canvas", "Components", "Animation", "Textures"];

#[crate::model]
pub struct EditorLab {
    pub(super) keyboard: model!(owned super::keyboard::Keyboard),
    pub(super) search: model!(owned TextDocument),
    pub(super) command: model!(owned TextDocument),
    pub(super) shared: model!(owned TextDocument),
    #[observe]
    pub(super) output: &'static str,
    #[observe]
    pub(super) accent: bool,
    #[observe]
    pub(super) clipboard_status: &'static str,
}

impl Default for EditorLab {
    fn default() -> Self {
        let mut shared = TextDocument::new();
        shared.set_text("Edit either copy").unwrap();
        Self {
            keyboard: super::keyboard::Keyboard::default().into(),
            search: TextDocument::new().into(),
            command: TextDocument::new().into(),
            shared: shared.into(),
            output: "Try: help / blue / green / clear",
            accent: false,
            clipboard_status: "Select text to copy",
        }
    }
}

#[crate::model]
impl EditorLab {
    pub(super) fn clipboard_finished(
        &mut self,
        result: Result<
            crate::ui::clipboard::ClipboardOutcome,
            crate::ui::clipboard::ClipboardError,
        >,
    ) {
        use crate::ui::clipboard::{ClipboardError, ClipboardOutcome};
        self.clipboard_status = match result {
            Ok(ClipboardOutcome::Copied) => "Copied",
            Ok(ClipboardOutcome::Cut) => "Cut",
            Ok(ClipboardOutcome::Pasted) => "Pasted",
            Ok(ClipboardOutcome::CopiedWithoutCut) => "Copied; selection changed, not cut",
            Ok(ClipboardOutcome::EmptySelection) => "No selection",
            Err(ClipboardError::PermissionDenied) => "Clipboard permission denied",
            Err(ClipboardError::Unsupported) => "Clipboard unavailable",
            Err(_) => "Clipboard operation failed",
        };
    }
    #[observe]
    pub(super) fn matches(&self) -> u8 {
        let _ = self.search.revision();
        self.search.read(|query| {
            TOOLS.iter().enumerate().fold(0, |mask, (index, label)| {
                let found = query.is_empty()
                    || label
                        .as_bytes()
                        .windows(query.len())
                        .any(|part| part.eq_ignore_ascii_case(query.as_bytes()));
                mask | if found { 1 << index } else { 0 }
            })
        })
    }

    pub(super) fn submit(&mut self) -> Result<(), TextEditError> {
        let command = self.command.read(|text| match text.trim() {
            "help" => 0,
            "blue" => 1,
            "green" => 2,
            "clear" => 3,
            _ => 4,
        });
        self.output = match command {
            0 => "help / blue / green / clear",
            1 => {
                self.accent = false;
                "Accent: blue"
            }
            2 => {
                self.accent = true;
                "Accent: green"
            }
            3 => "",
            _ => "Unknown command. Type help.",
        };
        self.command.set_text("")
    }

    pub(super) fn reset_shared(&mut self) -> Result<(), TextEditError> {
        self.shared.set_text("Hello from the document")
    }
}
