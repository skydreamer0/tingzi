using System.Drawing;
using System.Drawing.Drawing2D;
using System.Drawing.Imaging;
using System.Drawing.Text;
using System.Runtime.InteropServices;
using VoiceInput.Protocol;

namespace VoiceInput.App.Services;

public sealed class LayeredCapsuleWindow : IDisposable, IMessageFilter
{
    private const int DefaultCapsuleWidth = 320;
    private const int BaseHeight = 72;
    private const int CapsuleX = 20;
    private const int CapsuleY = 14;
    private const int CapsuleHeight = 44;
    private const int DragThreshold = 4;
    private const int WmLButtonDown = 0x0201;
    private const int WmLButtonUp = 0x0202;
    private const int WmRButtonUp = 0x0205;
    private const int WmMouseMove = 0x0200;
    private const int WmDpiChanged = 0x02E0;
    private const int WmDestroy = 0x0002;
    private const int IdcArrow = 32512;
    private const int SwShowNoActivate = 4;
    private const int AcSrcOver = 0x00;
    private const int AcSrcAlpha = 0x01;
    private const int UlwAlpha = 0x00000002;
    private const uint SwpNoZOrder = 0x0004;
    private const uint SwpNoActivate = 0x0010;
    private const uint MonitorDefaultToNearest = 0x00000002;
    private const uint WsPopup = 0x80000000;
    private const uint WsExTopmost = 0x00000008;
    private const uint WsExToolWindow = 0x00000080;
    private const uint WsExNoActivate = 0x08000000;
    private const uint WsExLayered = 0x00080000;

    private readonly Action mainAction;
    private readonly Action settingsAction;
    private readonly Action closeAction;
    private readonly Action<string> copyAction;
    private readonly Action editAction;
    private int textSize = 20;
    private readonly WndProcDelegate wndProc;
    private nint hwnd;
    private CapsuleSnapshot snapshot = new(CapsuleState.Idle, "點一下開始語音輸入");
    private string timerText = "00:00";
    private float audioLevel;
    private string microphone = "";
    private string recognizer = "本機 Whisper";
    private float loadingAngle;
    private bool darkMode;
    private bool pointerDown;
    private bool dragged;
    private bool previewExpanded;
    private bool previewRequested;
    private bool copyConfirmed;
    private bool copyFailed;
    private DockSide dockSide;
    private bool edgeMode;
    private int previewPanelHeight;
    private float dpiScale = 1f;
    private int capsuleWidth = DefaultCapsuleWidth;
    private int logicalWidth = DefaultCapsuleWidth + CapsuleX * 2;
    private int windowWidth = DefaultCapsuleWidth + CapsuleX * 2;
    private int logicalHeight = BaseHeight;
    private int windowHeight = BaseHeight;
    private PointNative dragStartCursor;
    private PointNative dragStartWindow;
    private sealed record SettingsSession(Form Form, RectNative Bounds, nint ExtendedStyle, nint PreviousForeground, uint PreviousProcess, DockSide ReturnDock)
    {
        public bool RestoreForeground { get; set; }
    }
    private SettingsSession? settings;
    private ContextMenuStrip? contextMenu;
    public bool SettingsOpen => settings != null;
    private bool Busy => snapshot.State is CapsuleState.Recording or CapsuleState.Transcribing;

    public void ActivateSettings()
    {
        if (settings == null) return;
        ShowWindow(hwnd, 5);
        SetForegroundWindow(hwnd);
        settings.Form.Select();
    }

    public void ShowSettings(Form form)
    {
        if (settings != null) { ActivateSettings(); return; }
        if (Busy || hwnd == 0 || form.IsDisposed) return;
        var returnDock = dockSide;
        if (dockSide != DockSide.None) ExpandFromDock();
        GetWindowRect(hwnd, out var bounds);
        var style = GetWindowLongPtr(hwnd, -20);
        var foreground = GetForegroundWindow();
        GetWindowThreadProcessId(foreground, out var foregroundProcess);
        settings = new SettingsSession(form, bounds, style, foreground, foregroundProcess, returnDock);
        form.TopLevel = false;
        form.FormBorderStyle = FormBorderStyle.None;
        form.MinimumSize = Size.Empty;
        form.FormClosed += SettingsClosed;
        form.FormClosing += SettingsClosing;
        form.Disposed += SettingsDisposed;
        try
        {
            SetWindowLongPtr(hwnd, -20, (nint)((long)style & ~((long)WsExLayered | WsExNoActivate)));
            SetParent(form.Handle, hwnd);
            LayoutSettings();
            Application.AddMessageFilter(this);
            form.Show();
            ActivateSettings();
        }
        catch
        {
            RestoreCapsule();
            throw;
        }
    }

    private void LayoutSettings()
    {
        if (settings == null) return;
        var work = GetMonitorWorkArea();
        var width = Math.Min(Scale(780), work.Right - work.Left);
        var height = Math.Min(Scale(640), work.Bottom - work.Top);
        var bounds = settings.Bounds;
        SetWindowPos(hwnd, new nint(-1), Math.Clamp(bounds.Left, work.Left, work.Right - width),
            Math.Clamp(bounds.Top, work.Top, work.Bottom - height), width, height, 0x0020);
        settings.Form.Bounds = new Rectangle(0, 0, width, height);
    }

    private void SettingsClosed(object? sender, FormClosedEventArgs e) => RestoreCapsule();
    private void SettingsDisposed(object? sender, EventArgs e) => RestoreCapsule();
    private void SettingsClosing(object? sender, FormClosingEventArgs e)
    {
        if (settings != null)
            settings.RestoreForeground = !e.Cancel && e.CloseReason == CloseReason.UserClosing && GetForegroundWindow() == hwnd;
    }

    private void RestoreCapsule()
    {
        var session = settings;
        if (session == null) return;
        settings = null;
        Application.RemoveMessageFilter(this);
        session.Form.FormClosed -= SettingsClosed;
        session.Form.FormClosing -= SettingsClosing;
        session.Form.Disposed -= SettingsDisposed;
        if (!session.Form.IsDisposed) session.Form.Hide();
        if (hwnd == 0) return;
        SetWindowLongPtr(hwnd, -20, session.ExtendedStyle);
        UpdateWindowHeight();
        var work = GetMonitorWorkArea();
        SetWindowPos(hwnd, new nint(-1), Math.Clamp(session.Bounds.Left, work.Left, Math.Max(work.Left, work.Right-windowWidth)),
            Math.Clamp(session.Bounds.Top, work.Top, Math.Max(work.Top, work.Bottom-windowHeight)),
            windowWidth, windowHeight, SwpNoActivate | 0x0020);
        if (session.ReturnDock != DockSide.None) DockToEdge(session.ReturnDock,GetMonitorWorkArea());
        Show();
        if (session.RestoreForeground && GetForegroundWindow() == hwnd && session.PreviousForeground != hwnd &&
            IsWindow(session.PreviousForeground) && IsWindowVisible(session.PreviousForeground) && !IsIconic(session.PreviousForeground))
        {
            GetWindowThreadProcessId(session.PreviousForeground, out var process);
            if (process == session.PreviousProcess) SetForegroundWindow(session.PreviousForeground);
        }
    }

