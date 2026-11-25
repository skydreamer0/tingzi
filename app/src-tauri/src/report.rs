//! A short record of what the app did, kept on this computer, and the problem report built from it.
//!
//! The record says what happened, such as which recogniser ran, how long it took and why something failed. It never
//! holds what the user said or typed, recordings, keys, or the user's folder names. Nothing is sent anywhere: the
//! report is shown to the user, who may edit it, and is only copied, saved or opened in the browser when they ask.

use crate::settings::{Engine, Settings};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// The record grows to this size, then the older half is kept as `tingzi-log.old.txt` and a new one starts.
const LONGEST_RECORD: u64 = 256 * 1024;
/// How many of the latest lines of the record go into a report.
const REPORT_LINES: usize = 200;
/// GitHub refuses addresses that are too long, so the record is cut to fit this many characters of address.
const LONGEST_ADDRESS: usize = 7000;
const NEW_ISSUE: &str = "https://github.com/skydreamer0/tingzi/issues/new?template=bug-report.yml";

static WRITING: Mutex<()> = Mutex::new(());

fn record() -> PathBuf {
    crate::paths::data().join("tingzi-log.txt")
}

fn older_record() -> PathBuf {
    crate::paths::data().join("tingzi-log.old.txt")
}

/// Adds a line to the record. Callers pass what happened, never the user's text; the line is cleaned again anyway.
/// A record that cannot be written is skipped, so nothing the user does ever fails because of it.
pub fn note(event: impl AsRef<str>) {
    if cfg!(test) { return; }
    let line = format!("{} {}\n", stamp(now()), clean(event.as_ref(), &crate::sys::home().to_string_lossy()).replace('\n', " "));
    let _held = WRITING.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let _ = std::fs::create_dir_all(crate::paths::data());
    if std::fs::metadata(record()).is_ok_and(|file| file.len() > LONGEST_RECORD) {
        let _ = std::fs::rename(record(), older_record());
    }
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(record()) {
        let _ = file.write_all(line.as_bytes());
    }
}

/// Notes a crash with where it happened, before the usual message is printed.
pub fn note_crashes() {
    let usual = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let place = info.location().map(|at| format!("{}:{}", at.file(), at.line())).unwrap_or_default();
        note(format!("程式錯誤 {place}"));
        usual(info);
    }));
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_secs())
}

/// Seconds since 1970 as a date and time in UTC, such as `2026-11-18 05:40:12 UTC`.
fn stamp(seconds: u64) -> String {
    let (days, rest) = (seconds / 86_400, seconds % 86_400);
    // Howard Hinnant's days-to-civil conversion.
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z - era * 146_097;
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 { month_index + 3 } else { month_index - 9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02} UTC", rest / 3600, rest / 60 % 60, rest % 60)
}

/// Takes out what could name the user or open their accounts: the home folder, other users' folders, e-mail
/// addresses, and anything that looks like a key or token.
pub fn clean(text: &str, home: &str) -> String {
    let mut text = text.to_string();
    if home.len() > 3 {
        for form in [home.to_string(), home.replace('\\', "/")] {
            text = replace_ignoring_case(&text, &form, "~");
        }
    }
    let text = hide_user_folders(&text);
    text.split_inclusive(|c: char| c.is_whitespace() || "\"'()<>[]{}=,;「」（）".contains(c))
        .map(|piece| {
            let end = piece.trim_end_matches(|c: char| c.is_whitespace() || "\"'()<>[]{}=,;「」（）".contains(c));
            let (word, tail) = piece.split_at(end.len());
            format!("{}{tail}", hide_word(word))
        })
        .collect()
}

