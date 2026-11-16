using System.Drawing.Drawing2D;
using System.Globalization;
using System.Runtime.InteropServices;
using System.Text.RegularExpressions;
using System.Xml.Linq;
using Microsoft.Win32;

internal enum ThemeMode { System, Light, Dark }

/// <summary>Colours for every window the app draws, resolved from the user's choice and Windows.</summary>
internal sealed record Theme(bool Dark, Color Back, Color Surface, Color Input, Color Text, Color Muted, Color Border,
    Color Accent, Color OnAccent, Color Danger)
{
    // The dialogs are written with these fixed colours; Apply maps each one to the active theme.
    public static readonly Color MarkText = Color.FromArgb(25, 49, 78), MarkMuted = Color.FromArgb(66, 83, 103),
        MarkPrimary = Color.FromArgb(18, 113, 112), MarkBack = Color.FromArgb(243, 246, 250);

    public static Theme Resolve(ThemeMode mode, bool systemAccent)
    {
        bool dark = mode == ThemeMode.Dark || (mode == ThemeMode.System && SystemDark());
        Color accent = dark ? Color.FromArgb(174, 182, 255) : Color.FromArgb(48, 56, 151);
        if (systemAccent && WindowsAccent() is Color chosen) accent = chosen;
        var onAccent = Luminance(accent) > 0.55 ? Color.FromArgb(24, 27, 60) : Color.White;
        return dark
            ? new Theme(true, Color.FromArgb(24, 26, 38), Color.FromArgb(32, 35, 50), Color.FromArgb(44, 48, 68),
                Color.FromArgb(232, 235, 255), Color.FromArgb(166, 172, 205), Color.FromArgb(74, 79, 112), accent, onAccent, Color.FromArgb(255, 140, 130))
            : new Theme(false, Color.FromArgb(243, 245, 252), Color.White, Color.FromArgb(245, 246, 253),
                Color.FromArgb(31, 36, 85), Color.FromArgb(84, 90, 128), Color.FromArgb(190, 196, 228), accent, onAccent, Color.FromArgb(176, 40, 30));
    }

    public static bool SystemDark()
    {
        try
        {
            using var key = Registry.CurrentUser.OpenSubKey(@"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize");
            return key?.GetValue("AppsUseLightTheme") is int light && light == 0;
        }
        catch (Exception ex) when (ex is System.Security.SecurityException or UnauthorizedAccessException or IOException) { return false; }
    }

    static Color? WindowsAccent()
    {
        try
        {
            using var key = Registry.CurrentUser.OpenSubKey(@"Software\Microsoft\Windows\DWM");
            if (key?.GetValue("AccentColor") is int abgr)
                return Color.FromArgb(abgr & 0xff, (abgr >> 8) & 0xff, (abgr >> 16) & 0xff);
        }
        catch (Exception ex) when (ex is System.Security.SecurityException or UnauthorizedAccessException or IOException) { }
        return null;
    }

    static double Luminance(Color c) => (0.2126 * c.R + 0.7152 * c.G + 0.0722 * c.B) / 255.0;

    /// <summary>Recolours a finished dialog and tints its icon buttons.</summary>
    public void Apply(Control root)
    {
        if (root is Form form)
        {
            form.BackColor = Back; form.ForeColor = Text;
            void TitleBar() { int on = Dark ? 1 : 0; DwmSetWindowAttribute(form.Handle, 20, ref on, sizeof(int)); }
            if (form.IsHandleCreated) TitleBar(); else form.HandleCreated += (_, _) => TitleBar();
        }
        foreach (Control control in root.Controls) Paint(control);
    }

    void Paint(Control control)
    {
        if (Dark && control is ScrollableControl or ComboBox or TextBox or ListBox)
        {
            // Scroll bars and drop-down lists take their dark look from the system theme.
            void Darken() => SetWindowTheme(control.Handle, "DarkMode_Explorer", null);
            if (control.IsHandleCreated) Darken(); else control.HandleCreated += (_, _) => Darken();
        }
        switch (control)
        {
            case Button button:
                bool primary = button.BackColor.ToArgb() == MarkPrimary.ToArgb();
                button.BackColor = primary ? Accent : Input;
                button.ForeColor = primary ? OnAccent : Text;
                button.FlatAppearance.BorderColor = primary ? Accent : Border;
                if (button.Tag is string icon)
                {
                    button.Image = Icons.Render(icon, button.LogicalToDeviceUnits(20), button.ForeColor);
                    button.TextImageRelation = TextImageRelation.ImageBeforeText;
                    button.ImageAlign = ContentAlignment.MiddleLeft;
                }
                break;
            case ComboBox box:
                box.FlatStyle = FlatStyle.Flat; box.BackColor = Input; box.ForeColor = Text;
                break;
            case TextBox field:
                field.BackColor = Input; field.ForeColor = Text; field.BorderStyle = BorderStyle.FixedSingle;
                break;
            case Card card:
                card.Fill = Surface; card.Edge = Border; card.BackColor = Back;
                break;
            case Toggle toggle:
                toggle.Colors = this; toggle.Invalidate();
                break;
            case PictureBox picture when picture.Tag is string glyph:
                picture.Image = Icons.Render(glyph, picture.LogicalToDeviceUnits(20), Accent);
                break;
            case ListBox list:
                list.BackColor = Input; list.ForeColor = Text; list.BorderStyle = BorderStyle.FixedSingle;
                break;
            case CheckBox check:
                check.ForeColor = Text;
                break;
            case Label label:
                int fore = label.ForeColor.ToArgb();
                label.ForeColor = fore == MarkMuted.ToArgb() ? Muted : fore == MarkPrimary.ToArgb() ? Accent
                    : fore == Color.DarkRed.ToArgb() ? Danger : Text;
                break;
            default:
                int backArgb = control.BackColor.ToArgb();
                if (backArgb == Color.White.ToArgb()) control.BackColor = Surface;
                else if (backArgb == MarkBack.ToArgb()) control.BackColor = Back;
                break;
        }
        foreach (Control child in control.Controls) Paint(child);
        if (control is PageHost host) host.Use(this);
    }

    /// <summary>Colours and icon tint for the tray menu.</summary>
    public void Apply(ToolStripDropDown menu)
    {
        menu.Renderer = new ToolStripProfessionalRenderer(new MenuColors(this)) { RoundedEdges = false };
        menu.BackColor = Surface; menu.ForeColor = Text;
        foreach (ToolStripItem item in menu.Items)
        {
            item.ForeColor = Text;
            if (item.Tag is string icon) item.Image = Icons.Render(icon, menu.LogicalToDeviceUnits(16), Text);
        }
    }

    sealed class MenuColors(Theme theme) : ProfessionalColorTable
    {
        public override Color ToolStripDropDownBackground => theme.Surface;
        public override Color ImageMarginGradientBegin => theme.Surface;
        public override Color ImageMarginGradientMiddle => theme.Surface;
        public override Color ImageMarginGradientEnd => theme.Surface;
        public override Color MenuBorder => theme.Border;
        public override Color MenuItemBorder => theme.Accent;
        public override Color MenuItemSelected => theme.Input == theme.Surface ? theme.Back : theme.Input;
        public override Color SeparatorDark => theme.Border;
        public override Color SeparatorLight => theme.Surface;
    }

    [DllImport("dwmapi.dll")] static extern int DwmSetWindowAttribute(nint window, int attribute, ref int value, int size);
    [DllImport("uxtheme.dll", CharSet = CharSet.Unicode)] static extern int SetWindowTheme(nint window, string? app, string? ids);
}

