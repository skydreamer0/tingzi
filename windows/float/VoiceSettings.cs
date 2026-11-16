using System.Runtime.InteropServices;
using System.Text.Json;
using VoiceInput.App.Services;

internal enum VoiceDeliveryMode { Auto, ClipboardOnly, PreviewOnly }

internal sealed record VoiceSettings(bool Online = false, int Modifiers = 3, Keys RecordKey = Keys.R, int TextSize = 20,
    string? MicrophoneId = null, string? MicrophoneName = null)
{
    public bool AutoInsert { get; init; } = true;
    public VoiceDeliveryMode? DeliveryMode { get; init; }
    [System.Text.Json.Serialization.JsonIgnore]
    public VoiceDeliveryMode EffectiveDeliveryMode => DeliveryMode ?? (AutoInsert ? VoiceDeliveryMode.Auto : VoiceDeliveryMode.PreviewOnly);
    static readonly string FilePath = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
        "Tingzi", "voice-settings.json");
    public bool Valid => (Modifiers is 3 or 5 or 6) && (RecordKey == Keys.None || (RecordKey >= Keys.A && RecordKey <= Keys.Z))
        && !(Modifiers == 3 && RecordKey == Keys.V) && TextSize is 18 or 20 or 24
        && (DeliveryMode == null || Enum.IsDefined(DeliveryMode.Value));
    public static VoiceSettings Load()
    {
        try { var value = JsonSerializer.Deserialize<VoiceSettings>(File.ReadAllText(FilePath)); if (value?.Valid == true) return value; }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException or JsonException) { }
        return new();
    }
    public void Save()
    {
        Directory.CreateDirectory(Path.GetDirectoryName(FilePath)!);
        var tmp = FilePath + "." + Guid.NewGuid().ToString("N") + ".tmp";
        try { File.WriteAllText(tmp, JsonSerializer.Serialize(this)); File.Move(tmp, FilePath, true); }
        finally { File.Delete(tmp); }
    }
    public string Gesture => RecordKey == Keys.None ? "停用" : ModifierText(Modifiers) + "+" + RecordKey;
    public static string ModifierText(int mods) => mods switch { 5 => "Alt+Shift", 6 => "Ctrl+Shift", _ => "Ctrl+Alt" };
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

internal sealed record SettingsActions(Action SwitchZhuyin, Action OpenTypingPad, Action OpenInputSettings, bool KeyboardAvailable, bool TypingPadAvailable, string RecognitionStatus, Action? OpenVendorPreferences = null, Func<bool, Task<string?>>? SetLearning = null, Func<Task<bool?>>? GetLearning = null);