fn replace_ignoring_case(text: &str, find: &str, with: &str) -> String {
    let (lower_text, lower_find) = (text.to_lowercase(), find.to_lowercase());
    // Lowercasing changes lengths for a few characters; then the plain, case-sensitive match is used.
    if lower_text.len() != text.len() || lower_find.len() != find.len() {
        return text.replace(find, with);
    }
    let mut out = String::new();
    let mut from = 0;
    while let Some(at) = lower_text[from..].find(&lower_find) {
        if !text.is_char_boundary(from + at) || !text.is_char_boundary(from + at + find.len()) {
            return text.replace(find, with);
        }
        out.push_str(&text[from..from + at]);
        out.push_str(with);
        from += at + find.len();
    }
    out.push_str(&text[from..]);
    out
}

/// `C:\Users\someone\...` and `/Users/someone/...` lose the name.
fn hide_user_folders(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    loop {
        let found = ["\\Users\\", "/Users/", "/home/"].iter().filter_map(|marker| rest.find(marker).map(|at| (at, *marker))).min();
        let Some((at, marker)) = found else { break };
        let after = at + marker.len();
        out.push_str(&rest[..after]);
        let name_end = rest[after..].find(|c: char| c == '\\' || c == '/' || c.is_whitespace() || c == '"').map_or(rest.len(), |end| after + end);
        if name_end > after {
            out.push('…');
        }
        rest = &rest[name_end..];
    }
    out.push_str(rest);
    out
}

fn hide_word(word: &str) -> String {
    let at = word.find('@');
    let looks_like_mail = at.is_some_and(|at| at > 0 && word[at + 1..].contains('.'));
    // Keys mix capitals, small letters and digits; file and model names do not.
    let keyish = word.len() >= 32
        && word.chars().all(|c| c.is_ascii_alphanumeric() || "-_".contains(c))
        && word.chars().any(|c| c.is_ascii_digit())
        && word.chars().any(|c| c.is_ascii_uppercase())
        && word.chars().any(|c| c.is_ascii_lowercase());
    let prefixed = ["sk-", "ghp_", "gho_", "github_pat_", "xoxb-", "eyJ"].iter().any(|start| word.starts_with(start) && word.len() > start.len() + 6);
    if looks_like_mail {
        "（已隱藏電子郵件）".into()
    } else if keyish || prefixed {
        "（已隱藏金鑰）".into()
    } else {
        word.into()
    }
}

/// The latest lines of the voice window's record and the Zhuyin input method's (`tingzi-ime-log.txt`, written by
/// every program the input method runs in), in time order, older ones first.
fn latest_lines(count: usize) -> Vec<String> {
    let read = |name: &str| std::fs::read(crate::paths::data().join(name)).map(|bytes| String::from_utf8_lossy(&bytes).into_owned()).unwrap_or_default();
    let files = ["tingzi-log.old.txt", "tingzi-log.txt", "tingzi-ime-log.old.txt", "tingzi-ime-log.txt"];
    merged(&files.map(read), count)
}

/// Lines from several records, each already in time order, put in one time order; the last `count` are kept.
/// Every line starts with the same kind of stamp, so text order is time order.
fn merged(records: &[String], count: usize) -> Vec<String> {
    let mut lines: Vec<&str> = records.iter().flat_map(|record| record.lines()).filter(|line| !line.trim().is_empty()).collect();
    // A stable sort keeps the order of lines written in the same second.
    lines.sort_by_key(|line| line.get(..19).unwrap_or_default().to_string());
    let skip = lines.len().saturating_sub(count);
    lines[skip..].iter().map(|line| line.to_string()).collect()
}

fn engine_name(engine: Engine) -> &'static str {
    match engine {
        Engine::Local => "本機 Whisper",
        Engine::Codex => "Codex 線上辨識",
        Engine::Api => "自選雲端服務",
    }
}