/// <summary>Pages with a navigation column beside them, the way Windows Settings lays itself out.</summary>
internal sealed class PageHost : Panel
{
    readonly Panel side = new() { Dock = DockStyle.Left, Width = 184, Padding = new Padding(10, 16, 6, 10), BackColor = Theme.MarkBack };
    readonly FlowLayoutPanel nav = new() { Dock = DockStyle.Fill, FlowDirection = FlowDirection.TopDown, WrapContents = false, BackColor = Color.Transparent };
    readonly Panel body = new() { Dock = DockStyle.Fill, BackColor = Theme.MarkBack };
    readonly List<(RoundButton Tab, Panel Page)> pages = new();
    readonly Bitmap mark = Brand.Render(64);
    Theme? theme;
    int selected;

    public PageHost()
    {
        var brand = new TableLayoutPanel { Dock = DockStyle.Top, AutoSize = true, ColumnCount = 2, RowCount = 2, BackColor = Color.Transparent, Padding = new Padding(4, 0, 0, 14) };
        brand.ColumnStyles.Add(new ColumnStyle(SizeType.AutoSize)); brand.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100));
        var picture = new PictureBox { Image = mark, SizeMode = PictureBoxSizeMode.Zoom, Size = new Size(36, 36), Margin = new Padding(0, 2, 10, 0), TabStop = false, BackColor = Color.Transparent };
        brand.Controls.Add(picture, 0, 0); brand.SetRowSpan(picture, 2);
        brand.Controls.Add(new Label { Text = "聽字", AutoSize = true, Font = new Font("Microsoft JhengHei UI", 14, FontStyle.Bold), ForeColor = Theme.MarkText, BackColor = Color.Transparent, Margin = new Padding(0, 0, 0, 0) }, 1, 0);
        brand.Controls.Add(new Label { Text = "注音與語音輸入法", AutoSize = true, Font = new Font("Microsoft JhengHei UI", 8.5f), ForeColor = Theme.MarkMuted, BackColor = Color.Transparent, Margin = new Padding(1, 0, 0, 0) }, 1, 1);
        side.Controls.Add(nav); side.Controls.Add(brand);
        Controls.Add(body); Controls.Add(side);
        Disposed += (_, _) => mark.Dispose();
    }

    public void Add(string title, string icon, Panel page)
    {
        var tab = new RoundButton { Text = title, Tag = icon, Flat = true, AlignLeft = true, Size = new Size(166, 40), Margin = new Padding(0, 0, 0, 4),
            AccessibleName = title, AccessibleRole = AccessibleRole.PageTab, BackColor = Color.White, ForeColor = Theme.MarkText };
        int index = pages.Count;
        tab.Click += (_, _) => SelectedIndex = index;
        page.Dock = DockStyle.Fill; page.Visible = index == 0;
        nav.Controls.Add(tab); body.Controls.Add(page); pages.Add((tab, page));
    }

    public int SelectedIndex
    {
        get => selected;
        set
        {
            if (value < 0 || value >= pages.Count) return;
            selected = value;
            for (int i = 0; i < pages.Count; i++) pages[i].Page.Visible = i == value;
            Restyle();
        }
    }

    public void Open(Panel page) => SelectedIndex = pages.FindIndex(entry => entry.Page == page);

    public void Use(Theme colors) { theme = colors; side.BackColor = body.BackColor = colors.Back; Restyle(); }

    void Restyle()
    {
        if (theme == null) return;
        for (int i = 0; i < pages.Count; i++)
        {
            var tab = pages[i].Tab;
            bool on = i == selected;
            tab.BackColor = theme.Surface;
            tab.Indicator = on ? theme.Accent : null;
            tab.ForeColor = theme.Text;
            tab.Image = Icons.Render((string)tab.Tag!, tab.LogicalToDeviceUnits(18), on ? theme.Accent : theme.Muted);
            tab.AccessibleDescription = on ? "已選取" : null;
            tab.Invalidate();
        }
    }
}