internal static class VoiceDialogs
{
    static Form Dialog(string title, Size size) => new Form { Text = title, ClientSize = size, MinimumSize = size,
        Font = new Font("Microsoft JhengHei UI", 11), AutoScaleMode = AutoScaleMode.Dpi,
        Icon = Icon.ExtractAssociatedIcon(Application.ExecutablePath), StartPosition = FormStartPosition.CenterScreen };
    static Button ActionButton(string label, Action action)
    {
        var button = new Button { Text=label, AutoSize=true, AutoSizeMode=AutoSizeMode.GrowAndShrink, MinimumSize=new Size(120,38), Padding=new Padding(12,5,12,5), Margin=new Padding(0,0,8,10), UseVisualStyleBackColor=false, BackColor=Color.White, ForeColor=Color.FromArgb(25,49,78), FlatStyle=FlatStyle.Flat, AccessibleName=label };
        button.FlatAppearance.BorderColor=Color.FromArgb(174,190,208);
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
        var tabs = new TabControl { Dock = DockStyle.Fill, Padding = new Point(20,10) };
        var voicePage = new TabPage("語音輸入") { BackColor = Color.White, Padding = new Padding(8) };
        var zhuyinPage = new TabPage("注音與工具") { BackColor = Color.White, Padding = new Padding(18), AutoScroll = true };
        tabs.TabPages.Add(voicePage); tabs.TabPages.Add(zhuyinPage);
        var header = new TableLayoutPanel { Dock = DockStyle.Top, AutoSize = true, BackColor=Color.White, Padding = new Padding(20,16,20,12), ColumnCount = 2 };
        header.ColumnStyles.Add(new ColumnStyle(SizeType.Percent,100));
        header.ColumnStyles.Add(new ColumnStyle(SizeType.AutoSize));
        var brand = new FlowLayoutPanel { AutoSize=true, WrapContents=false, Margin=Padding.Empty, Padding=Padding.Empty };
        var brandImage=form.Icon?.ToBitmap();
        brand.Controls.Add(new PictureBox { Image=brandImage, SizeMode=PictureBoxSizeMode.Zoom, Size=new Size(28,28), Margin=new Padding(0,1,8,0), TabStop=false });
        brand.Controls.Add(new Label { Text="聽字・注音與語音輸入法", Font=new Font(form.Font.FontFamily,14,FontStyle.Bold), ForeColor=Color.FromArgb(18,113,112), AutoSize=true, Margin=new Padding(0,0,0,0) });
        form.Disposed += (_,_) => brandImage?.Dispose();
        header.Controls.Add(brand,0,0);
        var recover = ActionButton("返回浮條", form.Close);
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
        Row(0,"麥克風",microphone); Row(1,"辨識方式",engine); Row(2,"錄音／停錄",hotkeys); Row(3,"預覽字級",size);
        var delivery = Choices(new[]{"自動插入並複製文字","只複製到剪貼簿","只顯示預覽"}, (int)current.EffectiveDeliveryMode);
        delivery.AccessibleName = "辨識完成後";
        Row(4,"辨識完成後",delivery);
        var note = new Label { Dock=DockStyle.Fill, AutoSize=true, MaximumSize=new Size(480,0),
            Text="選用線上辨識時，音檔會經第三方 codex-asr 傳到 ChatGPT。\n" + commands.RecognitionStatus + "\n錄音途中若切換欄位，辨識文字會留在浮窗預覽。", ForeColor = Color.FromArgb(66,83,103), Padding = new Padding(0,12,0,8) };
        panel.Controls.Add(note,0,10);
        var status = new Label { AutoSize=true, Dock=DockStyle.Fill, ForeColor=Color.DarkRed,
            Text=bound?"":"目前錄音快捷鍵被占用，請選其他組合。" };
        panel.Controls.Add(status,0,11);
        var save = ActionButton("儲存設定", () => { }); save.BackColor=Color.FromArgb(18,113,112); save.ForeColor=Color.White; var cancel=ActionButton("取消", form.Close); cancel.DialogResult=DialogResult.Cancel;
        save.Margin=new Padding(8,0,0,0); cancel.Margin=Padding.Empty;
        footer.TabIndex=2; save.TabIndex=1; cancel.TabIndex=0; footer.Controls.Add(save); footer.Controls.Add(cancel);
        var tools = new FlowLayoutPanel { Dock = DockStyle.Top, AutoSize = true, FlowDirection = FlowDirection.TopDown, WrapContents = false, Padding = new Padding(0,0,0,12) };
        void Tool(string title, string explanation, Control button) {
            tools.Controls.Add(new Label { Text=title, AutoSize=true, Font=new Font(form.Font,FontStyle.Bold), ForeColor=Color.FromArgb(25,49,78), Margin=new Padding(0,8,0,4) });
            tools.Controls.Add(new Label { Text=explanation, AutoSize=true, MaximumSize=new Size(560,0), ForeColor=Color.FromArgb(66,83,103), Margin=new Padding(0,0,0,8) });
            tools.Controls.Add(button);
        }
        var switchZhuyin = ActionButton("切換到注音輸入法", commands.SwitchZhuyin); switchZhuyin.Enabled=commands.KeyboardAvailable;
        Tool("注音輸入", commands.KeyboardAvailable ? "切換後，在原本的應用程式內打字。注音能力以這台電腦已安裝的輸入法為準。" : "找不到注音切換工具。請重新安裝完整套件，再從 Windows 語言設定確認輸入法。", switchZhuyin);
        var typingPad = ActionButton("開啟本機打字板", commands.OpenTypingPad); typingPad.Enabled=commands.TypingPadAvailable;
        var vendor = ActionButton("開啟 V12 偏好設定", () => commands.OpenVendorPreferences?.Invoke());
        vendor.Enabled = commands.OpenVendorPreferences != null;
        Tool("自然輸入法 V12", vendor.Enabled ? "使用已安裝的自然輸入法調整注音和選字偏好。" : "安裝自然輸入法 V12 後，可從這裡開啟原廠偏好設定。", vendor);
        Tool("本機打字板", commands.TypingPadAvailable ? "開啟獨立欄位練習注音與選字。" : "打字板需要已安裝的 V12 或本機自備詞表。", typingPad);
        var learning = new FlowLayoutPanel { AutoSize=true, WrapContents=true, Margin=new Padding(0), FlowDirection=FlowDirection.TopDown };
        var learningStatus = new Label { AutoSize=true, ForeColor=Color.FromArgb(66,83,103), Text=commands.GetLearning == null ? "未安裝本機學習工具" : "讀取學習狀態中…" };
        learning.Controls.Add(learningStatus);
        var learningButtons = new FlowLayoutPanel { Enabled=commands.GetLearning == null, AutoSize=true, WrapContents=true, Margin=new Padding(0) };
        learning.Controls.Add(learningButtons);
        foreach (bool enabled in new[]{true,false}) {
            var button=ActionButton(enabled ? "啟用本機學習" : "關閉本機學習", () => { });
            button.Click += async (_,_) => {
                learningButtons.Enabled=false;
                try {
                    var error=await commands.SetLearning!(enabled);
                    if (!form.IsDisposed) learningStatus.Text=error ?? (enabled ? "目前已啟用本機學習" : "目前已關閉本機學習");
                } finally { if (!form.IsDisposed) learningButtons.Enabled=true; }
            };
            button.Enabled=commands.SetLearning != null; learningButtons.Controls.Add(button);
        }
        form.Shown += async (_,_) => {
            if (commands.GetLearning == null) return;
            var enabled=await commands.GetLearning();
            if (!form.IsDisposed) { learningStatus.Text=enabled.HasValue ? (enabled.Value ? "目前已啟用本機學習" : "目前已關閉本機學習") : "無法讀取目前的學習狀態"; learningButtons.Enabled=true; }
        };
        Tool("自有引擎的本機學習", commands.SetLearning == null ? "找不到本機學習工具，請安裝完整套件。" : "只影響自有注音引擎。按下後立即套用；取消不會還原。V12 學習請使用原廠偏好設定。", learning);
        Tool("Windows 語言與鍵盤設定", "管理 Windows 的鍵盤清單和輸入語言。", ActionButton("開啟 Windows 輸入法設定", commands.OpenInputSettings));
        zhuyinPage.Controls.Add(tools);
        zhuyinPage.ClientSizeChanged += (_,_) => {
            foreach (var label in tools.Controls.OfType<Label>()) label.MaximumSize=new Size(Math.Max(200,zhuyinPage.ClientSize.Width-52),0);
        };
        panel.ClientSizeChanged += (_,_) => note.MaximumSize=new Size(Math.Max(200,panel.ClientSize.Width-30),0);
        save.Click+=(_,_)=>{
            var index = microphone.SelectedIndex;
            var input = index > 0 && index <= inputs.Count ? inputs[index-1] : null;
            var candidate=new VoiceSettings(engine.SelectedIndex==1,mods[mod.SelectedIndex],key.SelectedIndex==0?Keys.None:(Keys)((int)Keys.A+key.SelectedIndex-1),sizes[size.SelectedIndex],
                index == 0 ? null : input?.Id ?? current.MicrophoneId, index == 0 ? null : input?.Name ?? current.MicrophoneName) { AutoInsert = delivery.SelectedIndex == 0, DeliveryMode = (VoiceDeliveryMode)delivery.SelectedIndex };
            var error=apply(candidate);
            if(error==null) form.Close(); else { status.Text=error; tabs.SelectedTab=voicePage; voicePage.ScrollControlIntoView(status); }
        };
        form.AcceptButton=save;form.CancelButton=cancel;voicePage.Controls.Add(panel);
        return form;
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
        return form.ShowDialog()==DialogResult.OK?field.Text:null;
    }
}
