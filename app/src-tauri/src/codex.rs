//! What the app knows about the Codex desktop app: its message box, for the Ctrl+Alt+V draft
//! shortcut, and the version installed, which online recognition through Codex names itself by.

// OpenAI's desktop migration guide links here for the app that includes Codex on both platforms:
// https://help.openai.com/en/articles/20001276-moving-to-the-new-chatgpt-desktop-app
const DOWNLOAD_PAGE: &str = "https://chatgpt.com/download/";

pub(crate) fn open_sign_in() -> Result<(), &'static str> {
    launch_sign_in(
        crate::sys::open_codex_desktop,
        || crate::sys::codex_cli().map(|cli| crate::sys::open_codex_cli(&cli)),
        || crate::sys::open(DOWNLOAD_PAGE),
    )
}

fn launch_sign_in(
    desktop: impl FnOnce() -> Option<bool>,
    cli: impl FnOnce() -> Option<bool>,
    download: impl FnOnce() -> bool,
) -> Result<(), &'static str> {
    if let Some(opened) = desktop() {
        return opened.then_some(()).ok_or("無法開啟 Codex 桌面版，請手動開啟");
    }
    if let Some(opened) = cli() {
        return opened.then_some(()).ok_or("無法開啟終端機，請手動執行 codex login");
    }
    download().then_some(()).ok_or("無法開啟 Codex 下載頁")
}

/// Whether the keyboard is in Codex's message box rather than, say, its search field.
pub fn in_message_box(process: u32) -> bool {
    crate::sys::in_codex_message_box(process)
}

/// The message box is a ProseMirror editor; ordinary search boxes are not.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn is_composer(class: &str, automation_id: &str) -> bool {
    class.split_whitespace().any(|name| name == "ProseMirror") || automation_id == "prompt-textarea"
}

/// The Codex version in a Windows package name such as `OpenAI.Codex_26.1111.2228.0_x64__2p2nqsd0c76g0`,
/// as numbers so versions can be compared. Windows packages carry four numbers.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn package_version(full_name: &str) -> Option<[u32; 4]> {
    let numbers: Vec<u32> = full_name.split('_').nth(1)?.split('.').map(|part| part.parse().ok()).collect::<Option<_>>()?;
    numbers.try_into().ok()
}

/// The version as Codex writes it: three numbers. The fourth in a package name is filler Windows
/// requires, and is kept only if it ever carries something.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn version_text(version: [u32; 4]) -> String {
    let [year, day, build, extra] = version;
    if extra == 0 {
        format!("{year}.{day}.{build}")
    } else {
        format!("{year}.{day}.{build}.{extra}")
    }
}

/// How online recognition names itself to the service, in the form the Codex desktop app uses,
/// with the version installed on this computer. None when the desktop app is not installed, and
/// on macOS, where the recogniser reads the version itself.
pub fn user_agent() -> Option<String> {
    let version = crate::sys::codex_version()?;
    Some(format!("{}/{version} ({}; {})", codex_asr::DEFAULT_ORIGINATOR, std::env::consts::OS, std::env::consts::ARCH))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_in_opens_the_first_available_option() {
        assert_eq!(launch_sign_in(|| Some(true), || panic!("CLI was checked"), || panic!("browser was opened")), Ok(()));
        assert_eq!(launch_sign_in(|| None, || Some(true), || panic!("browser was opened")), Ok(()));
        assert_eq!(launch_sign_in(|| None, || None, || true), Ok(()));
        assert!(launch_sign_in(|| Some(false), || panic!("CLI was checked"), || panic!("browser was opened")).is_err());
    }

    #[test]
    fn the_version_is_read_from_the_package_name() {
        let installed = package_version("OpenAI.Codex_26.1111.2228.0_x64__2p2nqsd0c76g0").unwrap();
        assert_eq!(installed, [26, 1111, 2228, 0]);
        assert_eq!(version_text(installed), "26.1111.2228");
        assert_eq!(version_text([26, 429, 30905, 2]), "26.429.30905.2");
        // The newer of two installed versions is the larger array.
        assert!(installed > package_version("OpenAI.Codex_26.429.30905.0_x64__2p2nqsd0c76g0").unwrap());
        for broken in ["", "OpenAI.Codex", "OpenAI.Codex_26.1111_x64__2p2nqsd0c76g0", "OpenAI.Codex_a.b.c.d_x64__x", "OpenAI.Codex_1.2.3.4.5_x64__x"] {
            assert_eq!(package_version(broken), None, "{broken}");
        }
    }

    #[test]
    fn only_the_message_box_counts() {
        assert!(is_composer("ProseMirror ProseMirror-focused", ""));
        assert!(is_composer("", "prompt-textarea"));
        assert!(!is_composer("search-input", "search"));
    }
}
