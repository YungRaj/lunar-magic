use super::{ClipboardError, ClipboardKind, ClipboardPayload};

#[path = "clipboard_domains/exanimation.rs"]
mod exanimation;
#[path = "clipboard_domains/graphics.rs"]
mod graphics;
#[path = "clipboard_domains/layer3.rs"]
mod layer3;
#[path = "clipboard_domains/level.rs"]
mod level;
#[path = "clipboard_domains/map16.rs"]
mod map16;
#[path = "clipboard_domains/overworld.rs"]
mod overworld;
#[path = "clipboard_domains/palette.rs"]
mod palette;

impl ClipboardPayload {
    pub(super) fn require_kind(&self, expected: ClipboardKind) -> Result<(), ClipboardError> {
        if self.kind == expected {
            Ok(())
        } else {
            Err(ClipboardError::WrongKind {
                expected,
                actual: self.kind,
            })
        }
    }
}