    public bool PreFilterMessage(ref Message message)
    {
        var form = settings?.Form;
        if (form == null || message.Msg < 0x0100 || message.Msg > 0x0109 ||
            (message.HWnd != form.Handle && !IsChild(form.Handle, message.HWnd))) return false;
        if (message.Msg == 0x0100 && message.WParam == (nint)Keys.Escape)
        {
            if (Control.FromChildHandle(message.HWnd) is ComboBox { DroppedDown: true } combo)
            {
                combo.DroppedDown = false;
                return true;
            }
            if (form.CancelButton != null) form.CancelButton.PerformClick();
            else form.Close();
            return true;
        }
        var control = Control.FromChildHandle(message.HWnd);
        return control?.PreProcessMessage(ref message) == true ||
            (control != form && form.PreProcessMessage(ref message));
    }

    public LayeredCapsuleWindow(Action mainAction, Action settingsAction, Action closeAction, Action<string> copyAction, Action editAction)
    {
        this.mainAction = mainAction;
        this.settingsAction = settingsAction;
        this.closeAction = closeAction;
        this.copyAction = copyAction;
        this.editAction = editAction;
        wndProc = WndProc;
        Create();
    }

    public void Show()
    {
        if (SettingsOpen) { ActivateSettings(); return; }
        ShowWindow(hwnd, SwShowNoActivate);
        SetWindowPos(hwnd, new nint(-1), 0, 0, 0, 0, SwpNoActivate | 0x0001 | 0x0002);
        Render();
    }

    public void Recover()
    {
        if (SettingsOpen) { LayoutSettings(); ActivateSettings(); return; }
        edgeMode = false;
        dockSide = DockSide.None;
        logicalWidth = DefaultCapsuleWidth + CapsuleX * 2;
        previewExpanded = previewRequested && snapshot.State == CapsuleState.Ready && !string.IsNullOrWhiteSpace(snapshot.PreviewText);
        UpdateWindowHeight();
        var work = Screen.FromPoint(Cursor.Position).WorkingArea;
        SetWindowPos(hwnd, new nint(-1), Math.Max(work.Left,work.Right-windowWidth-Scale(24)),
            Math.Max(work.Top,work.Bottom-windowHeight-Scale(48)), windowWidth,windowHeight,SwpNoActivate);
        Show();
    }

    public void DockRight(bool useCursorMonitor = false)
    {
        if (SettingsOpen) return;
        edgeMode = true;
        if (useCursorMonitor)
        {
            var area = Screen.FromPoint(Cursor.Position).WorkingArea;
            SetWindowPos(hwnd, 0, area.Right-Scale(72), Math.Clamp(Cursor.Position.Y-Scale(36),area.Top,Math.Max(area.Top,area.Bottom-Scale(72))),
                Scale(72),Scale(72),SwpNoZOrder|SwpNoActivate);
            dpiScale = Math.Max(1f, GetDpiForWindow(hwnd) / 96f);
        }
        DockToEdge(DockSide.Right, GetMonitorWorkArea());
        Show();
    }

    private void ShowContextMenu()
    {
        contextMenu?.Dispose();
        contextMenu = new ContextMenuStrip();
        var settingsItem = contextMenu.Items.Add(Busy ? "設定・停止錄音或辨識後可用" : "語音助理設定…", null, (_,_) => settingsAction());
        settingsItem.Enabled = !Busy;
        contextMenu.Items.Add("顯示完整浮條", null, (_,_) => { edgeMode=false; if(dockSide != DockSide.None) ExpandFromDock(); else Recover(); });
        contextMenu.Items.Add("靠右收合為麥克風", null, (_,_) => DockRight());
        contextMenu.Show(Cursor.Position);
    }

    public void SetTextSize(int pixels)
    {
        textSize = Math.Clamp(pixels,18,24);
        UpdateWindowHeight();
        Render();
    }

    public void Hide() => DockRight();

    public void Update(CapsuleSnapshot next)
    {
        var previousState = snapshot.State;
        if (next.State != CapsuleState.Ready) previewRequested = false;
        else if (previousState != CapsuleState.Ready || snapshot.PreviewText != next.PreviewText || snapshot.AutoExpandPreview != next.AutoExpandPreview)
            previewRequested = next.AutoExpandPreview;
        snapshot = next;
        if (edgeMode && dockSide == DockSide.None && !SettingsOpen &&
            (next.State == CapsuleState.Recording || next.State == CapsuleState.Idle || next.State == CapsuleState.Ready && !next.AutoExpandPreview))
            DockToEdge(DockSide.Right,GetMonitorWorkArea());
        if (dockSide != DockSide.None && (next.State is CapsuleState.RetryLocal or CapsuleState.Error ||
            next.State == CapsuleState.Ready && next.AutoExpandPreview)) ExpandFromDock();
        SetWindowText(hwnd, "聽字 語音浮窗 · " + next.Message +
            (next.State == CapsuleState.RetryLocal ? " · 點此改用本機" : "") +
            (next.State is CapsuleState.Idle or CapsuleState.Recording ? " · " + microphone : ""));
        copyConfirmed = false;
        copyFailed = false;
        if (next.State != CapsuleState.Recording)
        {
            audioLevel = 0;
        }
        var shouldExpandPreview = dockSide == DockSide.None
            && previewRequested
            && next.State == CapsuleState.Ready
            && !string.IsNullOrWhiteSpace(next.PreviewText);
        if (previewExpanded != shouldExpandPreview || shouldExpandPreview || previousState != next.State)
        {
            previewExpanded = shouldExpandPreview;
            UpdateWindowHeight();
        }
        Render();
    }

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern bool SetWindowText(nint handle, string title);

    public void UpdateRecording(float level, string elapsed)
    {
        timerText = elapsed;
        audioLevel = Math.Clamp(level, 0f, 1f);
        if (snapshot.State == CapsuleState.Recording)
        {
            Render();
        }
    }

    public void SetMicrophone(string label)
    {
        if (microphone == label) return;
        microphone = label;
        if (snapshot.State == CapsuleState.Idle) Update(snapshot);
    }

    private string MicrophoneShortName
    {
        get
        {
            var start = microphone.IndexOf('(');
            var end = microphone.LastIndexOf(')');
            if (start >= 0 && end > start &&
                (microphone[..start].Contains("麥克風") || microphone[..start].Contains("Microphone", StringComparison.OrdinalIgnoreCase)))
                return microphone[(start+1)..end];
            return microphone;
        }
    }

    public void SetRecognizer(string label)
    {
        if (recognizer == label) return;
        recognizer = label;
        Render();
    }

    public void AdvanceLoadingAnimation()
    {
        if (snapshot.State != CapsuleState.Transcribing)
        {
            return;
        }

        loadingAngle = (loadingAngle + 13f) % 360f;
        Render();
    }

    public void SetDarkMode(bool enabled)
    {
        darkMode = enabled;
        Render();
    }

    public void ShowCopyConfirmation()
    {
        copyConfirmed = true;
        copyFailed = false;
        Render();
    }

    public void ShowCopyFailure()
    {
        copyConfirmed = false;
        copyFailed = true;
        Render();
    }

    public void Dispose()
    {
        contextMenu?.Dispose();
        settings?.Form.Close();
        RestoreCapsule();
        if (hwnd != 0)
        {
            DestroyWindow(hwnd);
            hwnd = 0;
        }
        GC.SuppressFinalize(this);
    }

