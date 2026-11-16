using System.Drawing.Drawing2D;

/// <summary>Shared drawing helpers for the controls the app paints itself.</summary>
internal static class Shapes
{
    public static GraphicsPath Rounded(RectangleF bounds, float radius)
    {
        var path = new GraphicsPath();
        float corner = Math.Max(1f, Math.Min(radius * 2, Math.Min(bounds.Width, bounds.Height)));
        path.AddArc(bounds.Left, bounds.Top, corner, corner, 180, 90);
        path.AddArc(bounds.Right - corner, bounds.Top, corner, corner, 270, 90);
        path.AddArc(bounds.Right - corner, bounds.Bottom - corner, corner, corner, 0, 90);
        path.AddArc(bounds.Left, bounds.Bottom - corner, corner, corner, 90, 90);
        path.CloseFigure();
        return path;
    }

    public static Color Blend(Color from, Color to, double amount) => Color.FromArgb(
        (int)Math.Round(from.R + (to.R - from.R) * amount), (int)Math.Round(from.G + (to.G - from.G) * amount),
        (int)Math.Round(from.B + (to.B - from.B) * amount));

    /// <summary>The colour actually visible behind a control, which a card paints itself.</summary>
    public static Color Behind(Control control)
    {
        for (var parent = control.Parent; parent != null; parent = parent.Parent)
        {
            if (parent is Card card) return card.Fill;
            if (parent.BackColor.A == 255) return parent.BackColor;
        }
        return SystemColors.Control;
    }
}

/// <summary>The product mark, drawn in code so it looks the same wherever the app runs.</summary>
internal static class Brand
{
    public static Bitmap Render(int pixels)
    {
        var bitmap = new Bitmap(pixels, pixels);
        using var graphics = Graphics.FromImage(bitmap);
        graphics.SmoothingMode = SmoothingMode.AntiAlias;
        graphics.PixelOffsetMode = PixelOffsetMode.HighQuality;
        graphics.ScaleTransform(pixels / 64f, pixels / 64f);
        var ink = Color.FromArgb(48, 56, 151);
        using (var cap = Shapes.Rounded(new RectangleF(2, 6, 60, 53), 13)) using (var brush = new SolidBrush(ink)) graphics.FillPath(brush, cap);
        using (var face = Shapes.Rounded(new RectangleF(3.75f, 7.75f, 56.5f, 46), 11)) using (var brush = new SolidBrush(Color.FromArgb(228, 232, 255))) graphics.FillPath(brush, face);
        void Stroke(float width, float x1, float y1, float x2, float y2)
        {
            using var pen = new Pen(ink, width) { StartCap = LineCap.Round, EndCap = LineCap.Round };
            graphics.DrawLine(pen, x1, y1, x2, y2);
        }
        Stroke(4, 46, 15, 46, 44); Stroke(3, 17, 21, 46, 21); Stroke(4, 12, 38, 46, 38);
        Stroke(3.5f, 36, 21, 36, 38); Stroke(3.5f, 27, 21, 27, 38); Stroke(4, 18, 17, 16, 42);
        return bitmap;
    }
}

/// <summary>A rounded surface holding one setting.</summary>
internal sealed class Card : Panel
{
    public Color Fill = Color.White, Edge = Color.FromArgb(190, 196, 228);
    public Label? Description;
    public Card()
    {
        SetStyle(ControlStyles.OptimizedDoubleBuffer | ControlStyles.AllPaintingInWmPaint | ControlStyles.UserPaint | ControlStyles.ResizeRedraw, true);
        Dock = DockStyle.Fill;
        Padding = new Padding(14, 10, 14, 10); Margin = new Padding(0, 0, 0, 6); BackColor = Theme.MarkBack;
    }
    /// <summary>The card takes its height from its content, which wraps to the card's width.</summary>
    public void Hold(Control content)
    {
        Controls.Add(content);
        void Fit() { int height = content.Height + Padding.Vertical; if (Height != height) Height = height; }
        content.SizeChanged += (_, _) => Fit(); Fit();
    }
    protected override void OnPaint(PaintEventArgs e)
    {
        var graphics = e.Graphics;
        graphics.SmoothingMode = SmoothingMode.AntiAlias;
        graphics.Clear(BackColor);
        using var path = Shapes.Rounded(new RectangleF(0.5f, 0.5f, Width - 1, Height - 1), LogicalToDeviceUnits(8));
        using var brush = new SolidBrush(Fill); using var pen = new Pen(Edge);
        graphics.FillPath(brush, path); graphics.DrawPath(pen, path);
    }
}

