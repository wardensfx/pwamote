//! Presse-papiers Windows, via `arboard` : texte Unicode, et image en DIB
//! accompagnée d'une copie PNG (transparence conservée pour les applications qui la lisent).

use std::borrow::Cow;

use crate::clipboard::{ClipboardContent, ClipboardSink};

pub struct WindowsClipboard;

impl ClipboardSink for WindowsClipboard {
    fn set(&self, content: &ClipboardContent) -> Result<(), String> {
        let mut clipboard =
            arboard::Clipboard::new().map_err(|e| format!("presse-papiers indisponible : {e}"))?;
        match content {
            ClipboardContent::Text(text) => clipboard.set_text(text.as_str()),
            ClipboardContent::Image(img) => clipboard.set_image(arboard::ImageData {
                width: img.width as usize,
                height: img.height as usize,
                bytes: Cow::Borrowed(&img.pixels),
            }),
        }
        .map_err(|e| format!("écriture dans le presse-papiers impossible : {e}"))
    }
}