    private void Create()
    {
        var className = $"VoiceInputCapsule{Guid.NewGuid():N}";
        var wndClass = new WndClass
        {
            ClassName = className,
            WndProc = Marshal.GetFunctionPointerForDelegate(wndProc),
            Instance = GetModuleHandle(null),
            Cursor = LoadCursor(0, new nint(IdcArrow)),
        };
        if (RegisterClass(ref wndClass) == 0)
        {
            throw new InvalidOperationException("Failed to register capsule window class.");
        }

        dpiScale = Math.Max(1f, GetDpiForSystem() / 96f);
        UpdatePhysicalSize();
        var screenWidth = GetSystemMetrics(0);
        var screenHeight = GetSystemMetrics(1);
        var x = Math.Max(Scale(24), screenWidth - windowWidth - Scale(32));
        var y = Math.Max(Scale(24), screenHeight - windowHeight - Scale(96));

        hwnd = CreateWindowEx(
            WsExTopmost | WsExToolWindow | WsExNoActivate | WsExLayered,
            className,
            "聽字 語音浮窗",
            WsPopup,
            x,
            y,
            windowWidth,
            windowHeight,
            0,
            0,
            wndClass.Instance,
            0);

        if (hwnd == 0)
        {
            throw new InvalidOperationException("Failed to create capsule window.");
        }
    }

    private nint WndProc(nint hWnd, uint message, nuint wParam, nint lParam)
    {
        if (SettingsOpen)
        {
            if (message == WmDpiChanged)
            {
                dpiScale = Math.Max(1f, unchecked((ushort)((ulong)wParam & 0xffff)) / 96f);
                LayoutSettings();
                return 0;
            }
            if (message is 0x007E or 0x001A) { LayoutSettings(); return 0; }
            if (message == 0x0010) { settings?.Form.Close(); return 0; }
            return DefWindowProc(hWnd, message, wParam, lParam);
        }
        switch ((int)message)
        {
            case 0x0010:
                DockRight();
                return 0;
            case 0x007E:
            case 0x001A:
                ConstrainToWorkArea();
                return 0;
            case WmRButtonUp:
                ShowContextMenu();
                return 0;
            case WmLButtonDown:
                pointerDown = true;
                dragged = false;
                GetCursorPos(out dragStartCursor);
                dragStartWindow = GetWindowPosition();
                SetCapture(hwnd);
                return 0;
            case WmMouseMove:
                if (pointerDown)
                {
                    DragWindowIfNeeded();
                }
                return 0;
            case WmDpiChanged:
                dpiScale = Math.Max(1f, unchecked((ushort)((ulong)wParam & 0xffff)) / 96f);
                UpdatePhysicalSize();
                var suggested = Marshal.PtrToStructure<RectNative>(lParam);
                SetWindowPos(
                    hwnd,
                    0,
                    suggested.Left,
                    suggested.Top,
                    windowWidth,
                    windowHeight,
                    SwpNoZOrder | SwpNoActivate);
                ConstrainToWorkArea();
                return 0;
            case WmLButtonUp:
                ReleaseCapture();
                pointerDown = false;
                if (dragged)
                {
                    if (dockSide == DockSide.None)
                    {
                        TryDockAtEdge();
                    }
                    else
                    {
                        SnapDockedToEdge();
                    }
                    return 0;
                }

                if (dockSide != DockSide.None)
                {
                    var dockX = unchecked((short)((long)lParam & 0xffff)) / dpiScale;
                    var dockY = unchecked((short)(((long)lParam >> 16) & 0xffff)) / dpiScale;
                    if (dockX >= 14 && dockX <= 58 && dockY >= 14 && dockY <= 58 && snapshot.State != CapsuleState.Transcribing)
                        mainAction();
                    return 0;
                }

                var x = unchecked((short)((long)lParam & 0xffff)) / dpiScale - CapsuleX;
                var y = unchecked((short)(((long)lParam >> 16) & 0xffff)) / dpiScale - CapsuleY;
                if (x < 0 || x > capsuleWidth || y < 0)
                {
                    return 0;
                }
                if (y > CapsuleHeight)
                {
                    var discardTop = 52 + CalculateReasonPanelHeight() - 36;
                    if (snapshot.State == CapsuleState.RetryLocal && y >= discardTop && y <= discardTop+30 && x >= capsuleWidth-76 && x <= capsuleWidth-12)
                        closeAction();
                    if (previewExpanded && snapshot.State == CapsuleState.Ready)
                    {
                        if (x >= capsuleWidth - 46 && !string.IsNullOrWhiteSpace(snapshot.PreviewText))
                        {
                            copyAction(snapshot.PreviewText!);
                        }
                        else
                        {
                            editAction();
                        }
                    }
                    return 0;
                }
                if (x >= capsuleWidth - 34)
                {
                    DockRight();
                }
                else if (x >= capsuleWidth - 70)
                {
                    if (!Busy) settingsAction();
                }
                else if (snapshot.State == CapsuleState.Ready && x >= capsuleWidth - 104)
                {
                    TogglePreview();
                }
                else
                {
                    mainAction();
                }
                return 0;
            case WmDestroy:
                return 0;
        }

        return DefWindowProc(hWnd, message, wParam, lParam);
    }

    private void DragWindowIfNeeded()
    {
        if (!GetCursorPos(out var cursor))
        {
            return;
        }

        var dx = cursor.X - dragStartCursor.X;
        var dy = cursor.Y - dragStartCursor.Y;
        if (!dragged && Math.Abs(dx) < DragThreshold && Math.Abs(dy) < DragThreshold)
        {
            return;
        }

        dragged = true;
        var nextX = dockSide == DockSide.None ? dragStartWindow.X + dx : dragStartWindow.X;
        SetWindowPos(hwnd, 0, nextX, dragStartWindow.Y + dy, windowWidth, windowHeight, SwpNoZOrder | SwpNoActivate);
        Render();
    }

    private void Render()
    {
        if (SettingsOpen || hwnd == 0) return;
        using var bitmap = new Bitmap(windowWidth, windowHeight, PixelFormat.Format32bppPArgb);
        using (var graphics = Graphics.FromImage(bitmap))
        {
            graphics.CompositingMode = CompositingMode.SourceOver;
            graphics.CompositingQuality = CompositingQuality.HighQuality;
            graphics.InterpolationMode = InterpolationMode.HighQualityBicubic;
            graphics.PixelOffsetMode = PixelOffsetMode.HighQuality;
            graphics.SmoothingMode = SmoothingMode.AntiAlias;
            graphics.TextRenderingHint = TextRenderingHint.AntiAliasGridFit;
            graphics.Clear(Color.Transparent);
            graphics.ScaleTransform(dpiScale, dpiScale);
            if (dockSide == DockSide.None)
            {
                DrawCapsule(graphics);
            }
            else
            {
                DrawDockedCapsule(graphics);
            }
        }

        var screenDc = GetDC(0);
        var memoryDc = CreateCompatibleDC(screenDc);
        var hBitmap = bitmap.GetHbitmap(Color.FromArgb(0));
        var oldBitmap = SelectObject(memoryDc, hBitmap);
        try
        {
            var position = GetWindowPosition();
            var size = new SizeNative(windowWidth, windowHeight);
            var source = new PointNative(0, 0);
            var blend = new BlendFunction(AcSrcOver, 0, 255, AcSrcAlpha);
            UpdateLayeredWindow(hwnd, screenDc, ref position, ref size, memoryDc, ref source, 0, ref blend, UlwAlpha);
        }
        finally
        {
            SelectObject(memoryDc, oldBitmap);
            DeleteObject(hBitmap);
            DeleteDC(memoryDc);
            ReleaseDC(0, screenDc);
        }
    }

