using System.Runtime.InteropServices;
using System.Text.Json;
using VoiceInput.App.Services;

internal enum VoiceDeliveryMode { Auto, ClipboardOnly, PreviewOnly }

internal sealed record VoiceSettings(bool Online = false, int Modifiers = 3, Keys RecordKey = Keys.R, int TextSize = 20,
    string? MicrophoneId = null, string? MicrophoneName = null)
{
    public bool AutoInsert { get; init; } = true;
    public bool Simplified { get; init; }
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
        var button = new Button { Text=label, AutoSize=true, AutoSizeMode=AutoSizeMode.GrowAndShrink, MinimumSize=new Size(120,38), Padding=new Padding(12,5,12,5), Margin=new Padding(0,0,8,10), UseVisualStyleBackColor=false, BackColor=Color.White, ForeColor=Color.FromArgb(25,49,78), FlatStyle=FlatStyle.Flat, AccessibleName=label };
        button.FlatAppearance.BorderColor=Color.FromArgb(174,190,208);
        button.Tag = icon;
        button.Click += (_,_) => action(); return button;
    }
    static ComboBox Choices(string[] options,int index)
    {
        var box = new ComboBox { DropDownStyle = ComboBoxStyle.DropDownList, Dock = DockStyle.Fill };
        box.Items.AddRange(options); box.SelectedIndex = index; return box;
    }
    public static Form CreateSettings(VoiceSettings current, bool bound, Func<VoiceSettings,string?> apply, SettingsActions commands)
    {
        var form = Dialog("聽字・注音與語音輸入法 · 設定", new Size(600,620));
        form.MinimumSize = new Size(360, 360);
        form.BackColor = Color.FromArgb(243,246,250);
        var tabs = new PageHost { Dock = DockStyle.Fill };
        var voicePage = new Panel { BackColor = Color.White, Padding = new Padding(8) };
        var zhuyinPage = new Panel { BackColor = Color.White, Padding = new Padding(18), AutoScroll = true };
        tabs.Add("語音輸入", "mic", voicePage); tabs.Add("注音與工具", "keyboard", zhuyinPage);
        var lookPage = new Panel { BackColor = Color.White, Padding = new Padding(18), AutoScroll = true };
        tabs.Add("外觀與啟動", "theme", lookPage);
        var look = new TableLayoutPanel { Dock = DockStyle.Top, ColumnCount = 1, AutoSize = true };
        look.ColumnStyles.Add(new ColumnStyle(SizeType.Percent,100));
        void LookRow(string label, Control control) {
            look.Controls.Add(new Label { Text=label, AutoSize=true, ForeColor=Color.FromArgb(25,49,78), Font=new Font(form.Font,FontStyle.Bold), Margin=new Padding(2,14,0,6) });
            control.AccessibleName=label; control.Margin=new Padding(2,0,2,4); look.Controls.Add(control);
        }
        var appearance = Choices(new[]{"跟隨 Windows","淺色","深色"}, (int)current.Appearance);
        var accent = Choices(new[]{"聽字靛色","跟隨 Windows 強調色"}, current.SystemAccent ? 1 : 0);
        var startup = new CheckBox { Text="登入 Windows 時啟動語音浮窗", AutoSize=true, Checked=LoginStartup.Enabled };
        LookRow("色系", appearance); LookRow("強調色", accent); LookRow("啟動", startup);
        look.Controls.Add(new Label { AutoSize=true, MaximumSize=new Size(520,0), ForeColor=Color.FromArgb(66,83,103), Margin=new Padding(2,14,2,0),
            Text="色系會套用到設定、浮條、系統匣選單，以及注音的候選窗與輸入行。變更在按下「儲存設定」後生效。" });
        lookPage.Controls.Add(look);
        var aboutPage = new Panel { BackColor = Color.White, Padding = new Padding(18), AutoScroll = true };
        tabs.Add("關於", "info", aboutPage);
        var about = new FlowLayoutPanel { Dock = DockStyle.Top, AutoSize = true, FlowDirection = FlowDirection.TopDown, WrapContents = false };
        Label AboutText(string text) => new Label { Text=text, AutoSize=true, MaximumSize=new Size(520,0), ForeColor=Color.FromArgb(66,83,103), Margin=new Padding(2,0,2,10) };
        about.Controls.Add(new Label { Text="聽字・注音與語音輸入法", AutoSize=true, Font=new Font(form.Font.FontFamily,14,FontStyle.Bold), ForeColor=Color.FromArgb(18,113,112), Margin=new Padding(2,4,2,4) });
        about.Controls.Add(new Label { Text="版本 " + (typeof(VoiceDialogs).Assembly.GetName().Version?.ToString(3) ?? "未知"), AutoSize=true, ForeColor=Color.FromArgb(25,49,78), Margin=new Padding(2,0,2,12) });
        about.Controls.Add(AboutText("Windows 的繁體中文注音與語音輸入。注音組字、學習資料、自訂詞和本機語音辨識都在這台電腦上處理；只有選用線上辨識時，音檔才會送到第三方服務。"));
        about.Controls.Add(AboutText("安裝包附的注音詞庫取自 McBopomofo（MIT 授權），授權全文在安裝資料夾的 McBopomofo-LICENSE.txt。"));
        var aboutButtons = new FlowLayoutPanel { AutoSize=true, WrapContents=true, Margin=new Padding(2,4,0,0) };
        aboutButtons.Controls.Add(ActionButton("專案首頁", () => OpenTarget("https://github.com/skydreamer0/tingzi"), "link"));
        aboutButtons.Controls.Add(ActionButton("回報問題", () => OpenTarget("https://github.com/skydreamer0/tingzi/issues"), "link"));
        aboutButtons.Controls.Add(ActionButton("開啟個人資料夾", () => OpenTarget(Path.GetDirectoryName(UserPhrases.FilePath)!), "folder"));
        about.Controls.Add(aboutButtons);
        about.Controls.Add(AboutText("個人資料夾存放設定、學習資料和自訂詞；刪除它就會清除這台電腦上所有的個人資料。"));
        aboutPage.Controls.Add(about);
        aboutPage.ClientSizeChanged += (_,_) => {
            foreach (var label in about.Controls.OfType<Label>()) label.MaximumSize=new Size(Math.Max(200,aboutPage.ClientSize.Width-52),0);
        };
        var header = new TableLayoutPanel { Dock = DockStyle.Top, AutoSize = true, BackColor=Color.White, Padding = new Padding(20,16,20,12), ColumnCount = 2 };
        header.ColumnStyles.Add(new ColumnStyle(SizeType.Percent,100));
        header.ColumnStyles.Add(new ColumnStyle(SizeType.AutoSize));
        var brand = new FlowLayoutPanel { AutoSize=true, WrapContents=false, Margin=Padding.Empty, Padding=Padding.Empty };
        var brandImage=form.Icon?.ToBitmap();
        brand.Controls.Add(new PictureBox { Image=brandImage, SizeMode=PictureBoxSizeMode.Zoom, Size=new Size(28,28), Margin=new Padding(0,1,8,0), TabStop=false });
        brand.Controls.Add(new Label { Text="聽字・注音與語音輸入法", Font=new Font(form.Font.FontFamily,14,FontStyle.Bold), ForeColor=Color.FromArgb(18,113,112), AutoSize=true, Margin=new Padding(0,0,0,0) });
        form.Disposed += (_,_) => brandImage?.Dispose();
        header.Controls.Add(brand,0,0);
        var recover = ActionButton("返回浮條", form.Close, "back");
        header.Controls.Add(recover,1,0); recover.Margin=new Padding(8,0,0,0); recover.Anchor=AnchorStyles.Right | AnchorStyles.Top; header.TabIndex=0; recover.TabIndex=0; tabs.TabIndex=1;
        var footer = new FlowLayoutPanel { Dock = DockStyle.Bottom, AutoSize = true, BackColor=Color.White, WrapContents=false, Padding = new Padding(20,12,20,12), FlowDirection = FlowDirection.RightToLeft };
        form.Controls.Add(tabs); form.Controls.Add(footer); form.Controls.Add(header);
        var panel = new TableLayoutPanel { Dock = DockStyle.Top, Padding = new Padding(12), ColumnCount = 1, AutoSize = true };
        panel.ColumnStyles.Add(new ColumnStyle(SizeType.Percent,100));
        voicePage.AutoScroll = true;
        void Row(int i,string label,Control control) {
            panel.Controls.Add(new Label { Text=label, AutoSize=true, Anchor=AnchorStyles.Left, ForeColor=Color.FromArgb(25,49,78), Font=new Font(form.Font,FontStyle.Bold), Margin=new Padding(2,14,0,6) },0,i*2);
            control.TabIndex=i; control.AccessibleName=label; control.Margin=new Padding(2,0,2,4); panel.Controls.Add(control,0,i*2+1);
        }
        var engine = Choices(new[]{"本機 Whisper","Codex 線上辨識（第三方橋接）"}, current.Online?1:0);
        int[] mods = {3,6,5};
        var mod = Choices(mods.Select(VoiceSettings.ModifierText).ToArray(),Array.IndexOf(mods,current.Modifiers));
        var keys = new[]{"停用"}.Concat(Enumerable.Range('A',26).Select(c=>((char)c).ToString())).ToArray();
        var key = Choices(keys,current.RecordKey == Keys.None ? 0 : (int)current.RecordKey-(int)Keys.A+1);
        var hotkeys = new TableLayoutPanel { ColumnCount=2, RowCount=1, Dock=DockStyle.Fill, AutoSize=true, AutoSizeMode=AutoSizeMode.GrowAndShrink, Padding=Padding.Empty };
        hotkeys.RowStyles.Add(new RowStyle(SizeType.AutoSize));
        mod.Margin=new Padding(0,0,8,0); key.Margin=Padding.Empty;
        void SizeHotkeys() {
            var height=Math.Max(mod.PreferredHeight+mod.Margin.Vertical,key.PreferredHeight+key.Margin.Vertical)+2;
            hotkeys.MinimumSize=new Size(0,height);
        }
        hotkeys.DpiChangedAfterParent += (_,_) => SizeHotkeys();
        hotkeys.FontChanged += (_,_) => SizeHotkeys();
        hotkeys.ColumnStyles.Add(new ColumnStyle(SizeType.Percent,65)); hotkeys.ColumnStyles.Add(new ColumnStyle(SizeType.Percent,35));
        hotkeys.Controls.Add(mod,0,0); hotkeys.Controls.Add(key,1,0); SizeHotkeys();
        int[] sizes={18,20,24};
        var size = Choices(new[]{"18 px","20 px","24 px"},Array.IndexOf(sizes,current.TextSize));
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
        var microphone = Choices(labels.ToArray(), selected);
        microphone.AccessibleName = "麥克風";
        var meter = new LevelMeter { Height = 8, Dock = DockStyle.Fill, Margin = new Padding(0,6,0,0), Track = current.Colors.Border, Bar = current.Colors.Accent };
        var test = ActionButton("測試", () => { }, "mic"); test.MinimumSize = new Size(96,32); test.Padding = new Padding(8,2,8,2); test.Margin = new Padding(8,0,0,0);
        var micRow = new TableLayoutPanel { ColumnCount=2, RowCount=2, Dock=DockStyle.Fill, AutoSize=true, AutoSizeMode=AutoSizeMode.GrowAndShrink, Padding=Padding.Empty };
        micRow.ColumnStyles.Add(new ColumnStyle(SizeType.Percent,100)); micRow.ColumnStyles.Add(new ColumnStyle(SizeType.AutoSize));
        microphone.Margin = Padding.Empty; microphone.Anchor = AnchorStyles.Left | AnchorStyles.Right;
        micRow.Controls.Add(microphone,0,0); micRow.Controls.Add(test,1,0); micRow.Controls.Add(meter,0,1); micRow.SetColumnSpan(meter,2);
        Row(0,"麥克風",micRow); micRow.AccessibleName = "麥克風與測試"; test.AccessibleName = "測試麥克風"; Row(1,"辨識方式",engine); Row(2,"錄音／停錄",hotkeys); Row(3,"預覽字級",size);
        var delivery = Choices(new[]{"自動插入並複製文字","只複製到剪貼簿","只顯示預覽"}, (int)current.EffectiveDeliveryMode);
        delivery.AccessibleName = "辨識完成後";
        Row(4,"辨識完成後",delivery);
        var script = Choices(new[]{"繁體中文","簡體中文"}, current.Simplified ? 1 : 0);
        script.AccessibleName = "輸出文字";
        Row(5,"輸出文字",script);
        var note = new Label { Dock=DockStyle.Fill, AutoSize=true, MaximumSize=new Size(480,0),
            Text="選用線上辨識時，音檔會經第三方 codex-asr 傳到 ChatGPT。\n「輸出文字」同時套用到語音結果與注音送出的字，逐字轉換。\n" + commands.RecognitionStatus + "\n錄音途中若切換欄位，辨識文字會留在浮窗預覽。", ForeColor = Color.FromArgb(66,83,103), Padding = new Padding(0,12,0,8) };
        panel.Controls.Add(note,0,12);
        var status = new Label { AutoSize=true, Dock=DockStyle.Fill, ForeColor=Color.DarkRed,
            Text=bound?"":"目前錄音快捷鍵被占用，請選其他組合。" };
        panel.Controls.Add(status,0,13);
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
        var save = ActionButton("儲存設定", () => { }, "check"); save.BackColor=Color.FromArgb(18,113,112); save.ForeColor=Color.White; var cancel=ActionButton("取消", form.Close, "close"); cancel.DialogResult=DialogResult.Cancel;
        save.Margin=new Padding(8,0,0,0); cancel.Margin=Padding.Empty;
        footer.TabIndex=2; save.TabIndex=1; cancel.TabIndex=0; footer.Controls.Add(save); footer.Controls.Add(cancel);
        var tools = new FlowLayoutPanel { Dock = DockStyle.Top, AutoSize = true, FlowDirection = FlowDirection.TopDown, WrapContents = false, Padding = new Padding(0,0,0,12) };
        void Tool(string title, string explanation, Control button) {
            tools.Controls.Add(new Label { Text=title, AutoSize=true, Font=new Font(form.Font,FontStyle.Bold), ForeColor=Color.FromArgb(25,49,78), Margin=new Padding(0,8,0,4) });
            tools.Controls.Add(new Label { Text=explanation, AutoSize=true, MaximumSize=new Size(560,0), ForeColor=Color.FromArgb(66,83,103), Margin=new Padding(0,0,0,8) });
            tools.Controls.Add(button);
        }
        var zhuyin = ZhuyinOptions.Load();
        ComboBox Option(string title, string explanation, string[] options, int index) {
            var box = Choices(options, Math.Max(0,index)); box.Dock = DockStyle.None; box.Width = form.LogicalToDeviceUnits(300); box.AccessibleName = title; box.Margin = new Padding(0,0,0,6);
            tools.Controls.Add(new Label { Text=title, AutoSize=true, Font=new Font(form.Font,FontStyle.Bold), ForeColor=Color.FromArgb(25,49,78), Margin=new Padding(0,8,0,4) });
            if (explanation.Length > 0) tools.Controls.Add(new Label { Text=explanation, AutoSize=true, MaximumSize=new Size(560,0), ForeColor=Color.FromArgb(66,83,103), Margin=new Padding(0,0,0,6) });
            tools.Controls.Add(box); return box;
        }
        var keyLayout = Option("鍵盤配置", "注音符號在鍵盤上的排法。", new[]{"標準（大千）","倚天"}, zhuyin.Layout);
        var segment = Option("整句選字方式", "「整句最佳」比較整句的詞頻總和；「長詞優先」先定案長詞，再由左到右決定其餘的字。", new[]{"整句最佳","長詞優先"}, zhuyin.Staged ? 1 : 0);
        var candidateSize = Option("候選窗字級", "", ZhuyinOptions.CandidateSizes.Select(s => s + " px").ToArray(), Array.IndexOf(ZhuyinOptions.CandidateSizes, zhuyin.CandidateSize));
        var lineSize = Option("大字輸入行", "打字時在游標旁放大顯示正在組的字。", ZhuyinOptions.LineSizes.Select(s => s == 0 ? "關閉" : s + " px").ToArray(), Array.IndexOf(ZhuyinOptions.LineSizes, zhuyin.LineSize));
        tools.Controls.Add(new Label { Text="以上四項在按下「儲存設定」後，於重新開啟的應用程式生效。", AutoSize=true, MaximumSize=new Size(560,0), ForeColor=Color.FromArgb(66,83,103), Margin=new Padding(0,4,0,10) });
        var switchZhuyin = ActionButton("切換到注音輸入法", commands.SwitchZhuyin, "keyboard"); switchZhuyin.Enabled=commands.KeyboardAvailable;
        Tool("注音輸入", commands.KeyboardAvailable ? "切換後，在原本的應用程式內打字。" : "找不到注音切換工具。請重新安裝完整套件，再從 Windows 語言設定確認輸入法。", switchZhuyin);
        var typingPad = ActionButton("開啟本機打字板", commands.OpenTypingPad, "pad"); typingPad.Enabled=commands.TypingPadAvailable;
        Tool("本機打字板", commands.TypingPadAvailable ? "開啟獨立欄位練習注音與選字。" : "找不到打字板或注音詞表，請重新安裝完整套件。", typingPad);
        var phrases = ActionButton("管理自訂詞…", () => EditPhrases(form, commands.WordData!, current.Colors), "plus"); phrases.Enabled = commands.WordData != null;
        Tool("自訂詞", commands.WordData != null ? "加入姓名、專有名詞等詞語，打注音時優先出現。" : "找不到注音詞庫，請重新安裝完整套件。", phrases);
        var learning = new FlowLayoutPanel { AutoSize=true, WrapContents=true, Margin=new Padding(0), FlowDirection=FlowDirection.TopDown };
        var learningStatus = new Label { AutoSize=true, ForeColor=Color.FromArgb(66,83,103), Text=commands.GetLearning == null ? "未安裝本機學習工具" : "讀取學習狀態中…" };
        learning.Controls.Add(learningStatus);
        var learningButtons = new FlowLayoutPanel { Enabled=commands.GetLearning == null, AutoSize=true, WrapContents=true, Margin=new Padding(0) };
        learning.Controls.Add(learningButtons);
        foreach (bool enabled in new[]{true,false}) {
            var button=ActionButton(enabled ? "啟用本機學習" : "關閉本機學習", () => { }, "learn");
            button.Click += async (_,_) => {
                learningButtons.Enabled=false;
                try {
                    var error=await commands.SetLearning!(enabled);
                    if (!form.IsDisposed) learningStatus.Text=error ?? (enabled ? "目前已啟用本機學習" : "目前已關閉本機學習");
                } finally { if (!form.IsDisposed) learningButtons.Enabled=true; }
            };
            button.Enabled=commands.SetLearning != null; learningButtons.Controls.Add(button);
        }
        if (commands.LearningTool != null) {
            var export = ActionButton("匯出學習資料…", () => { }, "export");
            export.Click += async (_,_) => {
                using var pick = new SaveFileDialog { Filter="Tab 分隔文字 (*.tsv)|*.tsv", FileName="tingzi-learning.tsv", OverwritePrompt=true };
                if (pick.ShowDialog(form) != DialogResult.OK) return;
                try { File.Delete(pick.FileName); } catch (Exception ex) when (ex is IOException or UnauthorizedAccessException) { }
                bool done = await commands.LearningTool(new[]{"export", pick.FileName});
                if (!form.IsDisposed) learningStatus.Text = done ? "學習資料已匯出" : "沒有可匯出的學習資料，或無法寫入該位置";
            };
            var clear = ActionButton("清除學習資料", () => { }, "trash");
            clear.Click += async (_,_) => {
                if (MessageBox.Show(form, "會刪除所有記住的改字習慣，無法復原。要繼續嗎？", "聽字・注音與語音輸入法", MessageBoxButtons.OKCancel, MessageBoxIcon.Warning, MessageBoxDefaultButton.Button2) != DialogResult.OK) return;
                bool done = await commands.LearningTool(new[]{"clear","--confirm"});
                if (!form.IsDisposed) learningStatus.Text = done ? "學習資料已清除" : "沒有學習資料可清除，或無法寫入";
            };
            learningButtons.Controls.Add(export); learningButtons.Controls.Add(clear);
        }
        form.Shown += async (_,_) => {
            if (commands.GetLearning == null) return;
            var enabled=await commands.GetLearning();
            if (!form.IsDisposed) { learningStatus.Text=enabled.HasValue ? (enabled.Value ? "目前已啟用本機學習" : "目前已關閉本機學習") : "無法讀取目前的學習狀態"; learningButtons.Enabled=true; }
        };
        Tool("本機學習", commands.SetLearning == null ? "找不到本機學習工具，請安裝完整套件。" : "按下後立即套用；取消不會還原。", learning);
        Tool("Windows 語言與鍵盤設定", "管理 Windows 的鍵盤清單和輸入語言。", ActionButton("開啟 Windows 輸入法設定", commands.OpenInputSettings, "language"));
        zhuyinPage.Controls.Add(tools);
        zhuyinPage.ClientSizeChanged += (_,_) => {
            foreach (var label in tools.Controls.OfType<Label>()) label.MaximumSize=new Size(Math.Max(200,zhuyinPage.ClientSize.Width-52),0);
        };
        panel.ClientSizeChanged += (_,_) => note.MaximumSize=new Size(Math.Max(200,panel.ClientSize.Width-30),0);
        save.Click+=(_,_)=>{
            var index = microphone.SelectedIndex;
            var input = index > 0 && index <= inputs.Count ? inputs[index-1] : null;
            var candidate=new VoiceSettings(engine.SelectedIndex==1,mods[mod.SelectedIndex],key.SelectedIndex==0?Keys.None:(Keys)((int)Keys.A+key.SelectedIndex-1),sizes[size.SelectedIndex],
                index == 0 ? null : input?.Id ?? current.MicrophoneId, index == 0 ? null : input?.Name ?? current.MicrophoneName) { AutoInsert = delivery.SelectedIndex == 0, DeliveryMode = (VoiceDeliveryMode)delivery.SelectedIndex, Simplified = script.SelectedIndex == 1,
                Appearance = (ThemeMode)appearance.SelectedIndex, SystemAccent = accent.SelectedIndex == 1 };
            var error=apply(candidate);
            if(error==null) {
                try {
                    new ZhuyinOptions(keyLayout.SelectedIndex, segment.SelectedIndex == 1, ZhuyinOptions.CandidateSizes[candidateSize.SelectedIndex], ZhuyinOptions.LineSizes[lineSize.SelectedIndex]).Save();
                    LoginStartup.Set(startup.Checked);
                }
                catch (Exception ex) when (ex is UnauthorizedAccessException or System.Security.SecurityException or IOException) { }
                form.Close();
            } else { status.Text=error; tabs.Open(voicePage); voicePage.ScrollControlIntoView(status); }
        };
        form.AcceptButton=save;form.CancelButton=cancel;voicePage.Controls.Add(panel);
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
    static void EditPhrases(Form owner, string wordData, Theme colors)
    {
        using var form = Dialog("聽字 · 自訂詞", new Size(540,540));
        form.StartPosition = FormStartPosition.CenterParent; form.BackColor = Color.White; form.Padding = new Padding(16); form.MinimumSize = new Size(380,420);
        var words = UserPhrases.Load();
        var grid = new TableLayoutPanel { Dock = DockStyle.Fill, ColumnCount = 2, RowCount = 6 };
        grid.ColumnStyles.Add(new ColumnStyle(SizeType.Percent,100)); grid.ColumnStyles.Add(new ColumnStyle(SizeType.AutoSize));
        foreach (var style in new[]{ SizeType.AutoSize, SizeType.Percent, SizeType.AutoSize, SizeType.AutoSize, SizeType.AutoSize, SizeType.AutoSize })
            grid.RowStyles.Add(new RowStyle(style,100));
        var intro = new Label { AutoSize=true, MaximumSize=new Size(500,0), ForeColor=Color.FromArgb(66,83,103), Margin=new Padding(0,0,0,10),
            Text="加入的詞在打注音時優先出現。輸入詞語後，每個字會帶出讀音；破音字請從下拉選單選對的讀音。新詞在重新開啟的應用程式生效。" };
        var list = new ListBox { Dock=DockStyle.Fill, IntegralHeight=false, AccessibleName="已加入的詞", Margin=new Padding(0,0,0,10) };
        void Refill() { list.Items.Clear(); foreach (var entry in words) list.Items.Add(entry.Word + "　" + entry.Reading.Replace("-"," ")); }
        var word = new TextBox { Dock=DockStyle.Fill, MaxLength=UserPhrases.Longest, AccessibleName="新詞", PlaceholderText="輸入 2 到 8 個字的詞", Enabled=false, Margin=new Padding(0,4,0,6) };
        var add = ActionButton("加入", () => { }, "plus"); add.BackColor=Color.FromArgb(18,113,112); add.ForeColor=Color.White; add.Margin=new Padding(8,0,0,6); add.MinimumSize=new Size(96,34);
        var readings = new FlowLayoutPanel { Dock=DockStyle.Fill, AutoSize=true, WrapContents=true, Margin=Padding.Empty };
        var status = new Label { AutoSize=true, MaximumSize=new Size(500,0), ForeColor=Color.FromArgb(66,83,103), Text="讀取詞庫中…", Margin=new Padding(0,6,0,10) };
        var buttons = new FlowLayoutPanel { Dock=DockStyle.Fill, AutoSize=true, FlowDirection=FlowDirection.RightToLeft, Margin=Padding.Empty };
        var close = ActionButton("關閉", form.Close, "close"); close.Margin=new Padding(8,0,0,0);
        var remove = ActionButton("刪除選取的詞", () => { }, "trash"); remove.Margin=Padding.Empty;
        buttons.Controls.Add(close); buttons.Controls.Add(remove);
        grid.Controls.Add(intro,0,0); grid.SetColumnSpan(intro,2);
        grid.Controls.Add(list,0,1); grid.SetColumnSpan(list,2);
        grid.Controls.Add(word,0,2); grid.Controls.Add(add,1,2);
        grid.Controls.Add(readings,0,3); grid.SetColumnSpan(readings,2);
        grid.Controls.Add(status,0,4); grid.SetColumnSpan(status,2);
        grid.Controls.Add(buttons,0,5); grid.SetColumnSpan(buttons,2);
        form.Controls.Add(grid);
        UserPhrases.Lexicon? lexicon = null;
        bool Persist() {
            try { UserPhrases.Save(words); return true; }
            catch (Exception ex) when (ex is IOException or UnauthorizedAccessException) { status.Text="無法儲存自訂詞。"; return false; }
        }
        form.Shown += async (_,_) => {
            try { lexicon = await Task.Run(() => UserPhrases.Lexicon.Read(wordData)); }
            catch (Exception ex) when (ex is IOException or UnauthorizedAccessException) { }
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
                if (options.Count == 0) { status.Text=$"詞庫裡沒有「{character}」的讀音，無法加入這個詞。"; foreach (var box in boxes) box.Dispose(); return; }
                var choice = Choices(options.ToArray(),0); choice.Dock=DockStyle.None; choice.Width=form.LogicalToDeviceUnits(118);
                choice.AccessibleName=$"「{character}」的讀音"; choice.Margin=new Padding(0,0,6,6); boxes.Add(choice);
            }
            readings.Controls.AddRange(boxes.ToArray());
            colors.Apply(readings);
        };
        add.Click += (_,_) => {
            var text = word.Text.Trim();
            if (text.Length < 2) { status.Text="請輸入至少兩個字的詞。"; return; }
            if (readings.Controls.Count != text.Length) return;
            var reading = string.Join("-", readings.Controls.OfType<ComboBox>().Select(box => (string)box.SelectedItem!));
            words.RemoveAll(entry => entry.Word == text && entry.Reading == reading);
            words.Add((reading,text));
            if (!Persist()) return;
            Refill(); word.Clear(); status.Text=$"已加入「{text}」。"; word.Focus();
        };
        remove.Click += (_,_) => {
            int index = list.SelectedIndex;
            if (index < 0) { status.Text="請先在清單選一個詞。"; return; }
            var gone = words[index]; words.RemoveAt(index);
            if (!Persist()) { words.Insert(index,gone); return; }
            Refill(); status.Text=$"已刪除「{gone.Word}」。";
        };
        Refill(); form.AcceptButton=add; form.CancelButton=close;
        colors.Apply(form);
        form.ShowDialog(owner);
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
