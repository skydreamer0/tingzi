using System.Runtime.InteropServices;
using System.Text.Json;

internal static class VoiceDeliveryTests
{
    static int passed;

    [STAThread]
    static async Task<int> Main(string[] args)
    {
        if (CodexFocusProbe.IsProbe(args)) return CodexFocusProbe.Run(args);
        if (args.Length == 2 && args[0] == "--render-settings") { RenderSettings(args[1]); return 0; }
        Check("every icon in the set draws", () => {
            var names = typeof(Icons).Assembly.GetManifestResourceNames().Where(n => n.StartsWith("icons/")).ToArray();
            if (names.Length < 25) throw new Exception("icon set incomplete: " + names.Length);
            foreach (var resource in names) {
                using var image = Icons.Render(resource[6..^4], 32, Color.Black);
                bool drawn = false;
                for (int x = 0; x < 32 && !drawn; x++) for (int y = 0; y < 32 && !drawn; y++) drawn = image.GetPixel(x, y).A > 0;
                if (!drawn) throw new Exception("blank icon " + resource);
            }
        });
        Check("user phrases keep their readings", () => {
            var folder = Path.Combine(Path.GetTempPath(), "tingzi-test-" + Guid.NewGuid().ToString("N"));
            Directory.CreateDirectory(folder);
            try {
                var data = Path.Combine(folder, "phrases.txt");
                File.WriteAllLines(data, new[] { "_ctrl_punctuation_, ， 0.0", "ㄔㄨㄥˊ 重 -3.5", "ㄓㄨㄥˋ 重 -3.0", "ㄧㄠˋ 要 -3.1", "ㄓㄨㄥˋ-ㄧㄠˋ 重要 -4.0" });
                var lexicon = UserPhrases.Lexicon.Read(data);
                Equal(new[] { "ㄓㄨㄥˋ", "ㄔㄨㄥˊ" }, lexicon.For('重').ToArray());
                Equal(0, lexicon.For('好').Count);
                var file = Path.Combine(folder, "user-phrases.txt");
                UserPhrases.Save(new[] { ("ㄔㄨㄥˊ-ㄧㄠˋ", "重要") }, file);
                var saved = UserPhrases.Load(file);
                Equal(1, saved.Count); Equal("ㄔㄨㄥˊ-ㄧㄠˋ", saved[0].Reading); Equal("重要", saved[0].Word);
                Equal(0, UserPhrases.Load(Path.Combine(folder, "missing.txt")).Count);
                File.WriteAllLines(data, new[] { "ㄒㄧㄠˇ 小 -3.0", "ㄒㄧㄠˇ 曉 -4.0", "ㄇㄧㄥˊ 明 -3.0", "ㄇㄧㄥˊ 名 -3.2", "ㄇㄧㄥˋ 命 -3.2", "ㄌㄧˇ 李 -3.0", "ㄌㄧˇ 理 -3.1" });
                var sounds = UserPhrases.Lexicon.Read(data);
                var own = new[] { ("ㄌㄧˇ-ㄒㄧㄠˇ-ㄇㄧㄥˊ", "李曉明") };
                Equal("請李曉明回電", UserPhrases.Correct("請理小名回電", own, sounds));
                Equal("請李曉明回電", UserPhrases.Correct("請李曉明回電", own, sounds));
                Equal("理小命", UserPhrases.Correct("理小命", own, sounds));
                var templateFile = Path.Combine(folder, "user-templates.txt");
                UserPhrases.SaveTemplates(new[] { ("ㄉㄧˋ-ㄓˇ", "台北市 信義區 1 號") }, templateFile);
                var templates = UserPhrases.LoadTemplates(templateFile);
                Equal(1, templates.Count); Equal("ㄉㄧˋ-ㄓˇ", templates[0].Reading); Equal("台北市 信義區 1 號", templates[0].Word);
            } finally { Directory.Delete(folder, true); }
        });
        Check("themes keep text readable", () => {
            foreach (var mode in new[] { ThemeMode.Light, ThemeMode.Dark }) {
                var theme = Theme.Resolve(mode, false);
                if (theme.Dark != (mode == ThemeMode.Dark)) throw new Exception("mode");
                if (Contrast(theme.Text, theme.Surface) < 7 || Contrast(theme.Muted, theme.Surface) < 4.5 || Contrast(theme.OnAccent, theme.Accent) < 4.5)
                    throw new Exception("contrast " + mode);
            }
        });
        await Check("bounded focus probes reject hangs and recover", BoundedProbe);
        Check("script conversion maps characters both ways", () => {
            if (ChineseScript.Convert("語音輸入與體驗", true) != "语音输入与体验") throw new Exception("to simplified");
            if (ChineseScript.Convert("语音输入与体验", false) != "語音輸入與體驗") throw new Exception("to traditional");
            if (ChineseScript.Convert("abc 123", true) != "abc 123") throw new Exception("pass through");
        });
        await Check("clipboard only copies without insertion", ClipboardOnly);
        await Check("auto copies then attempts one insertion", AutoAttemptsOnce);
        await Check("field change preserves copy and expands preview", MovedField);
        await Check("changed clipboard blocks stale text delivery", ClipboardOverwritten);
        await Check("empty or cancelled delivery does not copy", EmptyAndCancelled);
        await Check("cancel while waiting for release does not copy", CancelDuringWait);
        await Check("clipboard failure prevents stale paste", ClipboardFailure);
        await Check("native editor can still receive text if clipboard fails", NativeFallback);
        await Check("explicit preview stays expanded without copying", PreviewOnly);
        Check("legacy settings keep auto as default", LegacySettings);
        Check("explicit legacy auto false remains preview only", LegacyAutoFalse);
        Console.WriteLine($"passed={passed} failed=0");
        return 0;
    }

