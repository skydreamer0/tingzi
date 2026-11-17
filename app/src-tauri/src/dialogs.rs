//! The dialogs the app needs: picking a file, and asking before something that cannot be undone.

pub use crate::sys::{confirm, open, pick};

/// The kinds of file the app opens or saves.
#[derive(Clone, Copy)]
pub enum Kind {
    Wav,
    Text,
    Table,
}
