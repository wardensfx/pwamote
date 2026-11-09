//! Implémentations propres à un système d'exploitation.

#[cfg(windows)]
pub mod windows;
#[cfg(windows)]
pub mod windows_clipboard;
#[cfg(windows)]
pub mod windows_magnifier;