    static async Task BoundedProbe()
    {
        static System.Diagnostics.ProcessStartInfo Shell(string command) {
            var start = new System.Diagnostics.ProcessStartInfo("powershell.exe");
            start.ArgumentList.Add("-NoProfile");
            start.ArgumentList.Add("-NonInteractive");
            start.ArgumentList.Add("-Command");
            start.ArgumentList.Add(command);
            return start;
        }
        var hung = CodexFocusProbe.RunBoundedAsync(Shell("Start-Sleep -Seconds 60"), CancellationToken.None);
        Equal<string?>(null, await CodexFocusProbe.RunBoundedAsync(Shell("[Console]::Write('99')"), CancellationToken.None));
        Equal<string?>(null, await hung);
        Equal<string?>("42,17,-1", await CodexFocusProbe.RunBoundedAsync(Shell("[Console]::Write('42,17,-1')"), CancellationToken.None));
        using var cancelled = new CancellationTokenSource();
        cancelled.Cancel();
        Equal<string?>(null, await CodexFocusProbe.RunBoundedAsync(Shell("Start-Sleep -Seconds 60"), cancelled.Token));
        Equal<string?>("42", await CodexFocusProbe.RunBoundedAsync(Shell("[Console]::Write('42')"), CancellationToken.None));
    }

    static async Task ClipboardOnly()
    {
        var target = new FakeTarget();
        var copied = new List<string>();
        var result = await Deliver("剪貼簿文字", VoiceDeliveryMode.ClipboardOnly, target, copied.Add,
            clipboardMatches: _ => true);
        Equal(new[] { "剪貼簿文字" }, copied.ToArray());
        Equal(0, target.Attempts);
        Equal(true, result.Copied);
        Equal(false, result.AutoExpandPreview);
    }

    static async Task AutoAttemptsOnce()
    {
        var target = new FakeTarget();
        var copied = new List<string>();
        var result = await Deliver("自動插入文字", VoiceDeliveryMode.Auto, target, copied.Add, clipboardMatches: text => text == "自動插入文字");
        Equal(1, target.Attempts);
        Equal("自動插入文字", target.Text);
        Equal(true, target.ReadyAtAttempt);
        Equal(new[] { "自動插入文字" }, copied.ToArray());
        Equal(VoiceInsertResult.Inserted, result.Insertion);
        Equal(false, result.AutoExpandPreview);
    }