/// The user's choices that help find a problem. Addresses are reduced to whether they are the usual one, since a
/// service of the user's own may carry a company or personal name.
fn choices(prefs: &Settings) -> Vec<String> {
    let on = |value: bool| if value { "開" } else { "關" };
    let delivery = match prefs.delivery_mode {
        Some(1) => "只複製",
        Some(2) => "只顯示",
        _ if prefs.auto_insert => "插入並複製",
        _ => "只複製",
    };
    let usual = |url: &str| if url.trim_end_matches('/') == crate::settings::DEFAULT_API_URL { "OpenAI" } else { "自訂網址" };
    let mut lines = vec![format!("辨識方式：{}", engine_name(prefs.engine()))];
    match prefs.engine() {
        Engine::Local => {
            lines.push(format!("本機模型：{}・顯示卡加速：{}", prefs.local_model.as_deref().unwrap_or("預設"), on(prefs.gpu)))
        }
        Engine::Api => lines.push(format!("辨識服務：{}・模型：{}", usual(&prefs.api_url), prefs.api_model)),
        Engine::Codex => lines.push(format!("Codex 版本：{}", crate::sys::codex_version().unwrap_or_else(|| "找不到".into()))),
    }
    lines.push(format!("辨識完成後：{delivery}・按住說話：{}・整理口語：{}・簡體：{}", on(prefs.hold_to_talk), on(prefs.tidy), on(prefs.simplified)));
    lines.push(format!("自訂詞：{}・取代規則：{}・辨識紀錄：{}・從修正學習：{}", on(prefs.use_phrases), on(prefs.use_replacements), on(prefs.keep_history), on(prefs.learn_fixes)));
    if prefs.refine {
        let model = if prefs.refine_model.is_empty() { "未填" } else { &prefs.refine_model };
        lines.push(format!("AI 潤稿：開・{}・模型：{model}", usual(&prefs.refine_url)));
    } else {
        lines.push("AI 潤稿：關".into());
    }
    lines.push(format!("自動更新：{}", on(prefs.check_updates)));
    lines
}

/// What a report says before the record: version, system and the user's choices.
fn heading(prefs: &Settings) -> String {
    let mut lines = vec![
        format!("聽字版本：{}", env!("CARGO_PKG_VERSION")),
        format!("系統：{}（{}）", crate::sys::system_version(), std::env::consts::ARCH),
    ];
    if cfg!(windows) {
        let zhuyin = if crate::paths::word_list().is_some() { "已安裝" } else { "找不到" };
        lines.push(format!("注音輸入法：{zhuyin}"));
    }
    lines.extend(choices(prefs));
    lines.join("\n")
}

/// The whole report as the user sees it before sending: the heading, then the record's latest lines.
pub fn build(prefs: &Settings) -> String {
    let home = crate::sys::home().to_string_lossy().into_owned();
    let lines: Vec<String> = latest_lines(REPORT_LINES).iter().map(|line| clean(line, &home)).collect();
    let record = if lines.is_empty() { "（還沒有記錄）".to_string() } else { lines.join("\n") };
    format!("{}\n\n最近的記錄（時間是 UTC）：\n{record}", clean(&heading(prefs), &home))
}

/// The address of a new issue with the version and the report filled in. A report too long for an address keeps
/// its beginning and its latest lines, and says the whole of it is on the clipboard.
pub fn issue_address(report: &str, version: &str) -> String {
    let start = format!("{NEW_ISSUE}&version={}&logs=", encode(version));
    let fits = |text: &str| start.len() + encode(text).len() <= LONGEST_ADDRESS;
    let fenced = |text: &str| format!("```\n{text}\n```");
    if fits(&fenced(report)) {
        return start + &encode(&fenced(report));
    }
    let lines: Vec<&str> = report.lines().collect();
    let head: Vec<&str> = lines.iter().take_while(|line| !line.is_empty()).copied().collect();
    const CUT: &str = "（記錄太長，這裡只放最新的部分；完整內容已複製到剪貼簿，可以貼在這裡。）";
    // The record starts after the heading and the blank line below it; its oldest lines go first.
    let mut kept = (head.len() + 1).min(lines.len());
    let shortened = |kept: usize| format!("{}\n\n{CUT}\n{}", head.join("\n"), lines[kept..].join("\n"));
    while kept < lines.len() && !fits(&fenced(&shortened(kept))) {
        kept += 1;
    }
    let body = if kept < lines.len() { shortened(kept) } else { format!("{}\n\n{CUT}", head.join("\n")) };
    let body = if fits(&fenced(&body)) { body } else { CUT.to_string() };
    start + &encode(&fenced(&body))
}

