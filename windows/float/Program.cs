using System.Diagnostics;
using System.Runtime.InteropServices;
using VoiceInput.App.Services;
using VoiceInput.Core.Recognition;
using VoiceInput.Protocol;

internal static class Program
{
    static LocalWhisperRecognitionEngine Engine()
    {
        var config = Path.Combine(AppContext.BaseDirectory, "voice-root.txt");
        var work = File.Exists(config) ? File.ReadAllText(config).Trim() : "";
        if (!Path.IsPathFullyQualified(work)) throw new InvalidOperationException("請先設定本機語音模型位置");
        return new LocalWhisperRecognitionEngine(Path.Combine(work, @"whisper-v1.9.2\Release\whisper-cli.exe"),
            Path.Combine(work, @"downloads\ggml-base.bin"));
    }

    static Task<RecognitionResult> Recognize(string path, bool online, CancellationToken token) =>
        Task.Run(() => {
            IRecognitionEngine engine = online ? new OnlineRecognition() : Engine();
            return engine.TranscribeAsync(path, new RecognitionOptions(Language: "zh"), token);
        }, token);

    [STAThread]
    static int Main(string[] args)
    {
        if (CodexFocusProbe.IsProbe(args)) return CodexFocusProbe.Run(args);
        if (args.Length == 1 && AudioImport.IsRetryAction(args[0])) return AudioImport.Send(args[0]) ? 0 : 4;
        if (args.Length == 2 && args[0] is "--check-audio" or "--check-online") {
            try {
                var text = Recognize(args[1], args[0] == "--check-online", CancellationToken.None).GetAwaiter().GetResult().Text;
                Console.WriteLine($"recognized_chars={text.Length}");
                return string.IsNullOrWhiteSpace(text) ? 2 : 0;
            } catch (Exception ex) { Console.Error.WriteLine(ex.Message); return 3; }
        }
        bool import = args.Length >= 1 && args[0] == "--open-wav";
        bool settingsRequested = args.Length == 1 && args[0] == "--settings";
        bool showRequested = args.Length == 1 && args[0] == "--show";
        bool editRequested = args.Length == 1 && args[0] == "--edit";
        var requestedFile = import && args.Length == 2 ? Path.GetFullPath(args[1]) : "";
        if (args.Length != 0 && !showRequested && !settingsRequested && !editRequested && (!import || args.Length > 2 || (requestedFile.Length != 0 && !AudioImport.IsWave(requestedFile)))) return 2;
        using var single = new Mutex(true, @"Local\TingziFloat020", out bool first);
        using var show = new EventWaitHandle(false, EventResetMode.AutoReset, @"Local\TingziFloatShow020");
        using var configure = new EventWaitHandle(false, EventResetMode.AutoReset, @"Local\TingziVoiceSettings");
        using var editor = new EventWaitHandle(false, EventResetMode.AutoReset, @"Local\TingziEditPreview");
        if (!first) { if (import) return AudioImport.Send(requestedFile) ? 0 : 4; (settingsRequested ? configure : editRequested ? editor : show).Set(); return 0; }
        using var zh = new EventWaitHandle(false, EventResetMode.AutoReset, @"Local\TingziChinese");
        using var en = new EventWaitHandle(false, EventResetMode.AutoReset, @"Local\TingziEnglish");
        Application.SetHighDpiMode(HighDpiMode.PerMonitorV2);
        Application.EnableVisualStyles();
        SynchronizationContext.SetSynchronizationContext(new WindowsFormsSynchronizationContext());
        using var menu = new ContextMenuStrip();
        var root = Directory.GetParent(AppContext.BaseDirectory.TrimEnd(Path.DirectorySeparatorChar))!.FullName;
        var state = CapsuleState.Idle;
        var mode = "中文";
        string? preview = null;
        bool autoExpandPreview = true;
        WavAudioRecorder? recorder = null;
        VoiceInsertionTarget? target = null;
        string? retryPath = null, importedCopy = null;
        CancellationTokenSource? pending = null;
        var clock = new Stopwatch();
        float level = 0;
        bool stopping = false;
        bool dialogOpen = false, importing = false, microphoneResult = false;
        VoiceDeliveryMode recordingDelivery=VoiceDeliveryMode.Auto;
        VoiceSettings prefs = VoiceSettings.Load();
        LayeredCapsuleWindow capsule = null!;
        using var draft = new CodexDraft(() => preview, pasted =>
            capsule.Update(new CapsuleSnapshot(CapsuleState.Ready, pasted ? "已送出貼上鍵" : "點選複製，再到草稿貼上", preview) { AutoExpandPreview=autoExpandPreview }));
        void Update(CapsuleState next, string message, string? text = null, bool? expandPreview = null)
        {
            state = next; preview = text;
            if (expandPreview.HasValue) autoExpandPreview=expandPreview.Value;
            draft.Enable(next == CapsuleState.Ready && !string.IsNullOrEmpty(text));
            capsule.Update(new CapsuleSnapshot(next, message, text) { AutoExpandPreview=autoExpandPreview });
        }
        void Idle() { capsule.SetRecognizer(prefs.Online ? "Codex 線上" : "本機 Whisper"); Update(CapsuleState.Idle, "聽字・" + mode); }
        void RefreshMicrophone() => capsule.SetMicrophone(Microphones.Resolve(prefs.MicrophoneId)?.Name
            ?? ((prefs.MicrophoneName ?? "預設麥克風") + "（未連接）"));
        void ReleaseAudio()
        {
            target?.Dispose();
            target = null;
            retryPath = null;
            recorder?.Dispose(); recorder = null;
            if (importedCopy != null) {
                try { File.Delete(importedCopy); } catch (IOException) { }
                importedCopy = null;
            }
        }
        void Copy(string text)
        {
            try { Clipboard.SetText(text); capsule.ShowCopyConfirmation(); }
            catch (ExternalException) { capsule.ShowCopyFailure(); }
        }
        async Task Transcribe(string path, bool online)
        {
            void OfferLocal(string reason) {
                retryPath = path;
                Update(CapsuleState.RetryLocal, reason);
            }
            using var cancel = new CancellationTokenSource();
            pending = cancel;
            capsule.SetRecognizer(online ? "Codex 線上" : "本機 Whisper");
            Update(CapsuleState.Transcribing, online ? "線上辨識中・點一下取消" : "本機辨識中・點一下取消");
            try {
                var result = await Recognize(path,online,cancel.Token);
                cancel.Token.ThrowIfCancellationRequested();
                var heard = result.Text;
                var wordList = Path.Combine(root, "phrases.txt");
                if (prefs.UsePhrases && !string.IsNullOrWhiteSpace(heard) && File.Exists(wordList)) {
                    // Whisper may answer in Simplified characters; the word list and the user's words are Traditional.
                    var own = UserPhrases.Load();
                    if (own.Count > 0 && await UserPhrases.SharedLexicon(wordList) is { } lexicon)
                        heard = UserPhrases.Correct(ChineseScript.Convert(heard, false), own, lexicon);
                }
                var spoken = ChineseScript.Convert(heard, prefs.Simplified);
                if (online && string.IsNullOrWhiteSpace(spoken)) {
                    OfferLocal("線上未回傳文字");
                }
                else if (string.IsNullOrWhiteSpace(spoken)) Update(CapsuleState.Error, "本機未辨識到文字，點一下重新錄音");
                else {
                    if (microphoneResult) {
                        var recordingTarget=target;
                        try {
                            var delivered=await VoiceTextDelivery.DeliverAsync(spoken,recordingDelivery,recordingTarget,
                                cancel.Token,() => ReferenceEquals(target,recordingTarget) && ReferenceEquals(pending,cancel));
                            cancel.Token.ThrowIfCancellationRequested();
                            Update(CapsuleState.Ready,delivered.Message,spoken,delivered.AutoExpandPreview);
                        } catch (Exception ex) when (ex is not OperationCanceledException) {
                            Update(CapsuleState.Ready,"文字已保留預覽，請手動複製",spoken,true);
                        }
                    } else {
                        Update(CapsuleState.Ready,"辨識完成・點文字編輯",spoken,true);
                    }
                }
            } catch (OperationCanceledException) { Idle(); }
            catch (Exception ex) when (online) {
                OfferLocal(ex is InvalidOperationException ? ex.Message : "線上回覆無法讀取");
            }
            catch (InvalidOperationException ex) { Update(CapsuleState.Error, ex.Message); }
            catch (Exception) { Update(CapsuleState.Error, "辨識失敗，請檢查音檔或辨識設定"); }
            finally { pending = null; if (retryPath == null) ReleaseAudio(); }
        }
        bool starting = false;
        async void ToggleRecording()
        {
            if (dialogOpen || starting) return;
            if (pending != null) { pending.Cancel(); return; }
            if (stopping || importing) return;
            try {
                if (state == CapsuleState.RetryLocal && retryPath != null) {
                    var path = retryPath;
                    retryPath = null;
                    await Transcribe(path, false);
                } else if (state == CapsuleState.Recording && recorder != null) {
                    stopping = true;
                    var wav = await recorder.StopAsync();
                    await Transcribe(wav, prefs.Online);
                } else {
                    starting = true;
                    var source = Microphones.Resolve(prefs.MicrophoneId);
                    if (source == null) {
                        RefreshMicrophone();
                        Update(CapsuleState.Error, (prefs.MicrophoneName ?? "預設麥克風") + "未連接，請開語音設定");
                        return;
                    }
                    capsule.SetRecognizer(prefs.Online ? "Codex 線上" : "本機 Whisper");
                    microphoneResult=true; recordingDelivery=prefs.EffectiveDeliveryMode;
                    target = recordingDelivery == VoiceDeliveryMode.Auto ? VoiceInsertionTarget.Capture() : null;
                    recorder = new WavAudioRecorder(source);
                    recorder.AudioLevelChanged += value => Volatile.Write(ref level, value);
                    await recorder.StartAsync();
                    capsule.SetMicrophone(recorder.DeviceLabel);
                    clock.Restart(); capsule.Show(); Update(CapsuleState.Recording, "錄音中，點一下停止");
                }
            } catch (Exception) {
                ReleaseAudio();
                Update(CapsuleState.Error, "無法收音，請檢查麥克風");
            } finally { starting = false; stopping = false; }
        }
        using var shortcut = new RecordingShortcut(ToggleRecording);
        bool bound = shortcut.Apply(prefs);
        void Settings() {
            if (capsule.SettingsOpen) { capsule.ActivateSettings(); return; }
            if (dialogOpen) return;
            if (starting || stopping || importing || state is CapsuleState.Recording or CapsuleState.Transcribing) return;
            dialogOpen=true;
            try {
                bool hasKeyboardEngine = File.Exists(Path.Combine(root, "Tingzi.cin"));
                var form = VoiceDialogs.CreateSettings(prefs,bound,next => {
                    if (!next.Valid) return "Ctrl+Alt+V 用於貼入草稿，請選其他錄音鍵。";
                    if (!shortcut.Apply(next)) return "此快捷鍵已被占用，請選其他組合。";
                    try { next.Save(); }
                    catch (Exception ex) when (ex is IOException or UnauthorizedAccessException) {
                        bound=shortcut.Apply(prefs); return "無法儲存設定。";
                    }
                    prefs=next; bound=true; capsule.SetTextSize(prefs.TextSize); ApplyTheme();
                    if (state == CapsuleState.Idle) {
                        capsule.SetRecognizer(prefs.Online ? "Codex 線上" : "本機 Whisper");
                        RefreshMicrophone();
                    }
                    return null;
                }, new SettingsActions(
                    () => Run("TingziKeyboard.exe","switch"),
                    () => Run("TingziPad.exe", ""),
                    () => {
                        try { Process.Start(new ProcessStartInfo("ms-settings:regionlanguage") { UseShellExecute=true }); }
                        catch (Exception ex) when (ex is System.ComponentModel.Win32Exception or InvalidOperationException) {
                            MessageBox.Show("無法開啟 Windows 設定，請從開始功能表開啟語言與鍵盤設定。","聽字・注音與語音輸入法");
                        }
                    }, hasKeyboardEngine && File.Exists(Path.Combine(root,"TingziKeyboard.exe")),
                    hasKeyboardEngine && File.Exists(Path.Combine(root,"TingziPad.exe")), RecognitionStatus(),
                    File.Exists(Path.Combine(root,"TingziLearning.exe")) ? async enabled => (await LearningCommand(enabled ? "on" : "off")).Success ? null : "無法變更本機學習，請檢查工具與資料權限。" : null,
                    File.Exists(Path.Combine(root,"TingziLearning.exe")) ? async () => {
                        var result=await LearningCommand("status");
                        return result.Success && result.Output.Trim() is "Local learning: on" or "Local learning: off" ? result.Output.Trim().EndsWith(" on",StringComparison.Ordinal) : (bool?)null;
                    } : null,
                    File.Exists(Path.Combine(root,"TingziLearning.exe")) ? async arguments => (await LearningCommand(arguments)).Success : null,
                    File.Exists(Path.Combine(root,"phrases.txt")) ? Path.Combine(root,"phrases.txt") : null));
                form.FormClosed += (_,_) => dialogOpen=false;
                capsule.ShowSettings(form);
            } catch { dialogOpen=false; throw; }
        }
        void Edit() {
            if (dialogOpen || state != CapsuleState.Ready || preview == null) return;
            dialogOpen=true; draft.Enable(false);
            try {
                var revised=VoiceDialogs.Edit(preview,prefs.TextSize);
                if (revised != null) {
                    if (string.IsNullOrWhiteSpace(revised)) Idle();
                    else Update(CapsuleState.Ready,"文字已更新",revised);
                }
            } finally { dialogOpen=false; draft.Enable(preview != null); }
        }
        void Hide() => capsule.DockRight();
        void Discard()
        {
            if (stopping && pending == null) return;
            pending?.Cancel();
            ReleaseAudio();
            Idle();
            capsule.DockRight();
        }
        using var window = new LayeredCapsuleWindow(ToggleRecording, Settings, Discard, Copy, Edit);
        capsule = window;
        capsule.SetTextSize(prefs.TextSize);
        using var trayIcon = Icon.ExtractAssociatedIcon(Application.ExecutablePath);
        using var tray = new NotifyIcon { Icon = trayIcon, Text="聽字・注音與語音輸入法", ContextMenuStrip=menu, Visible=true };
        tray.MouseClick+=(_,e)=>{ if(e.Button==MouseButtons.Left) Settings(); };
        void Item(string label, Action action, string? icon = null) => menu.Items.Add(label, null, (_, _) => action()).Tag = icon;
        void Run(string name, string arg) {
            try { Process.Start(new ProcessStartInfo(Path.Combine(root,name), arg) { UseShellExecute = false, CreateNoWindow = true }); }
            catch (Exception ex) when (ex is System.ComponentModel.Win32Exception or InvalidOperationException or IOException) {
                MessageBox.Show("無法開啟工具，請確認完整套件已安裝。", "聽字・注音與語音輸入法", MessageBoxButtons.OK, MessageBoxIcon.Information);
            }
        }
        async Task<(bool Success,string Output)> LearningCommand(params string[] arguments) {
            try {
                var start=new ProcessStartInfo(Path.Combine(root,"TingziLearning.exe")) { UseShellExecute=false, CreateNoWindow=true, RedirectStandardOutput=true };
                foreach (var argument in arguments) start.ArgumentList.Add(argument);
                using var process=Process.Start(start);
                if (process == null) return (false,"");
                var output=await process.StandardOutput.ReadToEndAsync();
                await process.WaitForExitAsync();
                return (process.ExitCode == 0,output);
            } catch (Exception ex) when (ex is System.ComponentModel.Win32Exception or InvalidOperationException or IOException) { return (false,""); }
        }
        string RecognitionStatus() {
            try {
                var config=Path.Combine(AppContext.BaseDirectory,"voice-root.txt");
                var work=File.Exists(config) ? File.ReadAllText(config).Trim() : "";
                return Path.IsPathFullyQualified(work) && File.Exists(Path.Combine(work,@"whisper-v1.9.2\Release\whisper-cli.exe")) && File.Exists(Path.Combine(work,@"downloads\ggml-base.bin"))
                    ? "本機 Whisper 與模型已就緒。"
                    : "本機 Whisper 或模型尚未就緒。請先完成本機語音模型安裝。";
            } catch (Exception ex) when (ex is IOException or UnauthorizedAccessException) { return "無法讀取本機語音模型設定。請確認安裝位置與權限。"; }
        }
        Item("開啟設定…", Settings, "settings");
        Item("顯示／找回浮窗", capsule.Recover, "float");
        Item("開始／停止錄音", ToggleRecording, "mic");
        async void Import(string path) {
            if (dialogOpen || importing || state is CapsuleState.Recording or CapsuleState.RetryLocal || pending != null || stopping) return;
            importing = true;
            try {
                if (path.Length == 0) {
                    using var open = new OpenFileDialog { Filter = "WAV 音檔|*.wav", Title = "聽字・選擇本機音檔" };
                    if (open.ShowDialog() != DialogResult.OK) return;
                    path = open.FileName;
                }
                if (!AudioImport.IsWave(path)) { Update(CapsuleState.Error, "請選本機的 WAV 音檔"); return; }
                string confirmImport = string.IsNullOrEmpty(Environment.GetEnvironmentVariable("AUDIO_CODEX_ASR_URL"))
                    ? "將這個 WAV 經 Codex ASR 傳到 ChatGPT 辨識？"
                    : "將這個 WAV 交給設定的本機 Codex ASR 橋接辨識？橋接可能會轉送至線上服務。";
                if (prefs.Online && MessageBox.Show(confirmImport,"聽字・注音與語音輸入法",
                    MessageBoxButtons.OKCancel, MessageBoxIcon.Question, MessageBoxDefaultButton.Button2) != DialogResult.OK) return;
                if (prefs.Online) {
                    var directory = Directory.CreateDirectory(Path.Combine(Path.GetTempPath(), "TingziVoice"));
                    importedCopy = Path.Combine(directory.FullName,$"voice-{Guid.NewGuid():N}.wav");
                    File.Copy(path, importedCopy);
                    path = importedCopy;
                }
                microphoneResult=false;
                capsule.Show(); await Transcribe(path, prefs.Online);
            } catch (Exception ex) when (ex is IOException or UnauthorizedAccessException) {
                ReleaseAudio(); Update(CapsuleState.Error, "無法讀取或暫存這個音檔");
            } finally { importing = false; }
        }
        Item("辨識本機音檔…", () => Import(""), "audio-file");
        var retryItem = menu.Items.Add("用同一段錄音改跑本機", null, (_, _) => ToggleRecording());
        var discardItem = menu.Items.Add("捨棄這段錄音", null, (_, _) => Discard());
        retryItem.Tag = "retry"; discardItem.Tag = "trash";

        Item("編輯辨識文字…", Edit, "edit");
        Item("複製辨識文字", () => { if (preview != null) Copy(preview); }, "copy");
        void ConvertClipboard(bool simplified) {
            try { if (Clipboard.ContainsText()) Copy(ChineseScript.Convert(Clipboard.GetText(), simplified)); }
            catch (System.Runtime.InteropServices.ExternalException) { }
        }
        Item("剪貼簿文字轉成繁體", () => ConvertClipboard(false), "convert");
        Item("剪貼簿文字轉成簡體", () => ConvertClipboard(true), "convert");
        var hint = menu.Items.Add("在 Codex 草稿按 Ctrl+Alt+V 貼入"); hint.Enabled = false;
        menu.Opening += (_, _) => {
            retryItem.Visible = discardItem.Visible = state == CapsuleState.RetryLocal;
            if (preview != null) hint.Text = draft.Enable(true) ? "在 Codex 草稿按 Ctrl+Alt+V 貼入" : "請複製後在草稿按 Ctrl+V";
        };
        menu.Items.Add(new ToolStripSeparator());
        Item("切換到注音輸入法", () => Run("TingziKeyboard.exe","switch"), "keyboard");
        Item("啟用本機學習", () => Run("TingziLearning.exe","on"), "learn");
        Item("關閉本機學習", () => Run("TingziLearning.exe","off"), "learn");
        Item("收合為右側麥克風", Hide);
        Item("結束程式", Application.Exit, "exit");
        void ApplyTheme() { var colors = prefs.Colors; colors.Apply(menu); capsule.SetDarkMode(colors.Dark); }
        ApplyTheme();
        Microsoft.Win32.UserPreferenceChangedEventHandler themeChanged = (_, e) => {
            if (e.Category == Microsoft.Win32.UserPreferenceCategory.General) ApplyTheme();
        };
        Microsoft.Win32.SystemEvents.UserPreferenceChanged += themeChanged;
        try { LoginStartup.Refresh(); }
        catch (Exception ex) when (ex is UnauthorizedAccessException or System.Security.SecurityException or IOException) { }
        RefreshMicrophone();
        Idle();
        if (showRequested) capsule.Recover(); else capsule.DockRight(true);
        if (!bound) Update(CapsuleState.Idle,"錄音鍵已占用・點設定");
        var ui = SynchronizationContext.Current!;
        using var end = new CancellationTokenSource();
        var listener = AudioImport.Listen(path => {
            if (AudioImport.IsRetryAction(path)) {
                if (state != CapsuleState.RetryLocal || pending != null || dialogOpen) return false;
                ui.Post(_ => {
                    if (state != CapsuleState.RetryLocal || pending != null || dialogOpen) return;
                    if (path == AudioImport.RetryLocal) ToggleRecording(); else Discard();
                }, null);
                return true;
            }
            if (dialogOpen || importing || state is CapsuleState.Recording or CapsuleState.RetryLocal || pending != null || stopping) return false;
            ui.Post(_ => Import(path), null); return true;
        }, end.Token);
        if (import) ui.Post(_ => Import(requestedFile), null);
        if (settingsRequested) ui.Post(_ => Settings(),null);
        using var timer = new System.Windows.Forms.Timer { Interval = 120 };
        var deviceRefresh = Stopwatch.StartNew();
        timer.Tick += (_, _) => {
            if (state == CapsuleState.Idle && deviceRefresh.ElapsedMilliseconds >= 3000) {
                RefreshMicrophone(); deviceRefresh.Restart();
            }
            if (show.WaitOne(0)) capsule.Recover();
            if (configure.WaitOne(0)) Settings();
            if (editor.WaitOne(0)) Edit();
            var next = zh.WaitOne(0) ? "中文" : en.WaitOne(0) ? "英數" : mode;
            if (next != mode) { mode = next; if (state == CapsuleState.Idle) Idle(); }
            if (state == CapsuleState.Recording) {
                if (clock.Elapsed >= TimeSpan.FromMinutes(10) && !stopping) { ToggleRecording(); return; }
                capsule.UpdateRecording(Volatile.Read(ref level),clock.Elapsed.ToString(@"mm\:ss"));
            }
            if (state == CapsuleState.Transcribing) capsule.AdvanceLoadingAnimation();
        };
        timer.Start();
        Application.Run();
        end.Cancel();
        pending?.Cancel(); ReleaseAudio();
        return 0;
    }
}
