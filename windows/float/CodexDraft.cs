using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Windows.Automation;
using System.Text;

// Register only while there is a preview. The user invokes the command inside the intended editor.
internal sealed class CodexDraft : NativeWindow, IDisposable
{
    const int Id = 0x4730;
    readonly Func<string?> preview;
    readonly Action<bool> report;
    bool registered, pasting;
    public CodexDraft(Func<string?> preview, Action<bool> report)
    {
        this.preview = preview;
        this.report = report;
        CreateHandle(new CreateParams { Parent = new IntPtr(-3) });
    }
    public bool Enable(bool enable)
    {
        if (enable && !registered) registered = RegisterHotKey(Handle, Id, 0x4003, (uint)Keys.V);
        if (!enable && registered) { UnregisterHotKey(Handle, Id); registered = false; }
        return registered;
    }
    internal static bool IsCodexEditor(nint window, out AutomationElement? editor)
    {
        editor = null;
        if (!IsCodexWindow(window)) return false;
        editor = AutomationElement.FocusedElement;
        if (editor == null || editor.Current.IsPassword || !editor.Current.HasKeyboardFocus) return false;
        if (!editor.Current.IsEnabled || editor.Current.ControlType != ControlType.Edit) return false;
        if (editor.TryGetCurrentPattern(ValuePattern.Pattern, out var value) && ((ValuePattern)value).Current.IsReadOnly) return false;
        // The desktop composer is a ProseMirror editor. Ordinary search boxes have no such class.
        var classes = editor.Current.ClassName.Split(' ', StringSplitOptions.RemoveEmptyEntries);
        if (!classes.Contains("ProseMirror") && editor.Current.AutomationId != "prompt-textarea") return false;
        var root = AutomationElement.FromHandle(window);
        for (var item = editor; item != null; item = TreeWalker.RawViewWalker.GetParent(item))
            if (Automation.Compare(item, root)) return true;
        return false;
    }
    internal static bool IsCodexWindow(nint window)
    {
        if (window == 0 || GetForegroundWindow() != window) return false;
        GetWindowThreadProcessId(window, out uint pid);
        using var process = Process.GetProcessById((int)pid);
        var family = new StringBuilder(128);
        uint length = (uint)family.Capacity;
        return GetPackageFamilyName(process.Handle, ref length, family) == 0 &&
            family.ToString().Equals("OpenAI.Codex_2p2nqsd0c76g0", StringComparison.OrdinalIgnoreCase);
    }
    async void Paste()
    {
        if (pasting) return;
        pasting = true;
        var text = preview();
        bool pasted = false;
        try {
            if (string.IsNullOrEmpty(text)) return;
            using var target = VoiceInsertionTarget.Capture(codexOnly: true);
            pasted = await target.TryInsertAsync(text, isCurrent: () => preview() == text) == VoiceInsertResult.Inserted;
        } catch (Exception ex) when (ex is ExternalException or System.ComponentModel.Win32Exception or InvalidOperationException or ArgumentException or UnauthorizedAccessException or ElementNotAvailableException) {
        } finally { pasting = false; if (text != null && text == preview()) report(pasted); }
    }
    protected override void WndProc(ref Message m)
    {
        if (m.Msg == 0x0312 && m.WParam == Id) { Paste(); return; }
        base.WndProc(ref m);
    }
    public void Dispose() { Enable(false); DestroyHandle(); }
    [DllImport("user32")] static extern nint GetForegroundWindow();
    [DllImport("user32")] static extern uint GetWindowThreadProcessId(nint window, out uint pid);
    [DllImport("user32")] static extern bool RegisterHotKey(nint window, int id, uint modifiers, uint key);
    [DllImport("user32")] static extern bool UnregisterHotKey(nint window, int id);
    [DllImport("kernel32", CharSet = CharSet.Unicode)] static extern int GetPackageFamilyName(nint process, ref uint length, StringBuilder name);
}