    private void DrawCapsule(Graphics graphics)
    {
        var rect = new RectangleF(CapsuleX + 0.5f, CapsuleY + 0.5f, capsuleWidth - 1, CapsuleHeight - 1);
        using var path = RoundedRect(rect, 22);

        var shadows = new[]
        {
            (Expand: 5f, OffsetY: 5f, Alpha: 2),
            (Expand: 2f, OffsetY: 3f, Alpha: 4),
            (Expand: 0.5f, OffsetY: 1.5f, Alpha: 5),
        };
        foreach (var layer in shadows)
        {
            using var shadowPath = RoundedRect(
                new RectangleF(
                    CapsuleX - layer.Expand,
                    CapsuleY + layer.OffsetY - layer.Expand,
                    capsuleWidth + layer.Expand * 2,
                    CapsuleHeight + layer.Expand * 2),
                22 + layer.Expand);
            using var shadow = new SolidBrush(Color.FromArgb(layer.Alpha, 35, 49, 67));
            graphics.FillPath(shadow, shadowPath);
        }

        if (snapshot.State is CapsuleState.Recording or CapsuleState.Transcribing or CapsuleState.Ready)
        {
            using var outerGlowPath = RoundedRect(new RectangleF(CapsuleX - 2, CapsuleY - 1, capsuleWidth + 4, CapsuleHeight + 4), 24);
            using var outerGlow = new SolidBrush(StateGlowColor());
            graphics.FillPath(outerGlow, outerGlowPath);
        }

        using (var glassFill = new LinearGradientBrush(rect, Color.Black, Color.Black, LinearGradientMode.Vertical))
        {
            var glassColors = darkMode
                ? new[]
                {
                    Color.FromArgb(246, 43, 54, 68),
                    Color.FromArgb(242, 32, 43, 56),
                    Color.FromArgb(240, 24, 34, 45),
                }
                : new[]
                {
                    Color.FromArgb(246, 255, 255, 255),
                    Color.FromArgb(238, 252, 253, 255),
                    Color.FromArgb(232, 246, 249, 253),
                };
            glassFill.InterpolationColors = new ColorBlend
            {
                Colors = glassColors,
                Positions = [0f, 0.5f, 1f],
            };
            graphics.FillPath(glassFill, path);
        }

        using (var edgeBrush = new LinearGradientBrush(rect, Color.Black, Color.Black, 12f))
        {
            var edgeColors = snapshot.State switch
            {
                CapsuleState.Recording => new[] { Color.FromArgb(126, 126, 187, 255), Color.FromArgb(112, 67, 139, 255) },
                CapsuleState.Transcribing => new[] { Color.FromArgb(72, 184, 211, 247), Color.FromArgb(62, 113, 166, 240) },
                CapsuleState.Ready => new[] { Color.FromArgb(98, 139, 232, 192), Color.FromArgb(72, 69, 207, 153) },
                _ when darkMode => new[] { Color.FromArgb(66, 104, 121, 143), Color.FromArgb(52, 54, 69, 85) },
                _ => new[] { Color.FromArgb(64, 229, 233, 239), Color.FromArgb(58, 197, 206, 218) },
            };
            edgeBrush.LinearColors = edgeColors;
            using var border = new Pen(edgeBrush, snapshot.State == CapsuleState.Recording ? 0.9f : 0.7f);
            graphics.DrawPath(border, path);
        }

        using (var innerPath = RoundedRect(new RectangleF(rect.X + 1, rect.Y + 1, rect.Width - 2, rect.Height - 2), 21))
        using (var innerBorder = new Pen(Color.FromArgb(darkMode ? 36 : 86, 255, 255, 255), 0.5f))
        {
            graphics.DrawPath(innerBorder, innerPath);
        }

        DrawMicButton(graphics);
        DrawTextOrWave(graphics);
        if (snapshot.State == CapsuleState.Ready)
        {
            DrawChevron(graphics, CapsuleX + capsuleWidth - 86, CapsuleY + 22, previewExpanded, darkMode);
        }
        DrawSettingsIcon(graphics, CapsuleX + capsuleWidth - 50, CapsuleY + 22, darkMode, Busy);
        DrawCloseIcon(graphics, CapsuleX + capsuleWidth - 18, CapsuleY + 22, darkMode);
        if (previewExpanded)
        {
            DrawPreviewPanel(graphics);
        }
        if (snapshot.State is CapsuleState.RetryLocal or CapsuleState.Error)
        {
            var panelHeight = CalculateReasonPanelHeight();
            var panel = new RectangleF(CapsuleX, CapsuleY+52, capsuleWidth, panelHeight);
            using var background = new SolidBrush(darkMode ? Color.FromArgb(31,43,57) : Color.White);
            using var retryPath = RoundedRect(panel, 12);
            graphics.FillPath(background, retryPath);
            using var brush = new SolidBrush(darkMode ? Color.WhiteSmoke : Color.FromArgb(58,71,90));
            using var font = new Font("Microsoft JhengHei UI", 13f, GraphicsUnit.Pixel);
            graphics.DrawString(snapshot.Message, font, brush, new RectangleF(panel.X+12,panel.Y+8,panel.Width-24,panelHeight-(snapshot.State == CapsuleState.RetryLocal?48:16)));
            if (snapshot.State == CapsuleState.RetryLocal)
            {
                var discard = new RectangleF(CapsuleX+capsuleWidth-76,panel.Bottom-36,64,30);
                using var border = new Pen(darkMode ? Color.Gray : Color.Silver);
                graphics.DrawRectangle(border, discard.X, discard.Y, discard.Width, discard.Height);
                using var center = new StringFormat { Alignment=StringAlignment.Center, LineAlignment=StringAlignment.Center };
                graphics.DrawString("捨棄",font,brush,discard,center);
            }
        }
    }

    private void DrawDockedCapsule(Graphics graphics)
    {
        const float x = 14f;
        const float y = 14f;
        const float width = 44f;
        const float height = 44f;
        var rect = new RectangleF(x + 0.5f, y + 0.5f, width - 1f, height - 1f);

        for (var i = 4; i >= 1; i--)
        {
            using var shadowPath = RoundedRect(new RectangleF(x - i, y + 3 - i, width + i * 2, height + i * 2), 22 + i);
            using var shadow = new SolidBrush(Color.FromArgb(2 + i, 35, 49, 67));
            graphics.FillPath(shadow, shadowPath);
        }

        using var path = RoundedRect(rect, 22);
        using (var fill = new LinearGradientBrush(
            rect,
            darkMode ? Color.FromArgb(244, 43, 54, 68) : Color.FromArgb(248, 255, 255, 255),
            darkMode ? Color.FromArgb(238, 24, 34, 45) : Color.FromArgb(235, 246, 249, 253),
            LinearGradientMode.Horizontal))
        {
            graphics.FillPath(fill, path);
        }
        using (var border = new Pen(
            darkMode ? Color.FromArgb(70, 112, 135, 162) : Color.FromArgb(76, 210, 219, 231),
            0.7f))
        {
            graphics.DrawPath(border, path);
        }

        var iconColor = snapshot.State == CapsuleState.Recording
            ? Color.FromArgb(255, 50, 126, 244)
            : darkMode ? Color.FromArgb(224, 230, 240, 252) : Color.FromArgb(255, 31, 45, 61);
        using var pen = new Pen(iconColor, 1.35f)
        {
            StartCap = LineCap.Round,
            EndCap = LineCap.Round,
            LineJoin = LineJoin.Round,
        };
        var cx = x + width / 2f;
        var cy = y + height / 2f;
        if (snapshot.State == CapsuleState.Recording)
        {
            using var recordingFill = new SolidBrush(Color.FromArgb(232,67,67));
            graphics.FillRectangle(recordingFill, cx-6,cy-6,12,12);
            return;
        }
        if (snapshot.State == CapsuleState.Transcribing)
        {
            graphics.DrawArc(pen,cx-9,cy-9,18,18,loadingAngle,250);
            return;
        }
        using var micBody = RoundedRect(new RectangleF(cx - 3.5f, cy - 9.5f, 7, 13), 3.5f);
        graphics.DrawPath(pen, micBody);
        graphics.DrawArc(pen, cx - 7.5f, cy - 4.5f, 15, 13, 0, 180);
        graphics.DrawLine(pen, cx, cy + 8, cx, cy + 12.5f);
        graphics.DrawLine(pen, cx - 4.5f, cy + 12.5f, cx + 4.5f, cy + 12.5f);
    }