/// Percent-encodes text for a web address.
fn encode(text: &str) -> String {
    text.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (byte as char).to_string(),
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

/// The version and system, for the issue's "version" field, such as `0.8.0，Windows 11 24H2`.
pub fn version_line() -> String {
    format!("{}，{}", env!("CARGO_PKG_VERSION"), crate::sys::system_version())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tests_skip_the_production_log_writer() {
        struct NeverFormat;
        impl AsRef<str> for NeverFormat {
            fn as_ref(&self) -> &str { panic!("test event reached the log writer"); }
        }
        note(NeverFormat);
    }

    #[test]
    fn stamps_read_as_dates() {
        assert_eq!(stamp(0), "1970-01-01 00:00:00 UTC");
        assert_eq!(stamp(1_794_980_412), "2026-11-18 05:40:12 UTC");
        assert_eq!(stamp(951_782_400), "2000-02-29 00:00:00 UTC");
    }

    #[test]
    fn reports_lose_names_folders_and_keys() {
        let home = r"C:\Users\Someone";
        assert_eq!(clean(r"無法讀取 C:\Users\Someone\AppData\Local\Tingzi\a.wav", home), r"無法讀取 ~\AppData\Local\Tingzi\a.wav");
        assert_eq!(clean(r"c:\users\someone\x", home), r"~\x");
        assert_eq!(clean(r"D:\Users\Other\x", home), r"D:\Users\…\x");
        assert_eq!(clean("/Users/other/Library", "/Users/me"), "/Users/…/Library");
        assert_eq!(clean("寄給 person@example.com 了", home), "寄給 （已隱藏電子郵件） 了");
        assert_eq!(clean("key=sk-proj-abcdefghij1234567890", home), "key=（已隱藏金鑰）");
        assert_eq!(clean("Bearer AbCdEfGhIjKlMnOpQrStUvWxYz012345", home), "Bearer （已隱藏金鑰）");
        assert_eq!(clean("辨識完成 1234 ms・本機 Whisper", home), "辨識完成 1234 ms・本機 Whisper");
        assert_eq!(clean("ggml-large-v3-turbo-q5_0.bin", home), "ggml-large-v3-turbo-q5_0.bin");
    }

    #[test]
    fn both_records_are_read_in_time_order() {
        let voice = "2026-11-18 05:40:12 UTC 啟動\n2026-11-18 05:41:00 UTC 錄音中\n".to_string();
        let zhuyin = "2026-11-18 05:40:30 UTC 注音（notepad.exe）：啟用\n".to_string();
        let lines = merged(&[voice, String::new(), zhuyin], 2);
        assert_eq!(lines, ["2026-11-18 05:40:30 UTC 注音（notepad.exe）：啟用", "2026-11-18 05:41:00 UTC 錄音中"]);
    }

    #[test]
    fn addresses_fill_the_issue_form_and_stay_short() {
        let short = issue_address("聽字版本：0.8.0\n\n最近的記錄：\na", "0.8.0，Windows 11");
        assert!(short.starts_with(NEW_ISSUE));
        assert!(short.contains("&version=0.8.0%EF%BC%8CWindows%2011&logs=%60%60%60%0A"));
        let long = format!("聽字版本：0.8.0\n\n最近的記錄：\n{}", (0..500).map(|n| format!("第 {n} 行 辨識完成")).collect::<Vec<_>>().join("\n"));
        let address = issue_address(&long, "0.8.0");
        assert!(address.len() <= LONGEST_ADDRESS);
        assert!(address.contains(&encode("第 499 行")));
        assert!(!address.contains(&encode("第 0 行 ")));
        assert!(address.contains(&encode("完整內容已複製到剪貼簿")));
    }
}