/// <summary>A thin bar showing how loud the microphone hears the room.</summary>
internal sealed class LevelMeter : Control
{
    float level;
    public Color Track = Color.Gainsboro, Bar = Color.SeaGreen;
    public LevelMeter()
    {
        SetStyle(ControlStyles.OptimizedDoubleBuffer | ControlStyles.AllPaintingInWmPaint | ControlStyles.UserPaint | ControlStyles.ResizeRedraw, true);
        TabStop = false; AccessibleName = "麥克風音量"; AccessibleRole = AccessibleRole.ProgressBar;
    }
    public float Level { get => level; set { level = Math.Clamp(value, 0f, 1f); Invalidate(); } }
    protected override void OnPaint(PaintEventArgs e)
    {
        using var track = new SolidBrush(Track); using var bar = new SolidBrush(Bar);
        e.Graphics.FillRectangle(track, ClientRectangle);
        // Square root keeps quiet speech visible.
        e.Graphics.FillRectangle(bar, 0, 0, (int)(Width * Math.Sqrt(level)), Height);
    }
}

/// <summary>Draws the icon set from the embedded SVG files in one colour.</summary>
internal static class Icons
{
    static readonly Dictionary<string, XElement> cache = new();
    static readonly Regex token = new(@"[MLHVCZmlhvcz]|-?\d*\.?\d+", RegexOptions.Compiled);

    public static Bitmap Render(string name, int pixels, Color color)
    {
        var bitmap = new Bitmap(pixels, pixels);
        using var graphics = Graphics.FromImage(bitmap);
        graphics.SmoothingMode = SmoothingMode.AntiAlias;
        graphics.PixelOffsetMode = PixelOffsetMode.HighQuality;
        graphics.ScaleTransform(pixels / 24f, pixels / 24f);
        using var pen = new Pen(color, 2f) { StartCap = LineCap.Round, EndCap = LineCap.Round, LineJoin = LineJoin.Round };
        using var brush = new SolidBrush(color);
        foreach (var element in Load(name).Elements())
        {
            using var path = Shape(element);
            if (path == null) continue;
            if ((string?)element.Attribute("fill") == "currentColor") graphics.FillPath(brush, path);
            graphics.DrawPath(pen, path);
        }
        return bitmap;
    }