    private Color StateGlowColor()
    {
        return snapshot.State switch
        {
            CapsuleState.Recording => Color.FromArgb(9, 42, 127, 255),
            CapsuleState.Transcribing => Color.FromArgb(6, 65, 139, 238),
            CapsuleState.Ready => Color.FromArgb(8, 43, 205, 145),
            CapsuleState.Error => Color.FromArgb(7, 255, 166, 98),
            _ => Color.Transparent,
        };
    }

    private void DrawMicButton(Graphics graphics)
    {
        var rect = new RectangleF(CapsuleX + 7, CapsuleY + 5, 34, 34);
        using var path = RoundedRect(rect, 17);
        var fillTop = snapshot.State switch
        {
            CapsuleState.Recording => Color.FromArgb(250, 125, 184, 255),
            CapsuleState.Ready => Color.FromArgb(248, 112, 229, 185),
            CapsuleState.Error => Color.FromArgb(246, 255, 249, 230),
            _ when darkMode => Color.FromArgb(238, 63, 79, 99),
            _ => Color.FromArgb(245, 255, 255, 255),
        };
        var fillBottom = snapshot.State switch
        {
            CapsuleState.Recording => Color.FromArgb(250, 43, 119, 242),
            CapsuleState.Ready => Color.FromArgb(248, 37, 199, 136),
            CapsuleState.Error => Color.FromArgb(242, 255, 240, 205),
            _ when darkMode => Color.FromArgb(238, 42, 56, 72),
            _ => Color.FromArgb(240, 246, 249, 253),
        };

        if (snapshot.State is CapsuleState.Recording or CapsuleState.Ready)
        {
            var glowColor = snapshot.State == CapsuleState.Recording
                ? Color.FromArgb(16, 53, 133, 255)
                : Color.FromArgb(14, 43, 207, 145);
            using var glowPath = RoundedRect(new RectangleF(rect.X - 3, rect.Y - 2, rect.Width + 6, rect.Height + 7), 20);
            using var glow = new SolidBrush(glowColor);
            graphics.FillPath(glow, glowPath);
        }

        using var fill = new LinearGradientBrush(rect, fillTop, fillBottom, LinearGradientMode.Vertical);
        using var border = new Pen(Color.FromArgb(70, 205, 214, 226), 0.7f);
        graphics.FillPath(fill, path);
        graphics.DrawPath(border, path);

        var iconColor = snapshot.State is CapsuleState.Recording or CapsuleState.Ready
            ? Color.White
            : darkMode
                ? Color.FromArgb(90, 165, 255)
                : Color.FromArgb(31, 45, 61);
        using var pen = new Pen(iconColor, 1.35f)
        {
            StartCap = LineCap.Round,
            EndCap = LineCap.Round,
            LineJoin = LineJoin.Round,
        };

        var cx = CapsuleX + 24f;
        var cy = CapsuleY + 22f;
        if (snapshot.State == CapsuleState.Recording)
        {
            using var stop = new SolidBrush(iconColor);
            graphics.FillRectangle(stop, cx-6, cy-6, 12, 12);
            return;
        }

        if (snapshot.State == CapsuleState.Transcribing)
        {
            using var track = new Pen(Color.FromArgb(76, 177, 197, 225), 1.25f);
            using var active = new Pen(Color.FromArgb(255, 54, 128, 240), 1.7f)
            {
                StartCap = LineCap.Round,
                EndCap = LineCap.Round,
            };
            graphics.DrawEllipse(track, cx - 8.5f, cy - 8.5f, 17, 17);
            graphics.DrawArc(active, cx - 8.5f, cy - 8.5f, 17, 17, -85 + loadingAngle, 245);
            return;
        }

        using var micBody = RoundedRect(new RectangleF(cx - 3.5f, cy - 9.5f, 7, 13), 3.5f);
        graphics.DrawPath(pen, micBody);
        graphics.DrawArc(pen, cx - 7.5f, cy - 4.5f, 15, 13, 0, 180);
        graphics.DrawLine(pen, cx, cy + 8, cx, cy + 12.5f);
        graphics.DrawLine(pen, cx - 4.5f, cy + 12.5f, cx + 4.5f, cy + 12.5f);
    }

    private void DrawTextOrWave(Graphics graphics)
    {
        if (snapshot.State is CapsuleState.Recording or CapsuleState.Idle or CapsuleState.RetryLocal)
        {
            bool recording = snapshot.State == CapsuleState.Recording;
            var color = darkMode ? Color.FromArgb(226, 236, 248) : Color.FromArgb(39, 55, 76);
            using var brush = new SolidBrush(color);
            using var font = new Font("Microsoft JhengHei UI", 15f, GraphicsUnit.Pixel);
            using var small = new Font("Microsoft JhengHei UI", 13f, GraphicsUnit.Pixel);
            using var formatLine = new StringFormat { FormatFlags = StringFormatFlags.NoWrap,
                Trimming = StringTrimming.EllipsisCharacter, LineAlignment = StringAlignment.Center };
            int left = CapsuleX + 55, width = capsuleWidth - 130;
            if (recording) {
                var db = audioLevel > 0 ? 20 * Math.Log10(audioLevel) : double.NegativeInfinity;
                string volume = audioLevel > 0 ? $"{db:F0} dBFS" : "靜音";
                graphics.DrawString($"{timerText}  {volume}", small, brush,
                    new RectangleF(left,CapsuleY+1,width-44,21),formatLine);
                var bar = new RectangleF(left+width-42,CapsuleY+8,38,7);
                using var track = new SolidBrush(Color.FromArgb(65,color));
                using var signal = new SolidBrush(darkMode ? Color.LightSkyBlue : Color.SteelBlue);
                graphics.FillRectangle(track,bar);
                graphics.FillRectangle(signal,bar.X,bar.Y,bar.Width*(float)Math.Clamp((db+60)/60,0,1),bar.Height);
            } else {
                var title = snapshot.State == CapsuleState.RetryLocal ? "點此改用本機辨識" : snapshot.Message;
                graphics.DrawString(title,font,brush,new RectangleF(left,CapsuleY+1,width,21),formatLine);
            }
            graphics.DrawString($"{(snapshot.State == CapsuleState.Idle ? "下次" : "來源")} {recognizer} · {MicrophoneShortName}",
                small,brush,new RectangleF(left,CapsuleY+22,width,20),formatLine);
            return;
        }

        using var textBrush = new SolidBrush(copyFailed
            ? darkMode ? Color.FromArgb(255, 252, 216) : Color.FromArgb(170, 93, 32)
            : snapshot.State == CapsuleState.Ready
            ? darkMode ? Color.FromArgb(236, 243, 252) : Color.FromArgb(34, 48, 66)
            : darkMode ? Color.FromArgb(190, 203, 220) : Color.FromArgb(111, 124, 143));
        using var textFont = new Font("Microsoft JhengHei UI", 16f, GraphicsUnit.Pixel);
        using var format = new StringFormat
        {
            Trimming = StringTrimming.EllipsisCharacter,
            FormatFlags = StringFormatFlags.NoWrap,
            LineAlignment = StringAlignment.Center,
        };
        var textWidth = capsuleWidth - (snapshot.State == CapsuleState.Ready ? 168 : 134);
        var text = copyFailed ? "複製失敗，請重試" : copyConfirmed ? "已複製文字" :
            snapshot.State == CapsuleState.Transcribing ? "辨識中・點此取消" : snapshot.Message;
        graphics.DrawString(text, textFont, textBrush, new RectangleF(CapsuleX + 60, CapsuleY + 1, textWidth, 23), format);
        using var detailFont = new Font("Microsoft JhengHei UI", 11f, GraphicsUnit.Pixel);
        graphics.DrawString($"來源 {recognizer} · {MicrophoneShortName}", detailFont, textBrush,
            new RectangleF(CapsuleX + 60, CapsuleY + 24, textWidth, 16), format);
    }

