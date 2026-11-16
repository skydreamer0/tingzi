using System.Runtime.InteropServices;
using System.Text.Json;
using VoiceInput.App.Services;

internal enum VoiceDeliveryMode { Auto, ClipboardOnly, PreviewOnly }

internal sealed record VoiceSettings(bool Online = false, int Modifiers = 3, Keys RecordKey = Keys.R, int TextSize = 20,
    string? MicrophoneId = null, string? MicrophoneName = null)
{
    public bool AutoInsert { get; init; } = true;
    public bool Simplified { get; init; }
    public bool UsePhrases { get; init; } = true;
    public ThemeMode Appearance { get; init; }
    public bool SystemAccent { get; init; }
    [System.Text.Json.Serialization.JsonIgnore]
    public Theme Colors => Theme.Resolve(Appearance, SystemAccent);
    public VoiceDeliveryMode? DeliveryMode { get; init; }
    [System.Text.Json.Serialization.JsonIgnore]
    public VoiceDeliveryMode EffectiveDeliveryMode => DeliveryMode ?? (AutoInsert ? VoiceDeliveryMode.Auto : VoiceDeliveryMode.PreviewOnly);
    static readonly string FilePath = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
        "Tingzi", "voice-settings.json");
    public bool Valid => (Modifiers is 3 or 5 or 6) && (RecordKey == Keys.None || (RecordKey >= Keys.A && RecordKey <= Keys.Z))
        && !(Modifiers == 3 && RecordKey == Keys.V) && TextSize is 18 or 20 or 24
        && (DeliveryMode == null || Enum.IsDefined(DeliveryMode.Value)) && Enum.IsDefined(Appearance);
    public static VoiceSettings Load()
    {
        try { var value = JsonSerializer.Deserialize<VoiceSettings>(File.ReadAllText(FilePath)); if (value?.Valid == true) return value; }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException or JsonException) { }
        return new();
    }
    public void Save()
    {
        // The text service reads the same choice when it commits Zhuyin text.
        using (var engine = Microsoft.Win32.Registry.CurrentUser.CreateSubKey(@"Software\Tingzi\Engine"))
            engine.SetValue("SimplifiedOutput", Simplified ? 1 : 0, Microsoft.Win32.RegistryValueKind.DWord);
        // The candidate window and composition line follow the same colour choice.
        using (var look = Microsoft.Win32.Registry.CurrentUser.CreateSubKey(@"Software\Tingzi\Appearance"))
            look.SetValue("Theme", (int)Appearance, Microsoft.Win32.RegistryValueKind.DWord);
        Directory.CreateDirectory(Path.GetDirectoryName(FilePath)!);
        var tmp = FilePath + "." + Guid.NewGuid().ToString("N") + ".tmp";
        try { File.WriteAllText(tmp, JsonSerializer.Serialize(this)); File.Move(tmp, FilePath, true); }
        finally { File.Delete(tmp); }
    }
    public string Gesture => RecordKey == Keys.None ? "停用" : ModifierText(Modifiers) + "+" + RecordKey;
    public static string ModifierText(int mods) => mods switch { 5 => "Alt+Shift", 6 => "Ctrl+Shift", _ => "Ctrl+Alt" };
}

internal static class ChineseScript
{
    // Character-level conversion from Windows; one-to-many characters follow the system table.
    public static string Convert(string text, bool simplified)
    {
        if (text.Length == 0) return text;
        var result = new char[text.Length * 2 + 1];
        var length = LCMapStringEx(null, simplified ? 0x02000000u : 0x04000000u, text, text.Length, result, result.Length, IntPtr.Zero, IntPtr.Zero, IntPtr.Zero);
        return length > 0 ? new string(result, 0, length) : text;
    }
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode)]
    static extern int LCMapStringEx(string? locale, uint flags, string source, int sourceLength,
        [Out] char[] destination, int destinationLength, IntPtr version, IntPtr reserved, IntPtr sortHandle);
}

internal sealed class RecordingShortcut : NativeWindow, IDisposable
{
    readonly Action toggle;
    VoiceSettings? active;
    int id = 701;
    public RecordingShortcut(Action toggle) { this.toggle = toggle; CreateHandle(new CreateParams { Parent = new IntPtr(-3) }); }
    public bool Apply(VoiceSettings prefs)
    {
        if (!prefs.Valid) return false;
        if (active?.RecordKey == prefs.RecordKey && active.Modifiers == prefs.Modifiers) return true;
        int next = id == 701 ? 702 : 701;
        if (prefs.RecordKey != Keys.None && !RegisterHotKey(Handle, next, (uint)(0x4000 | prefs.Modifiers), (uint)prefs.RecordKey)) return false;
        if (active != null && active.RecordKey != Keys.None) UnregisterHotKey(Handle,id);
        id = next; active = prefs; return true;
    }
    protected override void WndProc(ref Message msg)
    {
        if (msg.Msg == 0x0312 && msg.WParam == id) { toggle(); return; }
        base.WndProc(ref msg);
    }
    public void Dispose() { if (active?.RecordKey != Keys.None) UnregisterHotKey(Handle,id); DestroyHandle(); }
    [DllImport("user32")] static extern bool RegisterHotKey(nint window,int id,uint modifiers,uint key);
    [DllImport("user32")] static extern bool UnregisterHotKey(nint window,int id);
}

