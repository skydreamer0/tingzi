//! Where the installed program and the user's data live.

use std::path::PathBuf;

pub use crate::sys::{data, install};

pub fn word_list() -> Option<PathBuf> {
    install().map(|folder| folder.join("phrases.txt")).filter(|file| file.is_file())
}

/// A tool shipped beside the word list, such as the keyboard switcher.
pub fn tool(name: &str) -> Option<PathBuf> {
    install().map(|folder| folder.join(name)).filter(|file| file.is_file())
}