    private void DrawAmbientLightBands(Graphics graphics, GraphicsPath capsulePath)
    {
        var state = graphics.Save();
        graphics.SetClip(capsulePath, CombineMode.Intersect);

        using var upper = new GraphicsPath();
        upper.AddBezier(
            CapsuleX + 36, CapsuleY + 31,
            CapsuleX + capsuleWidth * 0.35f, CapsuleY + 4,
            CapsuleX + capsuleWidth * 0.66f, CapsuleY + 43,
            CapsuleX + capsuleWidth - 12, CapsuleY + 14);
        using var upperGlow = new Pen(Color.FromArgb(17, 119, 108, 255), 8f) { StartCap = LineCap.Round, EndCap = LineCap.Round };
        using var upperCore = new Pen(Color.FromArgb(30, 188, 165, 255), 1.1f) { StartCap = LineCap.Round, EndCap = LineCap.Round };
        graphics.DrawPath(upperGlow, upper);
        graphics.DrawPath(upperCore, upper);

        using var lower = new GraphicsPath();
        lower.AddBezier(
            CapsuleX + 44, CapsuleY + 40,
            CapsuleX + capsuleWidth * 0.34f, CapsuleY + 16,
            CapsuleX + capsuleWidth * 0.68f, CapsuleY + 51,
            CapsuleX + capsuleWidth - 10, CapsuleY + 25);
        using var lowerGlow = new Pen(Color.FromArgb(13, 74, 189, 255), 9f) { StartCap = LineCap.Round, EndCap = LineCap.Round };
        using var lowerCore = new Pen(Color.FromArgb(24, 147, 224, 255), 0.9f) { StartCap = LineCap.Round, EndCap = LineCap.Round };
        graphics.DrawPath(lowerGlow, lower);
        graphics.DrawPath(lowerCore, lower);

        graphics.Restore(state);
    }

    private int Scale(int logicalPixels) => (int)Math.Round(logicalPixels * dpiScale);

    private void TryDockAtEdge()
    {
        var workArea = GetMonitorWorkArea();
        GetWindowRect(hwnd, out var current);
        var centerX = current.Left + (current.Right - current.Left) / 2;
        var distanceLeft = Math.Abs(centerX - workArea.Left);
        var distanceRight = Math.Abs(workArea.Right - centerX);
        var threshold = Scale(64);
        if (distanceLeft <= threshold || current.Left <= workArea.Left + Scale(16))
        {
            DockToEdge(DockSide.Left, workArea);
        }
        else if (distanceRight <= threshold || current.Right >= workArea.Right - Scale(16))
        {
            DockToEdge(DockSide.Right, workArea);
        }
    }

    private void DockToEdge(DockSide side, RectNative workArea)
    {
        edgeMode = true;
        GetWindowRect(hwnd, out var current);
        var centerY = current.Top + (current.Bottom - current.Top) / 2;
        dockSide = side;
        previewExpanded = false;
        previewPanelHeight = 0;
        logicalWidth = 72;
        logicalHeight = 72;
        UpdatePhysicalSize();
        var x = side == DockSide.Left
            ? workArea.Left - Scale(14)
            : workArea.Right - windowWidth + Scale(14);
        var y = Math.Clamp(centerY - windowHeight / 2, workArea.Top, Math.Max(workArea.Top,workArea.Bottom - windowHeight));
        SetWindowPos(hwnd, new nint(-1), x, y, windowWidth, windowHeight, SwpNoActivate);
        Render();
    }

    private void SnapDockedToEdge()
    {
        var workArea = GetMonitorWorkArea();
        GetWindowRect(hwnd, out var current);
        var x = dockSide == DockSide.Left
            ? workArea.Left - Scale(14)
            : workArea.Right - windowWidth + Scale(14);
        var y = Math.Clamp(current.Top, workArea.Top, Math.Max(workArea.Top,workArea.Bottom - windowHeight));
        SetWindowPos(hwnd, new nint(-1), x, y, windowWidth, windowHeight, SwpNoActivate);
        Render();
    }

    private void ExpandFromDock()
    {
        var side = dockSide;
        var workArea = GetMonitorWorkArea();
        GetWindowRect(hwnd, out var current);
        var centerY = current.Top + (current.Bottom - current.Top) / 2;
        dockSide = DockSide.None;
        logicalWidth = DefaultCapsuleWidth + CapsuleX * 2;
        previewExpanded = previewRequested && snapshot.State == CapsuleState.Ready && !string.IsNullOrWhiteSpace(snapshot.PreviewText);
        previewPanelHeight = previewExpanded ? CalculatePreviewPanelHeight() : 0;
        logicalHeight = previewExpanded
            ? CapsuleY + CapsuleHeight + 8 + previewPanelHeight + 14
            : BaseHeight;
        if (snapshot.State is CapsuleState.RetryLocal or CapsuleState.Error) logicalHeight = CapsuleY + 52 + CalculateReasonPanelHeight() + 14;
        UpdatePhysicalSize();
        var x = side == DockSide.Left
            ? workArea.Left
            : workArea.Right - windowWidth;
        var y = Math.Clamp(centerY - windowHeight / 2, workArea.Top, Math.Max(workArea.Top,workArea.Bottom - windowHeight));
        SetWindowPos(hwnd, new nint(-1), x, y, windowWidth, windowHeight, SwpNoActivate);
        Render();
    }

    private void ConstrainToWorkArea()
    {
        if (SettingsOpen) { LayoutSettings(); return; }
        if (dockSide != DockSide.None) { SnapDockedToEdge(); return; }
        UpdateWindowHeight();
        Render();
    }

    private RectNative GetMonitorWorkArea()
    {
        var monitor = MonitorFromWindow(hwnd, MonitorDefaultToNearest);
        var info = new MonitorInfo { Size = Marshal.SizeOf<MonitorInfo>() };
        return GetMonitorInfo(monitor, ref info)
            ? info.WorkArea
            : new RectNative { Left = 0, Top = 0, Right = GetSystemMetrics(0), Bottom = GetSystemMetrics(1) };
    }