    public static bool Exists(string name) =>
        typeof(Icons).Assembly.GetManifestResourceNames().Contains("icons/" + name + ".svg");

    static XElement Load(string name)
    {
        lock (cache)
        {
            if (cache.TryGetValue(name, out var known)) return known;
            using var stream = typeof(Icons).Assembly.GetManifestResourceStream("icons/" + name + ".svg")
                ?? throw new ArgumentException("Unknown icon " + name);
            return cache[name] = XElement.Load(stream);
        }
    }

    static float Number(XElement element, string attribute) =>
        float.Parse((string?)element.Attribute(attribute) ?? "0", CultureInfo.InvariantCulture);

    static GraphicsPath? Shape(XElement element)
    {
        var path = new GraphicsPath();
        switch (element.Name.LocalName)
        {
            case "circle":
                float r = Number(element, "r");
                path.AddEllipse(Number(element, "cx") - r, Number(element, "cy") - r, r * 2, r * 2);
                return path;
            case "line":
                path.AddLine(Number(element, "x1"), Number(element, "y1"), Number(element, "x2"), Number(element, "y2"));
                return path;
            case "rect":
                float x = Number(element, "x"), y = Number(element, "y"), w = Number(element, "width"), h = Number(element, "height");
                float corner = Math.Min(Number(element, "rx"), Math.Min(w, h) / 2) * 2;
                if (corner <= 0) { path.AddRectangle(new RectangleF(x, y, w, h)); return path; }
                path.AddArc(x, y, corner, corner, 180, 90); path.AddArc(x + w - corner, y, corner, corner, 270, 90);
                path.AddArc(x + w - corner, y + h - corner, corner, corner, 0, 90); path.AddArc(x, y + h - corner, corner, corner, 90, 90);
                path.CloseFigure();
                return path;
            case "path":
                Trace(path, (string?)element.Attribute("d") ?? "");
                return path;
            default:
                path.Dispose();
                return null;
        }
    }

    // Supports M L H V C Z in absolute and relative form, which is all the icon set uses.
    static void Trace(GraphicsPath path, string data)
    {
        var parts = token.Matches(data).Select(match => match.Value).ToArray();
        PointF current = default, start = default;
        char command = 'M';
        int i = 0;
        float Next() => float.Parse(parts[i++], CultureInfo.InvariantCulture);
        while (i < parts.Length)
        {
            if (char.IsLetter(parts[i][0])) command = parts[i++][0];
            bool relative = char.IsLower(command);
            PointF Point() { float px = Next(), py = Next(); return relative ? new PointF(current.X + px, current.Y + py) : new PointF(px, py); }
            switch (char.ToUpperInvariant(command))
            {
                case 'M':
                    current = start = Point(); path.StartFigure();
                    command = relative ? 'l' : 'L';
                    break;
                case 'L': { var to = Point(); path.AddLine(current, to); current = to; break; }
                case 'H': { float hx = Next(); var to = new PointF(relative ? current.X + hx : hx, current.Y); path.AddLine(current, to); current = to; break; }
                case 'V': { float vy = Next(); var to = new PointF(current.X, relative ? current.Y + vy : vy); path.AddLine(current, to); current = to; break; }
                case 'C': { var a = Point(); var b = Point(); var to = Point(); path.AddBezier(current, a, b, to); current = to; break; }
                case 'Z': path.CloseFigure(); current = start; break;
                default: return;
            }
        }
    }
}

/// <summary>Starts the voice window when the user signs in to Windows.</summary>
internal static class LoginStartup
{
    const string RunKey = @"Software\Microsoft\Windows\CurrentVersion\Run", Name = "Tingzi";
    static string Command => "\"" + Application.ExecutablePath + "\"";

    public static bool Enabled
    {
        get
        {
            using var key = Registry.CurrentUser.OpenSubKey(RunKey);
            return key?.GetValue(Name) is string;
        }
    }

    public static void Set(bool enabled)
    {
        using var key = Registry.CurrentUser.CreateSubKey(RunKey);
        if (enabled) key.SetValue(Name, Command); else key.DeleteValue(Name, false);
    }

    /// <summary>An upgrade moves the program to a new folder; keep the entry pointing at the running copy.</summary>
    public static void Refresh()
    {
        using var key = Registry.CurrentUser.OpenSubKey(RunKey, true);
        if (key?.GetValue(Name) is string current && !string.Equals(current, Command, StringComparison.OrdinalIgnoreCase))
            key.SetValue(Name, Command);
    }
}
