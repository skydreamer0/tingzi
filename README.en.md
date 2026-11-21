<div align="center">

[繁體中文](README.md) · **English**

<img src="assets/brand/tingzi.svg" width="96" height="96" alt="The Tingzi ear-key icon">

# Tingzi｜聽字

**Speak it, then fix the wrong characters with Zhuyin.**

Traditional Chinese voice and Zhuyin (Bopomofo) input for Windows.

[![Latest release](https://img.shields.io/github/v/release/skydreamer0/tingzi?label=release)](https://github.com/skydreamer0/tingzi/releases/latest)
[![Platform](https://img.shields.io/badge/platform-Windows%2010%20%2F%2011%20x64-0078D4)](#download-and-install)
[![License](https://img.shields.io/github/license/skydreamer0/tingzi)](LICENSE)
[![Windows build](https://github.com/skydreamer0/tingzi/actions/workflows/windows.yml/badge.svg)](https://github.com/skydreamer0/tingzi/actions/workflows/windows.yml)
[![macOS build](https://github.com/skydreamer0/tingzi/actions/workflows/macos.yml/badge.svg)](https://github.com/skydreamer0/tingzi/actions/workflows/macos.yml)

[Download](https://github.com/skydreamer0/tingzi/releases/latest) ·
[Windows manual (Chinese)](windows/README.md) ·
[macOS (in progress)](macos/README.md) ·
[Report a problem](https://github.com/skydreamer0/tingzi/issues) ·
[Contributing](CONTRIBUTING.md)

</div>

![Three steps: click the Tingzi icon and speak, the text appears at the cursor, fix homophones with Zhuyin](docs/images/how-it-works.svg)

Put the cursor in Notepad, a browser or any text field, click the Tingzi icon at the edge of the screen and speak. When you finish, the text is inserted at the cursor. Speech recognition often picks the wrong character among Chinese homophones. When it does, switch to Tingzi's built-in Zhuyin input method and retype that character in place. You don't need another tool or a separate input method.

The app itself, its settings and the detailed manuals are in Traditional Chinese. Labels quoted below are given in Chinese with an English gloss.

## Why Tingzi

- **Voice and Zhuyin in one tool**: dictate long passages, fix homophones with Zhuyin. Once you correct a word, later dictations spell it the same way.
- **You decide where your audio goes**: recognize entirely on your own computer (local Whisper, including Breeze ASR 25, which is tuned for Taiwanese Mandarin), or use an online service you pick.
- **Built for Traditional Chinese**: Chinese punctuation is added and fillers such as 嗯 and 呃 are removed automatically. Zhuyin supports the Standard (Dachen) and Eten layouts, and long phrases are converted using context.
- **Open source**: the code is released under the MIT License. The built-in Zhuyin dictionary comes from McBopomofo's public word list.

## Download and install

| Platform | Status | How to get it |
| --- | --- | --- |
| Windows 10/11 (x64) | Preview | Download `Tingzi-Setup-<version>-x64.exe` from [Releases](https://github.com/skydreamer0/tingzi/releases/latest) (a ZIP up to 0.7.7) |
| macOS 11 or later | In progress, not yet tested on a real Mac | Voice input only, no Zhuyin; from 0.10.0 onward, download `Tingzi-<version>-macos.dmg` from [Releases](https://github.com/skydreamer0/tingzi/releases/latest) and drag it to Applications; see the [macOS notes](macos/README.md) |

**Installing on Windows**

1. Download `Tingzi-Setup-<version>-x64.exe` from [Releases](https://github.com/skydreamer0/tingzi/releases/latest).
2. Double-click it and follow the steps; choose Yes when Windows asks whether to allow changes.
3. When it finishes, the voice window appears at the right of the screen and walks you through choosing a microphone and a recognition method. Press Windows+Space to switch to 聽字・注音與語音輸入法.

While setup runs, it shows when it is unpacking files, copying files and registering the input method, or adding Tingzi to your keyboards. Setup adds Tingzi to your keyboards (adding Chinese (Taiwan) if it is missing) and downloads the Microsoft Edge WebView2 Runtime if it is missing. You don't need .NET. Running a newer installer on a computer that has Tingzi upgrades it; to uninstall, open Windows Settings → Apps → Installed apps and choose Uninstall. Versions up to 0.7.7 came as a ZIP with `InstallTingzi.cmd`. See the [Windows manual](windows/README.md) for other details.

> [!NOTE]
> This is a preview. The installer is not code-signed, so Windows may show "Windows protected your PC"; choose More info, then Run anyway. Each release includes a `.sha256` file you can use to check the download.
>
> The macOS app is signed locally only. It has no Developer ID and is not notarized by Apple, so macOS blocks it the first time; open System Settings > Privacy & Security and choose Open Anyway (on macOS 14 or earlier, right-click it in Finder and choose Open).

## Quick start

1. Click in the body of a Notepad document so the cursor is where you want the text.
2. Click the Tingzi icon at the edge of the screen, or press **Ctrl+Alt+R**, and start speaking. Press it again to stop.
3. The text is inserted at the cursor and also copied to the clipboard.
4. If a homophone came out wrong, press **Windows+Space** and switch to 聽字・注音與語音輸入法 (Tingzi Zhuyin and Voice Input). Delete the wrong character, type its Zhuyin in place, press **↓** to choose a character, then press **Enter**.

If you'd rather hold a key while you talk and send on release, choose 按住說話，放開送出 (hold to talk, release to send) on the 一般 (General) settings page. The recording shortcut can be changed on the same page.

## Features

### First-run setup and updates

- **First-run setup**: the first time you open Tingzi, the settings window walks you through choosing and testing a microphone, choosing a recognition method, and dictating one sentence in Notepad and fixing one character with Zhuyin. You can skip any step and start again later from the 關於 (About) page.
- **Automatic updates (Windows)**: Tingzi checks for a new version at most once a day. When it finds one, it downloads and verifies the installer in the background and asks before installing; your settings and data are kept. You can turn this off on the 關於 (About) page and check manually instead.

### Voice input

![The voice bar, the edit card shown after a field change, and its six states](docs/images/voice-bar.svg)

- **An always-available bar**: normally just an icon at the screen edge, which you can drag to the left or right side. It expands while recording, recognizing and refining, and shows whether the current sentence is being recognized online or locally. The editor and bar fold back toward the edge where the icon sits.
- **No pasting into the wrong field**: if you move to another field while recording, the text isn't pasted there. It stays in an editable card and is already on the clipboard.
- **Spoken-language cleanup**: removes leading fillers such as 嗯 and 呃 and stuttered repeats, and adds Chinese punctuation, without rewriting the sentence.
- **Custom words and replacement rules**: names and terms you add are spelled correctly first. For words that are always misheard, or English capitalization, add rules of the form "what was heard → what to write".
- **Learns from your fixes**: correct a dictated word once with Zhuyin, and later dictations with the same sound will use that word.
- **AI refinement (optional, off by default)**: sends the recognized text to an OpenAI-compatible service you configure, to tidy self-corrections, filler words and punctuation. The bar says whether each dictation was refined, history keeps the text from before refinement, and if the service fails, the original text is inserted.
- **Recording shortcut**: **Ctrl+Alt+R** by default. Change it on the settings page by pressing the combination you want; F1 to F24 can be used on their own.
- **History**: the last 50 results are kept only on your computer and can be recovered from the bar. **Alt+Shift+Z** pastes the previous one again.
- **Per-app settings**: choose Traditional or Simplified output, cleanup, refinement, and what happens after recognition, for each program.

### Zhuyin input method

- **Built-in Zhuyin engine**: no other input method is needed. Supports the Standard (Dachen) and Eten layouts.
- **Whole-phrase conversion**: characters are chosen by how common each word is and which words usually follow each other, so 程式編輯 (program editing) doesn't become 城市編輯 (city editing).
- **Fix one character mid-phrase**: while composing, move with ← → to the character you want to change. Tab forces a word break; while choosing, Tab shows near-sounding characters.
- **Punctuation, symbols and templates**: Shift plus a punctuation key gives full-width punctuation, and `` ` `` opens the symbol table even when not composing. Press Up after a symbol still in the composition to replace it, or Down after a bracket for alternatives on the same side. Frequently used addresses or replies can be saved as templates.
- **First-key recall**: type only the first Zhuyin symbol of each character and press Space to bring back a sentence you've typed before (requires local learning).

The full key table and settings reference are in the [Windows manual](windows/README.md) (Chinese).

### Other

- **Traditional or Simplified output**: both voice results and Zhuyin text follow the setting. The bar and the tray menu can also convert the clipboard between the two.
- **Follows the system look**: dark and light modes, and the Windows accent color.
- **Settings apply immediately**: five pages (一般 General, 語音 Voice, 注音 Zhuyin, 隱私 Privacy, 關於 About), with no Save button.

## Recognition and privacy

Choose a recognition method on the 語音 (Voice) settings page. Where your audio goes depends on the method:

| Method | Where the audio goes | What you need |
| --- | --- | --- |
| Local Whisper | Stays on your computer | Download a model and whisper.cpp from the settings page with one click; the source, size and license are listed before downloading, and models you no longer need can be deleted; you do not need to install the Visual C++ runtime separately. With an NVIDIA GPU you can also download an acceleration package |
| Your own online service | The OpenAI-compatible transcription service you enter (for example OpenAI or Groq), along with your custom words | Your own API key, billed by that service. If the URL is `localhost`, the audio stays on your computer |
| Codex online recognition (unofficial) | ChatGPT | A working Codex sign-in on this computer |

> [!WARNING]
> Codex online recognition uses [codex-asr](https://crates.io/crates/codex-asr), an unofficial third-party component that reuses the ChatGPT sign-in of Codex on this computer to transcribe your audio. It is not provided or supported by OpenAI and may stop working at any time if OpenAI changes the service. If you need it to work reliably, use local Whisper or your own online service instead.

With AI refinement on, Tingzi sends the recognized **text** (not the audio), with your custom words and any notes you wrote. With it off, nothing is sent. API keys are stored in Windows Credential Manager, not in the settings file. History, learned words and local learning data stay on your computer and can be cleared on the 隱私 (Privacy) page.

While automatic updates are on (關於 About page), Tingzi asks GitHub at most once a day whether a new version exists, sending only the current version number. With them off, it doesn't connect on its own.

Tingzi keeps an activity record, `tingzi-log.txt`, in your personal folder, to help with problem reports. It notes when and how a recording started (shortcut, window button or menu), which program the text was for, which method was used, how long it took, and what failed or crashed. The Zhuyin input method keeps its own record, `tingzi-ime-log.txt`, noting which programs it ran in, keys it could not take and compositions a program cut short; it never notes which keys you pressed or what you typed. It holds nothing you said or typed, no audio and no keys, and stays on your computer; past 256 KB it starts a new file and keeps only one older one. Press 回報問題… (Report a problem) on the 關於 (About) page or in the tray menu to see what a report would include and edit it. Only 到 GitHub 回報 (Report on GitHub) opens the issue page in your browser, and you decide whether to submit it.

## Documentation

The documents below are in Traditional Chinese.

| I want to… | Read |
| --- | --- |
| Install, upgrade, uninstall, or look up keys | [Windows manual](windows/README.md) |
| Build or try the macOS version in progress | [macOS notes](macos/README.md) |
| Build, test, or find my way around the source | [Developer guide](docs/development/README.md) |
| See what's planned | [General user experience spec](docs/specs/general-user-experience.md) |
| Browse everything | [Documentation index](docs/README.md) |

## Contributing

Bug reports and changes are welcome. The easiest way to report a problem is 回報問題… (Report a problem) on the 關於 (About) settings page: Tingzi gathers the version, system, settings and recent activity record, and you add the steps you took and what you expected and saw. Use made-up examples instead of real recordings or private word lists. See the [contributing guide](CONTRIBUTING.md) for the process. Please don't report security issues in public; follow the [security policy](SECURITY.md) instead.

Tingzi has two parts. `windows/` is the Windows text service (the Zhuyin input method), written in C++. `app/` is the voice window and settings, written in Rust with Tauri 2. With Rust and the Visual Studio C++ tools installed on Windows, you can start by running the voice window's tests:

```powershell
cd app/src-tauri
cargo test
```

Full build and packaging steps are in the [developer guide](docs/development/README.md).

## Acknowledgments

- [McBopomofo](https://github.com/openvanilla/McBopomofo): the built-in Zhuyin word data (MIT License)
- [whisper.cpp](https://github.com/ggml-org/whisper.cpp) and the [Whisper](https://github.com/openai/whisper) models: local speech recognition
- [Breeze ASR 25](https://huggingface.co/MediaTek-Research/Breeze-ASR-25) by MediaTek Research: a recognition model for Taiwanese Mandarin and mixed Chinese–English speech
- [codex-asr](https://crates.io/crates/codex-asr): Codex online recognition
- [Tauri](https://tauri.app/): the voice window and settings
- [Chinese Wikipedia](https://zh.wikipedia.org/): the word-pair statistics used for whole-phrase conversion (`pairs.txt`) were counted from its articles

## License

Tingzi's code is released under the [MIT License](LICENSE). The following are under their own licenses:

- The Zhuyin word data comes from McBopomofo, under the MIT License.
- The word-pair statistics in `windows/standalone-data/pairs.txt` were counted from Chinese Wikipedia and are licensed under [CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/); see `pairs-NOTICE.txt` in the same folder.
- Other third-party components are listed in `THIRD-PARTY-NOTICES.txt` in the package.

Apart from the McBopomofo word data above, Tingzi contains no other input method's code or dictionaries. Tingzi is an independent project and is not affiliated with OpenAI, Microsoft or Apple.
