//! The key for the user's own transcription service, kept in Windows Credential Manager
//! rather than in the settings file, so it is never written as plain text or sent to the settings page.

use windows::core::{HSTRING, PWSTR};
use windows::Win32::Security::Credentials::{
    CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_FLAGS, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC,
};

const TARGET: &str = "Tingzi/RecognitionServiceKey";

/// The stored key, or None when there is none.
pub fn read() -> Option<String> {
    let target = HSTRING::from(TARGET);
    let mut found: *mut CREDENTIALW = std::ptr::null_mut();
    // SAFETY: CredReadW fills `found` with a buffer Windows owns, which is read and then freed with CredFree.
    unsafe {
        CredReadW(&target, CRED_TYPE_GENERIC, 0, &mut found).ok()?;
        let blob = std::slice::from_raw_parts((*found).CredentialBlob, (*found).CredentialBlobSize as usize).to_vec();
        CredFree(found as *const _);
        String::from_utf8(blob).ok().filter(|key| !key.is_empty())
    }
}

pub fn exists() -> bool {
    read().is_some()
}

/// Stores `key` for this Windows user, replacing any earlier one.
pub fn write(key: &str) -> Result<(), String> {
    let key = key.trim();
    if key.is_empty() || key.len() > 2048 || key.chars().any(char::is_control) {
        return Err("金鑰的格式不正確".into());
    }
    let mut target: Vec<u16> = TARGET.encode_utf16().chain([0]).collect();
    let mut blob = key.as_bytes().to_vec();
    let credential = CREDENTIALW {
        Flags: CRED_FLAGS(0),
        Type: CRED_TYPE_GENERIC,
        TargetName: PWSTR(target.as_mut_ptr()),
        CredentialBlobSize: blob.len() as u32,
        CredentialBlob: blob.as_mut_ptr(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        ..Default::default()
    };
    // SAFETY: every pointer in `credential` points into buffers that live until the call returns.
    unsafe { CredWriteW(&credential, 0) }.map_err(|_| "無法把金鑰存進 Windows 認證管理員".to_string())
}

pub fn clear() {
    // SAFETY: the target name is a valid string for the length of the call.
    let _ = unsafe { CredDeleteW(&HSTRING::from(TARGET), CRED_TYPE_GENERIC, 0) };
}

#[cfg(test)]
mod tests {
    /// Touches this user's real Credential Manager, so it runs only when asked: `cargo test -- --ignored`.
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
