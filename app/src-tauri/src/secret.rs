//! The key for the user's own transcription service, kept in the system's own store for secrets
//! (Windows Credential Manager, or the macOS keychain) rather than in the settings file, so it is
//! never written as plain text or sent to the settings page.

/// The stored key, or None when there is none.
pub fn read() -> Option<String> {
    crate::sys::secret_read()
}

pub fn exists() -> bool {
    read().is_some()
}

/// Stores `key` for this user, replacing any earlier one.
pub fn write(key: &str) -> Result<(), String> {
    let key = key.trim();
    if key.is_empty() || key.len() > 2048 || key.chars().any(char::is_control) {
        return Err("金鑰的格式不正確".into());
    }
    if crate::sys::secret_store(key) {
        Ok(())
    } else {
        Err(format!("無法把金鑰存進 {}", crate::sys::KEY_STORE))
    }
}

pub fn clear() {
    crate::sys::secret_clear();
}

#[cfg(test)]
mod tests {
    /// Touches this user's real store of secrets, so it runs only when asked: `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn a_key_survives_storing_and_clearing() {
        let before = super::read();
        super::write(" sk-test-聽字 ").unwrap();
        assert_eq!(super::read().as_deref(), Some("sk-test-聽字"));
        assert!(super::write("bad\nkey").is_err());
        super::clear();
        assert_eq!(super::read(), None);
        if let Some(key) = before {
            super::write(&key).unwrap();
        }
    }
}