    private void UpdatePhysicalSize()
    {
        windowWidth = Scale(logicalWidth);
        windowHeight = Scale(logicalHeight);
    }

    private void TogglePreview()
    {
        previewExpanded = !previewExpanded;
        previewRequested = previewExpanded;
        UpdateWindowHeight();
        Render();
    }

    private void UpdateWindowHeight()
    {
        if (SettingsOpen) return;
        if (dockSide != DockSide.None)
        {
            return;
        }

        previewPanelHeight = previewExpanded ? CalculatePreviewPanelHeight() : 0;
        logicalHeight = previewExpanded
            ? CapsuleY + CapsuleHeight + 8 + previewPanelHeight + 14
            : BaseHeight;
        if (snapshot.State is CapsuleState.RetryLocal or CapsuleState.Error) logicalHeight = CapsuleY + 52 + CalculateReasonPanelHeight() + 14;
        if (hwnd == 0)
        {
            UpdatePhysicalSize();
            return;
        }
        GetWindowRect(hwnd, out var current);
        UpdatePhysicalSize();
        var work = GetMonitorWorkArea();
        SetWindowPos(
            hwnd,
            0,
            Math.Clamp(current.Left,work.Left,Math.Max(work.Left,work.Right-windowWidth)),
            Math.Clamp(current.Top,work.Top,Math.Max(work.Top,work.Bottom-windowHeight)),
            windowWidth,
            windowHeight,
            SwpNoZOrder | SwpNoActivate);
    }

    private int CalculateReasonPanelHeight()
    {
        using var font = new Font("Microsoft JhengHei UI",13f,GraphicsUnit.Pixel);
        var measured = TextRenderer.MeasureText(snapshot.Message,font,new Size(capsuleWidth-24,int.MaxValue),
            TextFormatFlags.WordBreak | TextFormatFlags.NoPrefix | TextFormatFlags.NoPadding);
        return Math.Max(48, measured.Height) + (snapshot.State == CapsuleState.RetryLocal ? 52 : 20);
    }

    private int CalculatePreviewPanelHeight()
    {
        var text = snapshot.PreviewText ?? string.Empty;
        using var font = new Font("Microsoft JhengHei UI", textSize, GraphicsUnit.Pixel);
        var measured = TextRenderer.MeasureText(text, font, new Size(capsuleWidth - 64, int.MaxValue),
            TextFormatFlags.WordBreak | TextFormatFlags.NoPrefix | TextFormatFlags.NoPadding);
        return 24 + Math.Clamp(measured.Height, textSize*3, textSize*9);
    }

    private void DrawPreviewPanel(Graphics graphics)
    {
        var rect = new RectangleF(CapsuleX, CapsuleY + CapsuleHeight + 8, capsuleWidth, previewPanelHeight);
        using var path = RoundedRect(rect, 12);
        using (var fill = new LinearGradientBrush(
            rect,
            darkMode ? Color.FromArgb(246, 43, 55, 70) : Color.FromArgb(246, 255, 255, 255),
            darkMode ? Color.FromArgb(242, 24, 35, 48) : Color.FromArgb(236, 244, 248, 253),
            LinearGradientMode.Vertical))
        {
            graphics.FillPath(fill, path);
        }
        using (var border = new Pen(Color.FromArgb(76, 180, 197, 220), 0.7f))
        {
            graphics.DrawPath(border, path);
        }

        using var textBrush = new SolidBrush(darkMode ? Color.FromArgb(224, 235, 249) : Color.FromArgb(47, 62, 81));
        using var textFont = new Font("Microsoft JhengHei UI", textSize, GraphicsUnit.Pixel);
        using var format = new StringFormat
        {
            Trimming = StringTrimming.EllipsisCharacter,
            LineAlignment = StringAlignment.Near,
        };
        graphics.DrawString(
            snapshot.PreviewText ?? string.Empty,
            textFont,
            textBrush,
            new RectangleF(rect.X + 12, rect.Y + 10, rect.Width - 58, rect.Height - 20),
            format);
        DrawCopyIcon(graphics, rect.Right - 25, rect.Y + 22, darkMode, copyConfirmed);
    }

    private static void DrawCopyIcon(Graphics graphics, float cx, float cy, bool darkMode, bool confirmed)
    {
        var color = confirmed
            ? Color.FromArgb(45, 190, 129)
            : darkMode ? Color.FromArgb(220, 232, 247) : Color.FromArgb(45, 61, 80);
        using var pen = new Pen(color, 1.15f)
        {
            StartCap = LineCap.Round,
            EndCap = LineCap.Round,
            LineJoin = LineJoin.Round,
        };
        if (confirmed)
        {
            graphics.DrawLines(pen,
            [
                new PointF(cx - 5, cy),
                new PointF(cx - 1, cy + 4),
                new PointF(cx + 6, cy - 5),
            ]);
            return;
        }

        using var back = RoundedRect(new RectangleF(cx - 6, cy - 6, 9, 10), 2);
        using var front = RoundedRect(new RectangleF(cx - 2, cy - 3, 9, 10), 2);
        graphics.DrawPath(pen, back);
        graphics.DrawPath(pen, front);
    }

    private static void DrawChevron(Graphics graphics, int cx, int cy, bool expanded, bool darkMode)
    {
        using var pen = new Pen(darkMode ? Color.FromArgb(218, 230, 246) : Color.FromArgb(41, 55, 73), 1.2f)
        {
            StartCap = LineCap.Round,
            EndCap = LineCap.Round,
        };
        var direction = expanded ? -1f : 1f;
        graphics.DrawLine(pen, cx - 3.5f, cy - 1.5f * direction, cx, cy + 2f * direction);
        graphics.DrawLine(pen, cx, cy + 2f * direction, cx + 3.5f, cy - 1.5f * direction);
    }

    private static void DrawSettingsIcon(Graphics graphics, int cx, int cy, bool darkMode, bool disabled = false)
    {
        var iconColor = darkMode ? Color.FromArgb(224, 234, 247) : Color.FromArgb(31, 45, 61);
        if (disabled) iconColor = Color.FromArgb(85, iconColor);
        using var pen = new Pen(iconColor, 1.15f) { StartCap = LineCap.Round, EndCap = LineCap.Round };
        using var knobFill = new SolidBrush(darkMode ? Color.FromArgb(255, 31, 43, 57) : Color.FromArgb(255, 250, 252, 255));
        using var knobPen = new Pen(iconColor, 1.05f);
        graphics.DrawLine(pen, cx - 6.5f, cy - 5.5f, cx + 6.5f, cy - 5.5f);
        graphics.DrawLine(pen, cx - 6.5f, cy, cx + 6.5f, cy);
        graphics.DrawLine(pen, cx - 6.5f, cy + 5.5f, cx + 6.5f, cy + 5.5f);
        DrawKnob(graphics, knobFill, knobPen, cx - 2, cy - 5.5f);
        DrawKnob(graphics, knobFill, knobPen, cx + 3.5f, cy);
        DrawKnob(graphics, knobFill, knobPen, cx - 3.5f, cy + 5.5f);
        if (disabled)
        {
            using var font = new Font("Microsoft JhengHei UI", 9f, GraphicsUnit.Pixel);
            using var brush = new SolidBrush(darkMode ? Color.LightGray : Color.DimGray);
            graphics.DrawString("稍後", font, brush, cx - 10, cy + 9);
        }
    }