internal sealed record SettingsActions(Action SwitchZhuyin, Action OpenTypingPad, Action OpenInputSettings, bool KeyboardAvailable, bool TypingPadAvailable, string RecognitionStatus, Func<bool, Task<string?>>? SetLearning = null, Func<Task<bool?>>? GetLearning = null,
    Func<string[], Task<bool>>? LearningTool = null, string? WordData = null);

internal static class VoiceDialogs
{
    static Form Dialog(string title, Size size) => new Form { Text = title, ClientSize = size, MinimumSize = size,
        Font = new Font("Microsoft JhengHei UI", 11), AutoScaleMode = AutoScaleMode.Dpi,
        Icon = Icon.ExtractAssociatedIcon(Application.ExecutablePath), StartPosition = FormStartPosition.CenterScreen };
    static Button ActionButton(string label, Action action, string? icon = null)
    {
        var button = new RoundButton { Text=label, AutoSize=true, AutoSizeMode=AutoSizeMode.GrowAndShrink, MinimumSize=new Size(112,36), Padding=new Padding(14,5,14,5), Margin=new Padding(0,0,8,10), UseVisualStyleBackColor=false, BackColor=Color.White, ForeColor=Color.FromArgb(25,49,78), FlatStyle=FlatStyle.Flat, AccessibleName=label };
        button.FlatAppearance.BorderColor=Color.FromArgb(174,190,208);
        button.Tag = icon;
        button.Click += (_,_) => action(); return button;
    }
    static ComboBox Choices(string[] options,int index)
    {
        var box = new ComboBox { DropDownStyle = ComboBoxStyle.DropDownList, Dock = DockStyle.Fill };
        box.Items.AddRange(options); box.SelectedIndex = index; return box;
    }
    static (Panel Page, TableLayoutPanel Stack) Page(Form form, string title)
    {
        var page = new Panel { BackColor = Theme.MarkBack, Padding = new Padding(10,18,18,8), AutoScroll = true };
        var stack = new TableLayoutPanel { Dock = DockStyle.Top, ColumnCount = 1, AutoSize = true, AutoSizeMode = AutoSizeMode.GrowAndShrink, BackColor = Color.Transparent };
        stack.ColumnStyles.Add(new ColumnStyle(SizeType.Percent,100));
        stack.Controls.Add(new Label { Text=title, AutoSize=true, Font=new Font(form.Font.FontFamily,17,FontStyle.Bold), ForeColor=Theme.MarkText, BackColor=Color.Transparent, Margin=new Padding(2,0,0,10) });
        page.Controls.Add(stack);
        return (page, stack);
    }
    static void Section(TableLayoutPanel stack, string title) =>
        stack.Controls.Add(new Label { Text=title, AutoSize=true, Font=new Font(stack.Font,FontStyle.Bold), ForeColor=Theme.MarkText, BackColor=Color.Transparent, Margin=new Padding(2,12,0,6) });
    static void Note(TableLayoutPanel stack, string text) =>
        stack.Controls.Add(new Label { Text=text, AutoSize=true, Anchor=AnchorStyles.Left | AnchorStyles.Right, ForeColor=Theme.MarkMuted, BackColor=Color.Transparent, Margin=new Padding(4,2,4,6) });
    // One setting: an icon, its name and explanation on the left, the control on the right.
    static Card Setting(TableLayoutPanel stack, string icon, string title, string? description, Control? control)
    {
        var card = new Card();
        var grid = new TableLayoutPanel { Dock = DockStyle.Top, AutoSize = true, AutoSizeMode = AutoSizeMode.GrowAndShrink, ColumnCount = 3, RowCount = 2, BackColor = Color.Transparent };
        grid.ColumnStyles.Add(new ColumnStyle(SizeType.AutoSize)); grid.ColumnStyles.Add(new ColumnStyle(SizeType.Percent,100)); grid.ColumnStyles.Add(new ColumnStyle(SizeType.AutoSize));
        var picture = new PictureBox { Tag=icon, Size=new Size(20,20), SizeMode=PictureBoxSizeMode.CenterImage, Anchor=AnchorStyles.None, Margin=new Padding(0,0,12,0), TabStop=false, BackColor=Color.Transparent };
        grid.Controls.Add(picture,0,0); grid.SetRowSpan(picture,2);
        var name = new Label { Text=title, AutoSize=true, Anchor=AnchorStyles.Left | AnchorStyles.Right, ForeColor=Theme.MarkText, BackColor=Color.Transparent, Margin=new Padding(0,2,8,2) };
        grid.Controls.Add(name,1,0);
        if (description != null) {
            card.Description = new Label { Text=description, AutoSize=true, Anchor=AnchorStyles.Left | AnchorStyles.Right, Font=new Font(stack.Font.FontFamily,9.5f), ForeColor=Theme.MarkMuted, BackColor=Color.Transparent, Margin=new Padding(0,0,8,2) };
            grid.Controls.Add(card.Description,1,1);
        } else grid.SetRowSpan(name,2);
        if (control != null) {
            control.Dock = DockStyle.None; control.Anchor = AnchorStyles.Right; control.Margin = Padding.Empty;
            if (string.IsNullOrEmpty(control.AccessibleName)) control.AccessibleName = title;
            grid.Controls.Add(control,2,0); grid.SetRowSpan(control,2);
        }
        card.Hold(grid); stack.Controls.Add(card);
        return card;
    }
    static FlowLayoutPanel Group(params Control[] controls)
    {
        var flow = new FlowLayoutPanel { AutoSize=true, AutoSizeMode=AutoSizeMode.GrowAndShrink, WrapContents=false, BackColor=Color.Transparent, Margin=Padding.Empty, Padding=Padding.Empty };
        for (int i = 0; i < controls.Length; i++) { controls[i].Margin = new Padding(i == 0 ? 0 : 8,0,0,0); controls[i].Anchor = AnchorStyles.None; flow.Controls.Add(controls[i]); }
        return flow;
    }
    static T Wide<T>(T control, int width) where T : Control { control.Dock = DockStyle.None; control.Width = width; return control; }
    public static Form CreateSettings(VoiceSettings current, bool bound, Func<VoiceSettings,string?> apply, SettingsActions commands)
    {
        var form = Dialog("聽字・注音與語音輸入法 · 設定", new Size(780,640));
        form.MinimumSize = new Size(560,420);
        form.BackColor = Theme.MarkBack;
        var tabs = new PageHost { Dock = DockStyle.Fill };
        var (voicePage, voice) = Page(form, "語音輸入");
        var (zhuyinPage, typing) = Page(form, "注音與工具");
        var (lookPage, look) = Page(form, "外觀與啟動");
        var (aboutPage, about) = Page(form, "關於");
        tabs.Add("語音輸入", "mic", voicePage); tabs.Add("注音與工具", "keyboard", zhuyinPage);
        tabs.Add("外觀與啟動", "theme", lookPage); tabs.Add("關於", "info", aboutPage);
        var footer = new TableLayoutPanel { Dock = DockStyle.Bottom, AutoSize = true, ColumnCount = 3, BackColor = Theme.MarkBack, Padding = new Padding(20,10,18,14) };
        footer.ColumnStyles.Add(new ColumnStyle(SizeType.Percent,100)); footer.ColumnStyles.Add(new ColumnStyle(SizeType.AutoSize)); footer.ColumnStyles.Add(new ColumnStyle(SizeType.AutoSize));
        var status = new Label { AutoSize=true, Anchor=AnchorStyles.Left | AnchorStyles.Right, ForeColor=Color.DarkRed, BackColor=Color.Transparent, Margin=new Padding(0,0,12,0),
            Text=bound?"":"目前錄音快捷鍵被占用，請選其他組合。" };
        var save = ActionButton("儲存設定", () => { }, "check"); save.BackColor=Color.FromArgb(18,113,112); save.ForeColor=Color.White;
        var cancel = ActionButton("取消", form.Close, "close"); cancel.DialogResult=DialogResult.Cancel;
        cancel.Margin=Padding.Empty; save.Margin=new Padding(8,0,0,0); cancel.Anchor=save.Anchor=AnchorStyles.Right;
        footer.Controls.Add(status,0,0); footer.Controls.Add(cancel,1,0); footer.Controls.Add(save,2,0);
        form.Controls.Add(tabs); form.Controls.Add(footer);

        // Voice input
        var inputs = Microphones.Available();
        var labels = new List<string> { "跟隨 Windows 預設麥克風" };
        labels.AddRange(inputs.Select(input => input.Name));
        int selected = 0;
        if (!string.IsNullOrEmpty(current.MicrophoneId)) {
            selected = inputs.FindIndex(input => string.Equals(input.Id,current.MicrophoneId,StringComparison.OrdinalIgnoreCase)) + 1;
            if (selected == 0) {
                selected = labels.Count;
                labels.Add((current.MicrophoneName ?? "指定的麥克風") + "（未連接）");
            }
        }
        var microphone = Wide(Choices(labels.ToArray(), selected), 250);
        var meter = new LevelMeter { Size = new Size(120,8), Track = current.Colors.Border, Bar = current.Colors.Accent };
        var test = ActionButton("測試", () => { }, "mic"); test.MinimumSize = new Size(96,34); test.AccessibleName = "測試麥克風";
        Section(voice, "麥克風");
        Setting(voice, "mic", "麥克風", "語音輸入使用的收音裝置。", microphone);
        Setting(voice, "audio-file", "測試收音", "按「測試」後對著麥克風說話，音量條會跟著動，十秒後自動停止。", Group(meter, test));
        Section(voice, "辨識");
        var engine = Wide(Choices(new[]{"本機 Whisper","Codex 線上辨識（第三方橋接）"}, current.Online?1:0), 250);
        Setting(voice, "settings", "辨識方式", commands.RecognitionStatus + " 選用線上辨識時，音檔會經第三方 codex-asr 傳到 ChatGPT。", engine);
        var script = Wide(Choices(new[]{"繁體中文","簡體中文"}, current.Simplified ? 1 : 0), 250);
        Setting(voice, "convert", "輸出文字", "同時套用到語音結果與注音送出的字，逐字轉換。", script);
        var usePhrases = new Toggle { Checked = current.UsePhrases, Enabled = commands.WordData != null };
        Setting(voice, "plus", "套用自訂詞", "把辨識結果裡同音的字換成你加入的自訂詞，例如人名和專有名詞。", usePhrases);
        var delivery = Wide(Choices(new[]{"自動插入並複製文字","只複製到剪貼簿","只顯示預覽"}, (int)current.EffectiveDeliveryMode), 250);
        Setting(voice, "copy", "辨識完成後", "錄音途中若切換欄位，辨識文字會留在浮窗預覽。", delivery);
        Section(voice, "操作");
        int[] mods = {3,6,5};
        var mod = Wide(Choices(mods.Select(VoiceSettings.ModifierText).ToArray(),Array.IndexOf(mods,current.Modifiers)), 150);
        var keys = new[]{"停用"}.Concat(Enumerable.Range('A',26).Select(c=>((char)c).ToString())).ToArray();
        var key = Wide(Choices(keys,current.RecordKey == Keys.None ? 0 : (int)current.RecordKey-(int)Keys.A+1), 92);
        mod.AccessibleName = "錄音快捷鍵的輔助鍵"; key.AccessibleName = "錄音快捷鍵的字母";
        Setting(voice, "keyboard", "錄音快捷鍵", "按一次開始錄音，再按一次停止。", Group(mod, key));
        int[] sizes={18,20,24};
        var size = Wide(Choices(new[]{"18 px","20 px","24 px"},Array.IndexOf(sizes,current.TextSize)), 250);
        Setting(voice, "edit", "預覽字級", "浮窗裡辨識文字的大小。", size);

        IDisposable? listening = null;
        var testLimit = new System.Windows.Forms.Timer { Interval = 10000 };
        void StopTest() { testLimit.Stop(); listening?.Dispose(); listening = null; meter.Level = 0; test.Text = "測試"; }
        testLimit.Tick += (_,_) => StopTest();
        test.Click += (_,_) => {
            if (listening != null) { StopTest(); return; }
            int index = microphone.SelectedIndex;
            if (index > inputs.Count) { status.Text = "這支麥克風目前未連接。"; return; }
            try {
                listening = Microphones.Listen(index == 0 ? -1 : inputs[index-1].Number, level => {
                    try { if (form.IsHandleCreated) form.BeginInvoke(() => { if (listening != null) meter.Level = level; }); }
                    catch (InvalidOperationException) { /* The window closed while audio was arriving. */ }
                });
                test.Text = "停止"; status.Text = ""; testLimit.Start();
            } catch (Exception) {
                // Drivers report a missing or blocked device in several different ways.
                status.Text = "無法開啟這支麥克風，請確認 Windows 的麥克風權限。";
            }
        };
        microphone.SelectedIndexChanged += (_,_) => StopTest();
        form.FormClosed += (_,_) => { StopTest(); testLimit.Dispose(); };

        // Zhuyin and tools
        var zhuyin = ZhuyinOptions.Load();
        Section(typing, "打字");
        var keyLayout = Wide(Choices(new[]{"標準（大千）","倚天"}, zhuyin.Layout), 250);
        Setting(typing, "keyboard", "鍵盤配置", "注音符號在鍵盤上的排法。", keyLayout);
        var segment = Wide(Choices(new[]{"整句最佳","長詞優先"}, zhuyin.Staged ? 1 : 0), 250);
        Setting(typing, "settings", "整句選字方式", "「整句最佳」比較整句的詞頻總和；「長詞優先」先定案長詞，再由左到右決定其餘的字。", segment);
        var candidateSize = Wide(Choices(ZhuyinOptions.CandidateSizes.Select(s => s + " px").ToArray(), Math.Max(0,Array.IndexOf(ZhuyinOptions.CandidateSizes, zhuyin.CandidateSize))), 250);
        Setting(typing, "edit", "候選窗字級", null, candidateSize);
        var lineSize = Wide(Choices(ZhuyinOptions.LineSizes.Select(s => s == 0 ? "關閉" : s + " px").ToArray(), Math.Max(0,Array.IndexOf(ZhuyinOptions.LineSizes, zhuyin.LineSize))), 250);
        Setting(typing, "float", "大字輸入行", "打字時在游標旁放大顯示正在組的字。", lineSize);
        Note(typing, "以上四項在按下「儲存設定」後，於重新開啟的應用程式生效。");
        Setting(typing, "info", "打字時的按鍵", "Shift 加 ， 。 ？ ！ 等鍵輸入全形標點，[ ] 輸入「」，` 開啟符號表。組字時按 Tab 把第一個詞切短，選字時按 Tab 切換近音字。Shift＋Enter 送出並把這段字存成自訂詞。", null);
        Section(typing, "詞語與學習");
        var phrases = ActionButton("管理", () => EditPhrases(commands.WordData!, current.Colors, false), "plus"); phrases.Enabled = commands.WordData != null; phrases.AccessibleName = "管理自訂詞";
        Setting(typing, "plus", "自訂詞", commands.WordData != null ? "加入姓名、專有名詞等詞語，打注音時優先出現。" : "找不到注音詞庫，請重新安裝完整套件。", phrases);
        var templates = ActionButton("管理", () => EditPhrases(commands.WordData!, current.Colors, true), "copy"); templates.Enabled = commands.WordData != null; templates.AccessibleName = "管理文字範本";
        Setting(typing, "copy", "文字範本", "打出觸發詞的注音後按 ↓，從候選清單選出整段文字，例如地址或常用回覆。", templates);
        var learn = new Toggle { Enabled = false, AccessibleName = "本機學習" };
        var learning = Setting(typing, "learn", "本機學習", commands.GetLearning == null ? "找不到本機學習工具，請安裝完整套件。" : "讀取學習狀態中…", learn);
        const string learnOn = "已啟用：記住你改選過的字；連續選出的字出現兩次就記成新詞。切換後立即套用。", learnOff = "已關閉：不記錄改字習慣。切換後立即套用。";
        bool settingLearning = false;
        learn.CheckedChanged += async (_,_) => {
            if (settingLearning || commands.SetLearning == null) return;
            bool wanted = learn.Checked;
            learn.Enabled = false;
            var error = await commands.SetLearning(wanted);
            if (form.IsDisposed) return;
            if (error != null) { settingLearning = true; learn.Checked = !wanted; settingLearning = false; }
            learning.Description!.Text = error ?? (wanted ? learnOn : learnOff);
            learn.Enabled = true;
        };
        form.Shown += async (_,_) => {
            if (commands.GetLearning == null) return;
            var enabled = await commands.GetLearning();
            if (form.IsDisposed) return;
            settingLearning = true; learn.Checked = enabled == true; settingLearning = false;
            learning.Description!.Text = enabled.HasValue ? (enabled.Value ? learnOn : learnOff) : "無法讀取目前的學習狀態";
            learn.Enabled = commands.SetLearning != null;
        };
        if (commands.LearningTool != null) {
            var import = ActionButton("匯入", () => { }, "folder"); import.MinimumSize = new Size(84,34); import.AccessibleName = "匯入學習資料";
            var export = ActionButton("匯出", () => { }, "export"); export.MinimumSize = new Size(84,34); export.AccessibleName = "匯出學習資料";
            var clear = ActionButton("清除", () => { }, "trash"); clear.MinimumSize = new Size(84,34); clear.AccessibleName = "清除學習資料";
            var data = Setting(typing, "export", "學習資料", "匯出成文字檔帶到另一台電腦再匯入，或全部清除。", Group(import, export, clear));
            import.Click += async (_,_) => {
                using var pick = new OpenFileDialog { Filter="Tab 分隔文字 (*.tsv)|*.tsv|所有檔案 (*.*)|*.*" };
                if (pick.ShowDialog(form) != DialogResult.OK) return;
                bool done = await commands.LearningTool(new[]{"import", pick.FileName});
                if (!form.IsDisposed) data.Description!.Text = done ? "學習資料已匯入。" : "無法匯入這個檔案。";
            };
            export.Click += async (_,_) => {
                using var pick = new SaveFileDialog { Filter="Tab 分隔文字 (*.tsv)|*.tsv", FileName="tingzi-learning.tsv", OverwritePrompt=true };
                if (pick.ShowDialog(form) != DialogResult.OK) return;
                try { File.Delete(pick.FileName); } catch (Exception ex) when (ex is IOException or UnauthorizedAccessException) { }
                bool done = await commands.LearningTool(new[]{"export", pick.FileName});
                if (!form.IsDisposed) data.Description!.Text = done ? "學習資料已匯出。" : "沒有可匯出的學習資料，或無法寫入該位置。";
            };
            clear.Click += async (_,_) => {
                if (MessageBox.Show(form, "會刪除所有記住的改字習慣，無法復原。要繼續嗎？", "聽字・注音與語音輸入法", MessageBoxButtons.OKCancel, MessageBoxIcon.Warning, MessageBoxDefaultButton.Button2) != DialogResult.OK) return;
                bool done = await commands.LearningTool(new[]{"clear","--confirm"});
                if (!form.IsDisposed) data.Description!.Text = done ? "學習資料已清除。" : "沒有學習資料可清除，或無法寫入。";
            };
        }
        Section(typing, "工具");
        var switchZhuyin = ActionButton("切換", commands.SwitchZhuyin, "keyboard"); switchZhuyin.Enabled=commands.KeyboardAvailable; switchZhuyin.AccessibleName = "切換到注音輸入法";
        Setting(typing, "keyboard", "切換到注音輸入法", commands.KeyboardAvailable ? "切換後，在原本的應用程式內打字。" : "找不到注音切換工具。請重新安裝完整套件，再從 Windows 語言設定確認輸入法。", switchZhuyin);
        var typingPad = ActionButton("開啟", commands.OpenTypingPad, "pad"); typingPad.Enabled=commands.TypingPadAvailable; typingPad.AccessibleName = "開啟本機打字板";
        Setting(typing, "pad", "本機打字板", commands.TypingPadAvailable ? "開啟獨立欄位練習注音與選字。" : "找不到打字板或注音詞表，請重新安裝完整套件。", typingPad);
        var language = ActionButton("開啟", commands.OpenInputSettings, "language"); language.AccessibleName = "開啟 Windows 輸入法設定";
        Setting(typing, "language", "Windows 語言與鍵盤設定", "管理 Windows 的鍵盤清單和輸入語言。", language);

        // Appearance and startup
        var appearance = Wide(Choices(new[]{"跟隨 Windows","淺色","深色"}, (int)current.Appearance), 250);
        var accent = Wide(Choices(new[]{"聽字靛色","跟隨 Windows 強調色"}, current.SystemAccent ? 1 : 0), 250);
        var startup = new Toggle { Checked = LoginStartup.Enabled };
        Section(look, "外觀");
        Setting(look, "theme", "色系", "套用到設定、浮條、系統匣選單，以及注音的候選窗與輸入行。", appearance);
        Setting(look, "edit", "強調色", "按鈕與選取項目使用的顏色。", accent);
        Section(look, "啟動");
        Setting(look, "power", "登入 Windows 時啟動語音浮窗", "登入後就會在螢幕右側看到麥克風。", startup);
        Note(look, "變更在按下「儲存設定」後生效。");

        // About
        var mark = Brand.Render(96);
        form.Disposed += (_,_) => mark.Dispose();
        var identity = new Card();
        var who = new TableLayoutPanel { Dock = DockStyle.Top, AutoSize = true, ColumnCount = 2, RowCount = 2, BackColor = Color.Transparent };
        who.ColumnStyles.Add(new ColumnStyle(SizeType.AutoSize)); who.ColumnStyles.Add(new ColumnStyle(SizeType.Percent,100));
        var logo = new PictureBox { Image=mark, SizeMode=PictureBoxSizeMode.Zoom, Size=new Size(48,48), Margin=new Padding(0,2,14,2), TabStop=false, BackColor=Color.Transparent };
        who.Controls.Add(logo,0,0); who.SetRowSpan(logo,2);
        who.Controls.Add(new Label { Text="聽字・注音與語音輸入法", AutoSize=true, Font=new Font(form.Font.FontFamily,13,FontStyle.Bold), ForeColor=Theme.MarkText, BackColor=Color.Transparent, Margin=new Padding(0,4,0,0) },1,0);
        who.Controls.Add(new Label { Text="版本 " + (typeof(VoiceDialogs).Assembly.GetName().Version?.ToString(3) ?? "未知"), AutoSize=true, ForeColor=Theme.MarkMuted, BackColor=Color.Transparent, Margin=new Padding(1,0,0,0) },1,1);
        identity.Hold(who); about.Controls.Add(identity);
        Note(about, "Windows 的繁體中文注音與語音輸入。注音組字、學習資料、自訂詞和本機語音辨識都在這台電腦上處理；只有選用線上辨識時，音檔才會送到第三方服務。");
        Section(about, "連結");
        Setting(about, "link", "專案首頁", "原始碼、說明文件與更新紀錄。", ActionButton("開啟", () => OpenTarget("https://github.com/skydreamer0/tingzi"), "link"));
        Setting(about, "edit", "回報問題", "在 GitHub 提出錯誤或建議。", ActionButton("開啟", () => OpenTarget("https://github.com/skydreamer0/tingzi/issues"), "link"));
        Section(about, "資料與授權");
        Setting(about, "folder", "個人資料夾", "存放設定、學習資料和自訂詞；刪除它就會清除這台電腦上所有的個人資料。", ActionButton("開啟", () => OpenTarget(Path.GetDirectoryName(UserPhrases.FilePath)!), "folder"));
        Setting(about, "info", "注音詞庫", "安裝包附的詞庫取自 McBopomofo（MIT 授權），授權全文在安裝資料夾的 McBopomofo-LICENSE.txt。", null);

        save.Click+=(_,_)=>{
            var index = microphone.SelectedIndex;
            var input = index > 0 && index <= inputs.Count ? inputs[index-1] : null;
            var candidate=new VoiceSettings(engine.SelectedIndex==1,mods[mod.SelectedIndex],key.SelectedIndex==0?Keys.None:(Keys)((int)Keys.A+key.SelectedIndex-1),sizes[size.SelectedIndex],
                index == 0 ? null : input?.Id ?? current.MicrophoneId, index == 0 ? null : input?.Name ?? current.MicrophoneName) { AutoInsert = delivery.SelectedIndex == 0, DeliveryMode = (VoiceDeliveryMode)delivery.SelectedIndex, Simplified = script.SelectedIndex == 1, UsePhrases = usePhrases.Checked,
                Appearance = (ThemeMode)appearance.SelectedIndex, SystemAccent = accent.SelectedIndex == 1 };
            var error=apply(candidate);
            if(error==null) {
                try {
                    new ZhuyinOptions(keyLayout.SelectedIndex, segment.SelectedIndex == 1, ZhuyinOptions.CandidateSizes[candidateSize.SelectedIndex], ZhuyinOptions.LineSizes[lineSize.SelectedIndex]).Save();
                    LoginStartup.Set(startup.Checked);
                }
                catch (Exception ex) when (ex is UnauthorizedAccessException or System.Security.SecurityException or IOException) { }
                form.Close();
            } else { status.Text=error; tabs.Open(voicePage); }
        };
        form.AcceptButton=save;form.CancelButton=cancel;
        current.Colors.Apply(form);
        return form;
    }
    static void OpenTarget(string target)
    {
        try { System.Diagnostics.Process.Start(new System.Diagnostics.ProcessStartInfo(target) { UseShellExecute = true }); }
        catch (Exception ex) when (ex is System.ComponentModel.Win32Exception or InvalidOperationException or IOException) {
            MessageBox.Show("無法開啟：" + target, "聽字・注音與語音輸入法");
        }
    }
    // Custom words and text templates share one editor; a template adds the text its trigger expands to.
    static void EditPhrases(string wordData, Theme colors, bool templates)
    {
        string kind = templates ? "範本" : "詞";
        using var form = Dialog(templates ? "聽字 · 文字範本" : "聽字 · 自訂詞", new Size(560,560));
        form.BackColor = Color.White; form.Padding = new Padding(16); form.MinimumSize = new Size(420,440);
        var words = templates ? UserPhrases.LoadTemplates() : UserPhrases.Load();
        var grid = new TableLayoutPanel { Dock = DockStyle.Fill, ColumnCount = 2, RowCount = 7 };
        grid.ColumnStyles.Add(new ColumnStyle(SizeType.Percent,100)); grid.ColumnStyles.Add(new ColumnStyle(SizeType.AutoSize));
        foreach (var style in new[]{ SizeType.AutoSize, SizeType.Percent, SizeType.AutoSize, SizeType.AutoSize, SizeType.AutoSize, SizeType.AutoSize, SizeType.AutoSize })
            grid.RowStyles.Add(new RowStyle(style,100));
        var intro = new Label { AutoSize=true, Anchor=AnchorStyles.Left | AnchorStyles.Right, ForeColor=Color.FromArgb(66,83,103), Margin=new Padding(0,0,0,10),
            Text = templates
                ? "輸入一個觸發詞（例如「地址」）和要展開的文字。打出觸發詞的注音後按 ↓，就能在候選清單選到這段文字。破音字請選對讀音。新範本在重新開啟的應用程式生效。"
                : "加入的詞在打注音時優先出現，語音辨識的同音字也會換成這些詞。輸入詞語後，每個字會帶出讀音；破音字請從下拉選單選對的讀音。新詞在重新開啟的應用程式生效。" };
        var list = new ListBox { Dock=DockStyle.Fill, IntegralHeight=false, AccessibleName=templates ? "已加入的範本" : "已加入的詞", Margin=new Padding(0,0,0,10) };
        void Refill() {
            list.Items.Clear();
            foreach (var entry in words) list.Items.Add(templates ? entry.Reading.Replace("-"," ") + "　→　" + entry.Word : entry.Word + "　" + entry.Reading.Replace("-"," "));
        }
        var word = new TextBox { Dock=DockStyle.Fill, MaxLength=UserPhrases.Longest, AccessibleName=templates ? "觸發詞" : "新詞", PlaceholderText=templates ? "觸發詞，2 到 8 個字" : "輸入 2 到 8 個字的詞", Enabled=false, Margin=new Padding(0,4,0,6) };
        var add = ActionButton("加入", () => { }, "plus"); add.BackColor=Color.FromArgb(18,113,112); add.ForeColor=Color.White; add.Margin=new Padding(8,0,0,6); add.MinimumSize=new Size(96,34);
        var readings = new FlowLayoutPanel { Dock=DockStyle.Fill, AutoSize=true, WrapContents=true, Margin=Padding.Empty };
        var expansion = new TextBox { Dock=DockStyle.Fill, MaxLength=400, AccessibleName="展開後的文字", PlaceholderText="展開後的文字", Visible=templates, Margin=new Padding(0,2,0,6) };
        var status = new Label { AutoSize=true, Anchor=AnchorStyles.Left | AnchorStyles.Right, ForeColor=Color.FromArgb(66,83,103), Text="讀取詞庫中…", Margin=new Padding(0,6,0,10) };
        var buttons = new FlowLayoutPanel { Dock=DockStyle.Fill, AutoSize=true, FlowDirection=FlowDirection.RightToLeft, Margin=Padding.Empty };
        var close = ActionButton("關閉", form.Close, "close"); close.Margin=new Padding(8,0,0,0);
        var remove = ActionButton("刪除選取", () => { }, "trash"); remove.Margin=new Padding(8,0,0,0);
        var exportList = ActionButton("匯出", () => { }, "export"); exportList.Margin=new Padding(8,0,0,0); exportList.MinimumSize=new Size(84,36);
        var importList = ActionButton("匯入", () => { }, "folder"); importList.Margin=Padding.Empty; importList.MinimumSize=new Size(84,36);
        buttons.Controls.Add(close); buttons.Controls.Add(remove); buttons.Controls.Add(exportList); buttons.Controls.Add(importList);
        grid.Controls.Add(intro,0,0); grid.SetColumnSpan(intro,2);
        grid.Controls.Add(list,0,1); grid.SetColumnSpan(list,2);
        grid.Controls.Add(word,0,2); grid.Controls.Add(add,1,2);
        grid.Controls.Add(readings,0,3); grid.SetColumnSpan(readings,2);
        grid.Controls.Add(expansion,0,4); grid.SetColumnSpan(expansion,2);
        grid.Controls.Add(status,0,5); grid.SetColumnSpan(status,2);
        grid.Controls.Add(buttons,0,6); grid.SetColumnSpan(buttons,2);
        form.Controls.Add(grid);
        UserPhrases.Lexicon? lexicon = null;
        bool Persist() {
            try { if (templates) UserPhrases.SaveTemplates(words); else UserPhrases.Save(words); return true; }
            catch (Exception ex) when (ex is IOException or UnauthorizedAccessException) { status.Text="無法儲存。"; return false; }
        }
        form.Shown += async (_,_) => {
            lexicon = await UserPhrases.SharedLexicon(wordData);
            if (form.IsDisposed) return;
            if (lexicon == null) { status.Text="無法讀取注音詞庫，請重新安裝完整套件。"; return; }
            status.Text=""; word.Enabled=true; word.Focus();
        };
        word.TextChanged += (_,_) => {
            foreach (var old in readings.Controls.OfType<Control>().ToArray()) old.Dispose();
            readings.Controls.Clear(); status.Text="";
            if (lexicon == null) return;
            var boxes = new List<Control>();
            foreach (char character in word.Text.Trim()) {
                var options = lexicon.For(character);
                if (options.Count == 0) { status.Text=$"詞庫裡沒有「{character}」的讀音，無法使用這個詞。"; foreach (var box in boxes) box.Dispose(); return; }
                var choice = Choices(options.ToArray(),0); choice.Dock=DockStyle.None; choice.Width=form.LogicalToDeviceUnits(118);
                choice.AccessibleName=$"「{character}」的讀音"; choice.Margin=new Padding(0,0,6,6); boxes.Add(choice);
            }
            readings.Controls.AddRange(boxes.ToArray());
            colors.Apply(readings);
        };
        add.Click += (_,_) => {
            var text = word.Text.Trim();
            if (text.Length < 2) { status.Text="請輸入至少兩個字。"; return; }
            if (readings.Controls.Count != text.Length) return;
            var output = templates ? expansion.Text.Trim() : text;
            if (output.Length == 0) { status.Text="請輸入展開後的文字。"; return; }
            var reading = string.Join("-", readings.Controls.OfType<ComboBox>().Select(box => (string)box.SelectedItem!));
            words.RemoveAll(entry => entry.Word == output && entry.Reading == reading);
            words.Add((reading,output));
            if (!Persist()) return;
            Refill(); word.Clear(); expansion.Clear(); status.Text=$"已加入「{(templates ? text : output)}」。"; word.Focus();
        };
        remove.Click += (_,_) => {
            int index = list.SelectedIndex;
            if (index < 0) { status.Text="請先在清單選一項。"; return; }
            var gone = words[index]; words.RemoveAt(index);
            if (!Persist()) { words.Insert(index,gone); return; }
            Refill(); status.Text="已刪除。";
        };
        exportList.Click += (_,_) => {
            using var pick = new SaveFileDialog { Filter="文字檔 (*.txt)|*.txt", FileName=templates ? "tingzi-templates.txt" : "tingzi-words.txt", OverwritePrompt=true };
            if (pick.ShowDialog(form) != DialogResult.OK) return;
            try { if (templates) UserPhrases.SaveTemplates(words, pick.FileName); else UserPhrases.Save(words, pick.FileName); status.Text=$"已匯出 {words.Count} 筆。"; }
            catch (Exception ex) when (ex is IOException or UnauthorizedAccessException) { status.Text="無法寫入該位置。"; }
        };
        importList.Click += (_,_) => {
            using var pick = new OpenFileDialog { Filter="文字檔 (*.txt)|*.txt|所有檔案 (*.*)|*.*" };
            if (pick.ShowDialog(form) != DialogResult.OK) return;
            var incoming = templates ? UserPhrases.LoadTemplates(pick.FileName) : UserPhrases.Load(pick.FileName);
            var before = words.ToList();
            int added = 0;
            foreach (var entry in incoming) if (!words.Contains(entry)) { words.Add(entry); added++; }
            if (added > 0 && !Persist()) { words.Clear(); words.AddRange(before); return; }
            Refill(); status.Text = incoming.Count == 0 ? $"檔案裡沒有可用的{kind}。" : $"已匯入 {added} 筆，略過 {incoming.Count - added} 筆重複。";
        };
        Refill(); form.AcceptButton=add; form.CancelButton=close;
        colors.Apply(form);
        form.TopMost = true; form.StartPosition = FormStartPosition.CenterScreen;
        form.ShowDialog();
    }
    public static string? Edit(string text,int pixels)
    {
        using var form=Dialog("聽字 · 編輯辨識文字",new Size(650,355));
        var field=new TextBox{Multiline=true,ScrollBars=ScrollBars.Vertical,Dock=DockStyle.Fill,Text=text,AcceptsReturn=true,
            Font=new Font("Microsoft JhengHei UI",pixels,GraphicsUnit.Pixel),AccessibleName="辨識文字"};
        var actions=new FlowLayoutPanel{Dock=DockStyle.Bottom,Height=50,Padding=new Padding(4),FlowDirection=FlowDirection.RightToLeft};
        actions.Controls.Add(new Button{Text="套用",AutoSize=true,DialogResult=DialogResult.OK});
        var cancel=new Button{Text="取消",AutoSize=true,DialogResult=DialogResult.Cancel};actions.Controls.Add(cancel);
        form.CancelButton=cancel;form.Controls.Add(field);form.Controls.Add(actions);
        VoiceSettings.Load().Colors.Apply(form);
        return form.ShowDialog()==DialogResult.OK?field.Text:null;
    }
}
