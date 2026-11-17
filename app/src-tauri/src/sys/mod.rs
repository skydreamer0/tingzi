//! Everything the app asks of the operating system, written once for each one.
//!
//! The rest of the app holds the rules; these files hold the calls. Both files offer the same
//! functions, so a module that uses `sys::` builds on Windows and on macOS.

#[cfg(windows)]
mod win;
#[cfg(windows)]
pub use win::*;

#[cfg(target_os = "macos")]
mod mac;
#[cfg(target_os = "macos")]
pub use mac::*;
