//! Where the installed program and the user's data live.

use std::path::PathBuf;

/// The user's settings, words and window position.
pub fn data() -> PathBuf {
    PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap_or_default()).join("Tingzi")
}

/// The installed folder that holds the word list and the Zhuyin tools.
/// The app sits one level below it when installed; a development build uses the newest installed version.
pub fn install() -> Option<PathBuf> {
    let beside = std::env::current_exe().ok().and_then(|exe| Some(exe.parent()?.parent()?.to_path_buf()));
    if let Some(folder) = beside.filter(|folder| folder.join("phrases.txt").is_file()) {
        return Some(folder);
    }
    let programs = PathBuf::from(std::env::var_os("ProgramFiles")?).join("Tingzi");
    let mut versions: Vec<(Vec<u32>, PathBuf)> = std::fs::read_dir(programs)
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let numbers: Option<Vec<u32>> = name.strip_prefix('v')?.split('.').map(|part| part.parse().ok()).collect();
            Some((numbers?, entry.path()))
        })
        .collect();
    versions.sort();
    versions.pop().map(|(_, folder)| folder)
}

pub fn word_list() -> Option<PathBuf> {
    install().map(|folder| folder.join("phrases.txt")).filter(|file| file.is_file())
}

/// A tool shipped beside the word list, such as the keyboard switcher.
pub fn tool(name: &str) -> Option<PathBuf> {
    install().map(|folder| folder.join(name)).filter(|file| file.is_file())
}