    static async Task MovedField()
    {
        var target = new FakeTarget();
        var copied = new List<string>();
        var current = true;
        target.BeforePaste = () => current = false;
        var result = await Deliver("留存的文字", VoiceDeliveryMode.Auto, target, copied.Add,
            isCurrent: () => current,
            clipboardMatches: _ => true);
        Equal(new[] { "留存的文字" }, copied.ToArray());
        Equal(1, target.Attempts);
        Equal(false, target.Pasted);
        Equal(true, result.Copied);
        Equal(true, result.AutoExpandPreview);
    }

    static async Task ClipboardOverwritten()
    {
        var target = new FakeTarget();
        var copied = new List<string>();
        var reads = 0;
        var result = await Deliver("舊剪貼簿內容", VoiceDeliveryMode.Auto, target, copied.Add,
            clipboardMatches: _ => ++reads == 1);
        Equal(3, reads);
        Equal(false, target.Pasted);
        Equal(VoiceInsertResult.PreviewRequired, result.Insertion);
        Equal(new[] { "舊剪貼簿內容" }, copied.ToArray());
        Equal(true, result.AutoExpandPreview);
    }

    static async Task EmptyAndCancelled()
    {
        var target = new FakeTarget();
        var writes = 0;
        var empty = await Deliver(" \t", VoiceDeliveryMode.Auto, target, _ => writes++);
        Equal(false, empty.Copied);
        Equal(0, writes);
        using var cancel = new CancellationTokenSource();
        cancel.Cancel();
        var cancelled = await Deliver("取消文字", VoiceDeliveryMode.Auto, target, _ => writes++, token: cancel.Token);
        Equal(VoiceInsertResult.Cancelled, cancelled.Insertion);
        Equal(0, writes);
        Equal(0, target.Attempts);
    }

    static async Task ClipboardFailure()
    {
        var target = new FakeTarget();
        var result = await Deliver("仍可取回", VoiceDeliveryMode.Auto, target,
            _ => throw new ExternalException("clipboard busy"), clipboardMatches: _ => true);
        Equal(false, result.Copied);
        Equal(1, target.Attempts);
        Equal(false, target.Pasted);
        Equal(true, result.AutoExpandPreview);
    }

    static async Task CancelDuringWait()
    {
        var target = new FakeTarget { WaitForRelease = true };
        var writes = 0;
        using var cancel = new CancellationTokenSource();
        var delivery = Deliver("等待後不得複製", VoiceDeliveryMode.Auto, target, _ => writes++, token: cancel.Token);
        await target.WaitStarted.Task;
        cancel.Cancel();
        var result = await delivery;
        Equal(VoiceInsertResult.Cancelled, result.Insertion);
        Equal(0, writes);
        Equal(0, target.Attempts);
    }

    static async Task PreviewOnly()
    {
        var target = new FakeTarget();
        var writes = 0;
        var result = await Deliver("預覽文字", VoiceDeliveryMode.PreviewOnly, target, _ => writes++);
        Equal(0, writes);
        Equal(0, target.Attempts);
        Equal(false, result.Copied);
        Equal(true, result.AutoExpandPreview);
    }

    static async Task NativeFallback()
    {
        var target = new FakeTarget { NativeEditor = true };
        var result = await Deliver("原生欄位結果", VoiceDeliveryMode.Auto, target,
            _ => throw new ExternalException("clipboard busy"));
        Equal(1, target.Attempts);
        Equal(true, target.Pasted);
        Equal(false, result.Copied);
        Equal(VoiceInsertResult.Inserted, result.Insertion);
        Equal(true, result.AutoExpandPreview);
    }

    static void LegacySettings()
    {
        var parsed = JsonSerializer.Deserialize<VoiceSettings>("{\"Online\":false,\"Modifiers\":3,\"RecordKey\":82,\"TextSize\":20}")!;
        Equal(VoiceDeliveryMode.Auto, parsed.EffectiveDeliveryMode);
    }

    static void LegacyAutoFalse()
    {
        var parsed = JsonSerializer.Deserialize<VoiceSettings>("{\"Online\":false,\"Modifiers\":3,\"RecordKey\":82,\"TextSize\":20,\"AutoInsert\":false}")!;
        Equal(VoiceDeliveryMode.PreviewOnly, parsed.EffectiveDeliveryMode);
    }