/// <summary>A button with rounded corners, an icon before its label and visible hover, press and focus states.</summary>
internal class RoundButton : Button
{
    bool hover, pressed;
    /// <summary>Navigation entries sit flat on the background until selected.</summary>
    public bool Flat, AlignLeft;
    public Color? Indicator;
    public RoundButton()
    {
        SetStyle(ControlStyles.OptimizedDoubleBuffer | ControlStyles.AllPaintingInWmPaint | ControlStyles.UserPaint | ControlStyles.ResizeRedraw, true);
        FlatStyle = FlatStyle.Flat; UseVisualStyleBackColor = false;
    }
    protected override void OnMouseEnter(EventArgs e) { hover = true; Invalidate(); base.OnMouseEnter(e); }
    protected override void OnMouseLeave(EventArgs e) { hover = pressed = false; Invalidate(); base.OnMouseLeave(e); }
    protected override void OnMouseDown(MouseEventArgs e) { pressed = true; Invalidate(); base.OnMouseDown(e); }
    protected override void OnMouseUp(MouseEventArgs e) { pressed = false; Invalidate(); base.OnMouseUp(e); }
    protected override void OnEnabledChanged(EventArgs e) { Invalidate(); base.OnEnabledChanged(e); }
    protected override void OnPaint(PaintEventArgs e)
    {
        var graphics = e.Graphics;
        var behind = Shapes.Behind(this);
        graphics.SmoothingMode = SmoothingMode.AntiAlias;
        graphics.Clear(behind);
        var fill = Flat && Indicator == null ? behind : BackColor;
        if (!Enabled) fill = Shapes.Blend(fill, behind, 0.5);
        else if (pressed) fill = Shapes.Blend(fill, ForeColor, 0.16);
        else if (hover) fill = Shapes.Blend(fill, ForeColor, 0.08);
        var text = Enabled ? ForeColor : Shapes.Blend(ForeColor, fill, 0.55);
        using (var path = Shapes.Rounded(new RectangleF(0.5f, 0.5f, Width - 1, Height - 1), LogicalToDeviceUnits(6)))
        {
            using var brush = new SolidBrush(fill);
            graphics.FillPath(brush, path);
            if (!Flat) { using var pen = new Pen(Enabled ? FlatAppearance.BorderColor : Shapes.Blend(FlatAppearance.BorderColor, behind, 0.5)); graphics.DrawPath(pen, path); }
        }
        if (Indicator is Color mark)
        {
            int bar = LogicalToDeviceUnits(3), inset = Height / 4;
            using var pill = Shapes.Rounded(new RectangleF(0, inset, bar, Height - inset * 2), bar / 2f);
            using var brush = new SolidBrush(mark);
            graphics.FillPath(brush, pill);
        }
        if (Focused && ShowFocusCues)
        {
            using var ring = Shapes.Rounded(new RectangleF(2.5f, 2.5f, Width - 5, Height - 5), LogicalToDeviceUnits(4));
            using var pen = new Pen(text) { DashStyle = DashStyle.Dot };
            graphics.DrawPath(pen, ring);
        }
        const TextFormatFlags plain = TextFormatFlags.NoPadding | TextFormatFlags.SingleLine | TextFormatFlags.NoPrefix;
        var size = TextRenderer.MeasureText(graphics, Text, Font, Size.Empty, plain);
        int icon = Image?.Width ?? 0, gap = icon > 0 && Text.Length > 0 ? LogicalToDeviceUnits(8) : 0;
        int left = AlignLeft ? LogicalToDeviceUnits(14) : (Width - icon - gap - size.Width) / 2;
        if (Image != null) graphics.DrawImage(Image, left, (Height - Image.Height) / 2);
        TextRenderer.DrawText(graphics, Text, Font, new Point(left + icon + gap, (Height - size.Height) / 2), text, plain);
    }
}

/// <summary>An on/off switch.</summary>
internal sealed class Toggle : CheckBox
{
    public Theme? Colors;
    public Toggle()
    {
        SetStyle(ControlStyles.OptimizedDoubleBuffer | ControlStyles.AllPaintingInWmPaint | ControlStyles.UserPaint | ControlStyles.ResizeRedraw, true);
        AutoSize = false; Size = new Size(46, 24); Text = ""; Cursor = Cursors.Hand; Anchor = AnchorStyles.Right; Margin = Padding.Empty;
    }
    protected override void OnCheckedChanged(EventArgs e) { Invalidate(); base.OnCheckedChanged(e); }
    protected override void OnEnabledChanged(EventArgs e) { Invalidate(); base.OnEnabledChanged(e); }
    protected override void OnPaint(PaintEventArgs e)
    {
        var graphics = e.Graphics;
        var behind = Shapes.Behind(this);
        graphics.SmoothingMode = SmoothingMode.AntiAlias;
        graphics.Clear(behind);
        var colors = Colors ?? Theme.Resolve(ThemeMode.Light, false);
        var track = Checked ? colors.Accent : colors.Input;
        var knob = Checked ? colors.OnAccent : colors.Muted;
        var edge = Checked ? colors.Accent : colors.Muted;
        if (!Enabled) { track = Shapes.Blend(track, behind, 0.6); knob = Shapes.Blend(knob, behind, 0.5); edge = Shapes.Blend(edge, behind, 0.6); }
        float height = Height - 3, inset = height * 0.22f, dot = height - inset * 2;
        var bounds = new RectangleF(1, 1.5f, Width - 2, height);
        using (var path = Shapes.Rounded(bounds, height / 2)) { using var brush = new SolidBrush(track); using var pen = new Pen(edge); graphics.FillPath(brush, path); graphics.DrawPath(pen, path); }
        using (var brush = new SolidBrush(knob))
            graphics.FillEllipse(brush, Checked ? bounds.Right - inset - dot : bounds.Left + inset, bounds.Top + inset, dot, dot);
        if (Focused && ShowFocusCues) { using var pen = new Pen(colors.Text) { DashStyle = DashStyle.Dot }; graphics.DrawRectangle(pen, 0, 0, Width - 1, Height - 1); }
    }
}