    private static void DrawCloseIcon(Graphics graphics, int cx, int cy, bool darkMode)
    {
        using var pen = new Pen(darkMode ? Color.FromArgb(226, 236, 248) : Color.FromArgb(31, 45, 61), 1.45f) { StartCap = LineCap.Round, EndCap = LineCap.Round };
        graphics.DrawLine(pen, cx - 4.5f, cy - 4.5f, cx + 4.5f, cy + 4.5f);
        graphics.DrawLine(pen, cx + 4.5f, cy - 4.5f, cx - 4.5f, cy + 4.5f);
    }

    private static void DrawKnob(Graphics graphics, Brush fill, Pen pen, float cx, float cy)
    {
        var rect = new RectangleF(cx - 1.45f, cy - 1.45f, 2.9f, 2.9f);
        graphics.FillEllipse(fill, rect);
        graphics.DrawEllipse(pen, rect);
    }

    private static GraphicsPath RoundedRect(RectangleF rect, float radius)
    {
        var path = new GraphicsPath();
        var diameter = radius * 2;
        path.AddArc(rect.X, rect.Y, diameter, diameter, 180, 90);
        path.AddArc(rect.Right - diameter, rect.Y, diameter, diameter, 270, 90);
        path.AddArc(rect.Right - diameter, rect.Bottom - diameter, diameter, diameter, 0, 90);
        path.AddArc(rect.X, rect.Bottom - diameter, diameter, diameter, 90, 90);
        path.CloseFigure();
        return path;
    }

    private PointNative GetWindowPosition()
    {
        GetWindowRect(hwnd, out var rect);
        return new PointNative(rect.Left, rect.Top);
    }

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    private struct WndClass
    {
        public uint Style;
        public nint WndProc;
        public int ClassExtra;
        public int WindowExtra;
        public nint Instance;
        public nint Icon;
        public nint Cursor;
        public nint Background;
        public string? MenuName;
        public string ClassName;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct RectNative
    {
        public int Left;
        public int Top;
        public int Right;
        public int Bottom;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct MonitorInfo
    {
        public int Size;
        public RectNative Monitor;
        public RectNative WorkArea;
        public uint Flags;
    }

    private enum DockSide
    {
        None,
        Left,
        Right,
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct PointNative
    {
        public int X;
        public int Y;

        public PointNative(int x, int y)
        {
            X = x;
            Y = y;
        }
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct SizeNative
    {
        public int Cx;
        public int Cy;

        public SizeNative(int cx, int cy)
        {
            Cx = cx;
            Cy = cy;
        }
    }

    [StructLayout(LayoutKind.Sequential, Pack = 1)]
    private struct BlendFunction
    {
        public byte BlendOp;
        public byte BlendFlags;
        public byte SourceConstantAlpha;
        public byte AlphaFormat;

        public BlendFunction(int blendOp, byte blendFlags, byte sourceConstantAlpha, int alphaFormat)
        {
            BlendOp = (byte)blendOp;
            BlendFlags = blendFlags;
            SourceConstantAlpha = sourceConstantAlpha;
            AlphaFormat = (byte)alphaFormat;
        }
    }

    private delegate nint WndProcDelegate(nint hWnd, uint message, nuint wParam, nint lParam);

    [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern ushort RegisterClass(ref WndClass lpWndClass);

    [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern nint CreateWindowEx(uint exStyle, string className, string windowName, uint style, int x, int y, int width, int height, nint parent, nint menu, nint instance, nint param);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern nint DefWindowProc(nint hWnd, uint msg, nuint wParam, nint lParam);

    [DllImport("user32.dll")]
    private static extern bool ShowWindow(nint hWnd, int nCmdShow);

    [DllImport("user32.dll")] private static extern nint SetParent(nint child, nint parent);
    [DllImport("user32.dll")] private static extern bool SetForegroundWindow(nint window);
    [DllImport("user32.dll")] private static extern nint GetForegroundWindow();
    [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(nint window, out uint process);
    [DllImport("user32.dll")] private static extern bool IsWindow(nint window);
    [DllImport("user32.dll")] private static extern bool IsWindowVisible(nint window);
    [DllImport("user32.dll")] private static extern bool IsIconic(nint window);
    [DllImport("user32.dll")] private static extern bool IsChild(nint parent, nint child);
    [DllImport("user32.dll", EntryPoint = "GetWindowLongPtrW")] private static extern nint GetWindowLongPtr64(nint window, int index);
    [DllImport("user32.dll", EntryPoint = "SetWindowLongPtrW")] private static extern nint SetWindowLongPtr64(nint window, int index, nint value);
    [DllImport("user32.dll", EntryPoint = "GetWindowLongW")] private static extern int GetWindowLong32(nint window, int index);
    [DllImport("user32.dll", EntryPoint = "SetWindowLongW")] private static extern int SetWindowLong32(nint window, int index, int value);
    private static nint GetWindowLongPtr(nint window, int index) => IntPtr.Size == 8 ? GetWindowLongPtr64(window, index) : (nint)GetWindowLong32(window, index);
    private static nint SetWindowLongPtr(nint window, int index, nint value) => IntPtr.Size == 8 ? SetWindowLongPtr64(window, index, value) : (nint)SetWindowLong32(window, index, (int)value);

    [DllImport("user32.dll")]
    private static extern bool SetWindowPos(nint hWnd, nint hWndInsertAfter, int x, int y, int cx, int cy, uint flags);

    [DllImport("user32.dll")]
    private static extern nint SetCapture(nint hWnd);

    [DllImport("user32.dll")]
    private static extern bool ReleaseCapture();

    [DllImport("user32.dll")]
    private static extern bool GetCursorPos(out PointNative point);

    [DllImport("user32.dll")]
    private static extern bool DestroyWindow(nint hWnd);

    [DllImport("user32.dll")]
    private static extern int GetSystemMetrics(int nIndex);

    [DllImport("user32.dll")]
    private static extern uint GetDpiForSystem();

    [DllImport("user32.dll")]
    private static extern uint GetDpiForWindow(nint window);

    [DllImport("user32.dll")]
    private static extern nint LoadCursor(nint hInstance, nint cursorName);

    [DllImport("user32.dll")]
    private static extern nint GetDC(nint hWnd);

    [DllImport("user32.dll")]
    private static extern int ReleaseDC(nint hWnd, nint hdc);

    [DllImport("user32.dll")]
    private static extern bool GetWindowRect(nint hWnd, out RectNative lpRect);

    [DllImport("user32.dll")]
    private static extern nint MonitorFromWindow(nint hwnd, uint flags);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern bool GetMonitorInfo(nint monitor, ref MonitorInfo monitorInfo);

    [DllImport("user32.dll", SetLastError = true)]
    private static extern bool UpdateLayeredWindow(nint hwnd, nint hdcDst, ref PointNative pptDst, ref SizeNative psize, nint hdcSrc, ref PointNative pptSrc, int crKey, ref BlendFunction pblend, int dwFlags);

    [DllImport("gdi32.dll")]
    private static extern nint CreateCompatibleDC(nint hdc);

    [DllImport("gdi32.dll")]
    private static extern bool DeleteDC(nint hdc);

    [DllImport("gdi32.dll")]
    private static extern nint SelectObject(nint hdc, nint obj);

    [DllImport("gdi32.dll")]
    private static extern bool DeleteObject(nint obj);

    [DllImport("kernel32.dll", CharSet = CharSet.Unicode)]
    private static extern nint GetModuleHandle(string? lpModuleName);
}