    static Task<VoiceDeliveryResult> Deliver(string text, VoiceDeliveryMode mode, FakeTarget target,
        Action<string> copy, Func<bool>? isCurrent = null, CancellationToken token = default,
        Func<string, bool>? clipboardMatches = null) =>
        VoiceTextDelivery.DeliverAsync(text, mode, target, token, isCurrent,
            value => { copy(value); return true; }, clipboardMatches);

    static double Contrast(Color a, Color b)
    {
        static double Channel(int v) { double s = v / 255.0; return s <= 0.03928 ? s / 12.92 : Math.Pow((s + 0.055) / 1.055, 2.4); }
        static double Lum(Color c) => 0.2126 * Channel(c.R) + 0.7152 * Channel(c.G) + 0.0722 * Channel(c.B);
        double hi = Math.Max(Lum(a), Lum(b)), lo = Math.Min(Lum(a), Lum(b));
        return (hi + 0.05) / (lo + 0.05);
    }

    // Development aid: writes the settings window in both themes so the layout can be reviewed.
    static void RenderSettings(string folder)
    {
        Directory.CreateDirectory(folder);
        foreach (var mode in new[] { ThemeMode.Light, ThemeMode.Dark })
            for (int tab = 0; tab < 4; tab++)
            {
                using var form = VoiceDialogs.CreateSettings(new VoiceSettings { Appearance = mode }, true, _ => null,
                    new SettingsActions(() => { }, () => { }, () => { }, true, true, "本機 Whisper 與模型已就緒。"));
                form.StartPosition = FormStartPosition.Manual; form.Location = new Point(-4000, -4000); form.Show();
                form.Controls.OfType<PageHost>().First().SelectedIndex = tab;
                Application.DoEvents();
                using var image = new Bitmap(form.Width, form.Height);
                form.DrawToBitmap(image, new Rectangle(Point.Empty, form.Size));
                image.Save(Path.Combine(folder, $"settings-{mode}-{tab}.png"));
                form.Close();
            }
    }

    static async Task Check(string name, Func<Task> body)
    {
        await body();
        passed++;
        Console.WriteLine($"PASS {name}");
    }

    static void Check(string name, Action body)
    {
        body();
        passed++;
        Console.WriteLine($"PASS {name}");
    }

    static void Equal<T>(T expected, T actual)
    {
        if (expected is Array expectedArray && actual is Array actualArray) {
            if (!expectedArray.Cast<object?>().SequenceEqual(actualArray.Cast<object?>()))
                throw new InvalidOperationException($"expected={string.Join(",", expectedArray.Cast<object?>())}; actual={string.Join(",", actualArray.Cast<object?>())}");
            return;
        }
        if (!EqualityComparer<T>.Default.Equals(expected, actual))
            throw new InvalidOperationException($"expected={expected}; actual={actual}");
    }

    sealed class FakeTarget : IVoiceInsertionTarget
    {
        public bool DeliveryActive { get; set; } = true;
        public bool NativeEditor { get; set; }
        public int Attempts { get; private set; }
        public string? Text { get; private set; }
        public bool ReadyAtAttempt { get; private set; }
        public bool Pasted { get; private set; }
        public Action? BeforePaste { get; set; }
        public bool WaitForRelease { get; set; }
        public TaskCompletionSource WaitStarted { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);

        public async Task WaitForDeliveryAsync(CancellationToken cancellationToken)
        {
            if (!WaitForRelease) return;
            WaitStarted.TrySetResult();
            await Task.Delay(Timeout.Infinite, cancellationToken);
        }

        public Task<VoiceInsertResult> TryInsertAsync(string text, CancellationToken cancellationToken = default,
            Func<bool>? isCurrent = null, bool prepareClipboard = true, Func<bool>? clipboardReady = null)
        {
            Attempts++;
            Text = text;
            ReadyAtAttempt = clipboardReady?.Invoke() == true;
            BeforePaste?.Invoke();
            Pasted = !cancellationToken.IsCancellationRequested && isCurrent?.Invoke() != false &&
                (NativeEditor || (ReadyAtAttempt && clipboardReady?.Invoke() == true));
            var result = Pasted ? VoiceInsertResult.Inserted : VoiceInsertResult.PreviewRequired;
            return Task.FromResult(result);
        }
    }
}
