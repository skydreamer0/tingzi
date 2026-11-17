//! The keys for the user's own services, kept in the system's own store for secrets
//! (Windows Credential Manager, or the macOS keychain) rather than in the settings file, so they are
//! never written as plain text or sent to the settings page.
//!
//! The transcription service and the refinement service each have their own key, under their own name.

/// The name the transcription service's key is stored under, as `Tingzi/RecognitionServiceKey`.
const RECOGNITION: &str = "RecognitionServiceKey";
/// The name the refinement service's key is stored under, as `Tingzi/RefineServiceKey`.
const REFINE: &str = "RefineServiceKey";

/// The stored key for the transcription service, or None when there is none.
pub fn read() -> Option<String> {
    crate::sys::secret_read(RECOGNITION)
}

pub fn exists() -> bool {
    read().is_some()
}

/// Stores the transcription service's `key` for this user, replacing any earlier one.
pub fn write(key: &str) -> Result<(), String> {
    write_named(RECOGNITION, key)
}

pub fn clear() {
    crate::sys::secret_clear(RECOGNITION);
}

/// The stored key for the refinement service, or None when there is none.
pub fn refine_read() -> Option<String> {
    crate::sys::secret_read(REFINE)
}

pub fn refine_exists() -> bool {
    refine_read().is_some()
}

/// Stores the refinement service's `key` for this user, replacing any earlier one.
pub fn refine_write(key: &str) -> Result<(), String> {
    write_named(REFINE, key)
}

pub fn refine_clear() {
    crate::sys::secret_clear(REFINE);
}

fn write_named(name: &str, key: &str) -> Result<(), String> {
    let key = key.trim();
    if key.is_empty() || key.len() > 2048 || key.chars().any(char::is_control) {
        return Err("金鑰的格式不正確".into());
    }
    if crate::sys::secret_store(name, key) {
        Ok(())
    } else {
        Err(format!("無法把金鑰存進 {}", crate::sys::KEY_STORE))
    }
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

    /// The two keys live side by side; clearing one leaves the other. Touches the real store, so it runs only when asked.
    #[test]
    #[ignore]
    fn the_refinement_key_is_kept_apart() {
        let (recognition, refine) = (super::read(), super::refine_read());
        super::write("sk-recognition").unwrap();
        super::refine_write("sk-refine").unwrap();
        assert_eq!((super::read().as_deref(), super::refine_read().as_deref()), (Some("sk-recognition"), Some("sk-refine")));
        super::refine_clear();
        assert_eq!((super::read().as_deref(), super::refine_read()), (Some("sk-recognition"), None));
        super::clear();
        for (kept, put) in [(recognition, super::write as fn(&str) -> Result<(), String>), (refine, super::refine_write)] {
            if let Some(key) = kept {
                put(&key).unwrap();
            }
        }
    }
}
