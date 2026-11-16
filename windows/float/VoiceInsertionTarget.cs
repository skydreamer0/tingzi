using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Text;
using System.Windows.Automation;

internal enum VoiceInsertResult { Inserted, PreviewRequired, Cancelled, AlreadyAttempted }

internal interface IVoiceInsertionTarget
{
    bool DeliveryActive { get; }
    Task WaitForDeliveryAsync(CancellationToken token) => Task.CompletedTask;
    Task<VoiceInsertResult> TryInsertAsync(string text, CancellationToken cancellationToken = default, Func<bool>? isCurrent = null,
        bool prepareClipboard = true, Func<bool>? clipboardReady = null);
}

internal sealed class VoiceInsertionTarget : IDisposable, IVoiceInsertionTarget
{
    const uint EventSystemForeground = 0x0003, EventObjectFocus = 0x8005;
    const uint AncestorRoot = 2, ReplaceSelection = 0x00C2, AbortIfHung = 0x0002;
    const int WindowStyle = -16, EditReadOnly = 0x0800, EditPassword = 0x0020, EditMultiline = 0x0004;
    readonly WinEventProc eventProc;
    readonly CancellationTokenSource probeLifetime = new();
    Task<string?>? codexIdentity;
    nint foregroundHook, focusHook, window, nativeEditor;
    uint processId;
    string windowClass = "";

    int invalid, attempted, cancelled, disposed;
    bool capturing = true;

    VoiceInsertionTarget()
    {
        eventProc = OnWindowEvent;

    }

    public static VoiceInsertionTarget Capture() => Capture(false);

    internal static VoiceInsertionTarget Capture(bool codexOnly)
    {
        var target = new VoiceInsertionTarget();
        try {
            target.foregroundHook = SetWinEventHook(EventSystemForeground, EventSystemForeground, 0, target.eventProc, 0, 0, 0);
            target.window = GetForegroundWindow();
            var name = new StringBuilder(128);
            if (GetClassName(target.window, name, name.Capacity) == 0) target.Invalidate();
            target.windowClass = name.ToString();
            GetWindowThreadProcessId(target.window, out target.processId);
            target.focusHook = SetWinEventHook(EventObjectFocus, EventObjectFocus, 0, target.eventProc, 0, 0, 0);
            if (!codexOnly && target.TryNativeEditor(out var editor)) target.nativeEditor = editor;
            else if (CodexDraft.IsCodexWindow(target.window))
                target.codexIdentity = CodexFocusProbe.QueryAsync(target.window, target.probeLifetime.Token);
            else target.Invalidate();
            if (target.foregroundHook == 0 || target.focusHook == 0) target.Invalidate();
            target.capturing = false;
            if (!target.IsCurrent()) target.Invalidate();
        } catch (Exception ex) when (IsTargetException(ex)) { target.Invalidate(); target.capturing = false; }
        return target;
    }

    public bool DeliveryActive => Volatile.Read(ref cancelled) == 0 && Volatile.Read(ref disposed) == 0;

    public async Task WaitForDeliveryAsync(CancellationToken token) {
        for (int i=0; i<40 && ShortcutHeld(); i++) await Task.Delay(25,token);
        token.ThrowIfCancellationRequested();
    }

    public void Cancel() { Interlocked.Exchange(ref cancelled, 1); Invalidate(); }

    public async Task<VoiceInsertResult> TryInsertAsync(string text, CancellationToken cancellationToken = default, Func<bool>? isCurrent = null, bool prepareClipboard = true, Func<bool>? clipboardReady = null)
    {
        if (Volatile.Read(ref cancelled) != 0 || cancellationToken.IsCancellationRequested) return VoiceInsertResult.Cancelled;
        if (Interlocked.Exchange(ref attempted, 1) != 0) return VoiceInsertResult.AlreadyAttempted;
        if (string.IsNullOrWhiteSpace(text)) return VoiceInsertResult.PreviewRequired;
        try {
            if (prepareClipboard) await WaitForDeliveryAsync(cancellationToken);
            if (Cancelled(cancellationToken)) return VoiceInsertResult.Cancelled;
            if (ShortcutHeld() || isCurrent?.Invoke() == false || !IsCurrent()) return VoiceInsertResult.PreviewRequired;
            if (nativeEditor != 0) {
                if (Cancelled(cancellationToken) || isCurrent?.Invoke() == false || !IsCurrent()) return VoiceInsertResult.PreviewRequired;
                // A directed message cannot paste into a different application if focus changes.
                var ok = SendMessageTimeout(nativeEditor, ReplaceSelection, 1, text, AbortIfHung, 1000, out _);
                return ok != 0 ? VoiceInsertResult.Inserted : VoiceInsertResult.PreviewRequired;
            }
            if (!await CodexStillCurrentAsync(cancellationToken)) return VoiceInsertResult.PreviewRequired;
            uint preparedSequence=0;
            if (prepareClipboard) { Clipboard.SetText(text, TextDataFormat.UnicodeText); preparedSequence=GetClipboardSequenceNumber(); }
            else if (clipboardReady?.Invoke() != true) return VoiceInsertResult.PreviewRequired;
            if (Cancelled(cancellationToken)) return VoiceInsertResult.Cancelled;
            if (isCurrent?.Invoke() == false || !IsCurrent()) return VoiceInsertResult.PreviewRequired;
            if (prepareClipboard ? preparedSequence == 0 || GetClipboardSequenceNumber() != preparedSequence : clipboardReady?.Invoke() != true) return VoiceInsertResult.PreviewRequired;
            if (prepareClipboard && (Clipboard.GetText(TextDataFormat.UnicodeText) != text || GetClipboardSequenceNumber() != preparedSequence)) return VoiceInsertResult.PreviewRequired;
            if (Cancelled(cancellationToken) || isCurrent?.Invoke() == false || !IsCurrent()) return VoiceInsertResult.PreviewRequired;
            if (!await CodexStillCurrentAsync(cancellationToken) || Cancelled(cancellationToken) || isCurrent?.Invoke() == false || !IsCurrent() || (prepareClipboard ? GetClipboardSequenceNumber() != preparedSequence : clipboardReady?.Invoke() != true)) return VoiceInsertResult.PreviewRequired;
            SendKeys.SendWait("^v");
            return VoiceInsertResult.Inserted;
        } catch (OperationCanceledException) { return VoiceInsertResult.Cancelled; }
        catch (Exception ex) when (IsTargetException(ex)) { return VoiceInsertResult.PreviewRequired; }
    }

    bool Cancelled(CancellationToken token) => token.IsCancellationRequested || Volatile.Read(ref cancelled) != 0;
    void Invalidate() => Interlocked.Exchange(ref invalid, 1);

    bool IsCurrent()
    {
        if (Volatile.Read(ref invalid) != 0 || Volatile.Read(ref disposed) != 0 || GetForegroundWindow() != window) return false;
        GetWindowThreadProcessId(window, out var pid);
        if (pid != processId) return false;
        var name = new StringBuilder(128);
        if (GetClassName(window, name, name.Capacity) == 0 || name.ToString() != windowClass) return false;
        if (nativeEditor != 0) return TryNativeEditor(out var editor) && editor == nativeEditor;
        return codexIdentity != null && !probeLifetime.IsCancellationRequested && CodexDraft.IsCodexWindow(window);
    }

    bool TryNativeEditor(out nint editor)
    {
        editor = 0;
        var thread = GetWindowThreadProcessId(window, out var pid);
        if (pid != processId || pid == 0) return false;
        using var process = Process.GetProcessById((int)pid);
        if (!process.ProcessName.Equals("Notepad", StringComparison.OrdinalIgnoreCase)) return false;
        var info = new GuiThreadInfo { Size = (uint)Marshal.SizeOf<GuiThreadInfo>() };
        if (!GetGUIThreadInfo(thread, ref info) || info.Focus == 0 || GetAncestor(info.Focus, AncestorRoot) != window) return false;
        GetWindowThreadProcessId(info.Focus, out var focusPid);
        var name = new StringBuilder(128);
        if (focusPid != pid || GetClassName(info.Focus, name, name.Capacity) == 0) return false;
        var style = GetWindowLong(info.Focus, WindowStyle);
        if ((style & (EditReadOnly | EditPassword)) != 0 || !IsWindowEnabled(info.Focus)) return false;
        var richEdit = name.ToString().StartsWith("RichEdit", StringComparison.OrdinalIgnoreCase);
        if (!richEdit) {
            if (!name.ToString().Equals("Edit", StringComparison.OrdinalIgnoreCase) || (style & EditMultiline) == 0 || GetParent(info.Focus) != window) return false;
            name.Clear();
            if (GetClassName(window, name, name.Capacity) == 0 || !name.ToString().Equals("Notepad", StringComparison.OrdinalIgnoreCase)) return false;
        }
        editor = info.Focus;
        return true;
    }

    void OnWindowEvent(nint hook, uint kind, nint hwnd, int objectId, int childId, uint thread, uint time)
    {
        if (capturing) { Invalidate(); return; }
        if (kind == EventSystemForeground) { if (hwnd != window) Invalidate(); return; }
        if (kind != EventObjectFocus || hwnd == 0 || GetAncestor(hwnd, AncestorRoot) != window) return;
        if (nativeEditor != 0) { if (hwnd != nativeEditor) Invalidate(); }
        else Invalidate();
    }

    async Task<bool> CodexStillCurrentAsync(CancellationToken token)
    {
        if (!IsCurrent() || codexIdentity == null) return false;
        using var linked = CancellationTokenSource.CreateLinkedTokenSource(token, probeLifetime.Token);
        try {
            var original = await codexIdentity.WaitAsync(linked.Token);
            if (original == null || !IsCurrent()) { Invalidate(); return false; }
            var current = await CodexFocusProbe.QueryAsync(window, linked.Token);
            if (current == null || current != original || !IsCurrent()) { Invalidate(); return false; }
            return true;
        } catch (OperationCanceledException) { Invalidate(); return false; }
    }

    public void Dispose()
    {
        if (Interlocked.Exchange(ref disposed, 1) != 0) return;
        Invalidate();
        ReleaseHooks();
        probeLifetime.Cancel();
    }

    void ReleaseHooks()
    {
        if (foregroundHook != 0) { UnhookWinEvent(foregroundHook); foregroundHook = 0; }
        if (focusHook != 0) { UnhookWinEvent(focusHook); focusHook = 0; }
    }

    static bool ShortcutHeld() => GetAsyncKeyState(0x11) < 0 || GetAsyncKeyState(0x12) < 0 || GetAsyncKeyState(0x56) < 0 || GetAsyncKeyState(0x10) < 0 || GetAsyncKeyState(0x5B) < 0 || GetAsyncKeyState(0x5C) < 0;
    static bool IsTargetException(Exception ex) => ex is ExternalException or System.ComponentModel.Win32Exception or InvalidOperationException or ArgumentException or UnauthorizedAccessException or ElementNotAvailableException or NotSupportedException;
    delegate void WinEventProc(nint hook, uint kind, nint hwnd, int objectId, int childId, uint thread, uint time);
    [StructLayout(LayoutKind.Sequential)] struct GuiThreadInfo { public uint Size, Flags; public nint Active, Focus, Capture, MenuOwner, MoveSize, Caret; public int Left, Top, Right, Bottom; }
    [DllImport("user32")] static extern nint SetWinEventHook(uint min, uint max, nint module, WinEventProc callback, uint process, uint thread, uint flags);
    [DllImport("user32")] static extern bool UnhookWinEvent(nint hook);
    [DllImport("user32")] static extern nint GetForegroundWindow();
    [DllImport("user32")] static extern uint GetWindowThreadProcessId(nint window, out uint pid);
    [DllImport("user32")] static extern bool GetGUIThreadInfo(uint thread, ref GuiThreadInfo info);
    [DllImport("user32")] static extern nint GetAncestor(nint window, uint flags);
    [DllImport("user32")] static extern nint GetParent(nint window);
    [DllImport("user32", CharSet = CharSet.Unicode)] static extern int GetClassName(nint window, StringBuilder name, int count);
    [DllImport("user32", CharSet = CharSet.Unicode, EntryPoint = "GetWindowLongW")] static extern int GetWindowLong(nint window, int index);
    [DllImport("user32")] static extern bool IsWindowEnabled(nint window);
    [DllImport("user32")] static extern short GetAsyncKeyState(int key);
    [DllImport("user32")] static extern uint GetClipboardSequenceNumber();
    [DllImport("user32", CharSet = CharSet.Unicode)] static extern nint SendMessageTimeout(nint window, uint message, nint wparam, string text, uint flags, uint timeout, out nint result);
}


internal sealed record VoiceDeliveryResult(bool Copied, VoiceInsertResult Insertion, bool AutoExpandPreview, string Message);

internal static class VoiceTextDelivery
{
    public static async Task<VoiceDeliveryResult> DeliverAsync(string text, VoiceDeliveryMode mode,
        IVoiceInsertionTarget? target, CancellationToken token = default, Func<bool>? isCurrent = null,
        Func<string,bool>? copyText = null, Func<string,bool>? clipboardMatches = null)
    {
        bool Current() => !token.IsCancellationRequested && isCurrent?.Invoke() != false && target?.DeliveryActive != false;
        if (!Current()) return new(false,VoiceInsertResult.Cancelled,false,"已取消");
        if (string.IsNullOrWhiteSpace(text)) return new(false,VoiceInsertResult.PreviewRequired,false,"沒有辨識文字");
        if (mode == VoiceDeliveryMode.PreviewOnly) return new(false,VoiceInsertResult.PreviewRequired,true,"辨識完成・點文字編輯");
        if (mode == VoiceDeliveryMode.Auto && target != null) {
            try { await target.WaitForDeliveryAsync(token); }
            catch (OperationCanceledException) { return new(false,VoiceInsertResult.Cancelled,false,"已取消"); }
            if (!Current()) return new(false,VoiceInsertResult.Cancelled,false,"已取消");
        }
        bool copied=false;
        uint sequence=0;
        try {
            if (!Current()) return new(false,VoiceInsertResult.Cancelled,false,"已取消");
            if (copyText != null) copied=copyText(text);
            else { Clipboard.SetText(text,TextDataFormat.UnicodeText); copied=true; }
            if (copied && clipboardMatches == null) sequence=GetClipboardSequenceNumber();
        } catch (ExternalException) { }
        bool ClipboardReady() {
            if (!copied || !Current()) return false;
            try {
                if (clipboardMatches != null) return clipboardMatches(text);
                return sequence != 0 && GetClipboardSequenceNumber() == sequence &&
                    Clipboard.GetText(TextDataFormat.UnicodeText) == text && GetClipboardSequenceNumber() == sequence;
            } catch (ExternalException) { return false; }
        }
        var inserted=VoiceInsertResult.PreviewRequired;
        if (mode == VoiceDeliveryMode.Auto && target != null && Current()) {
            try { inserted=await target.TryInsertAsync(text,token,isCurrent,false,ClipboardReady); }
            catch (OperationCanceledException) { inserted=VoiceInsertResult.Cancelled; }
            catch (Exception ex) when (ex is not OperationCanceledException) { }
        }
        var clipboardChanged=copied && !ClipboardReady();
        var message=clipboardChanged
            ? inserted == VoiceInsertResult.Inserted ? "已插入・剪貼簿已變更，結果保留預覽" : "剪貼簿已變更・結果保留預覽"
            : inserted == VoiceInsertResult.Inserted
                ? copied ? "已插入並複製文字" : "已插入・剪貼簿忙碌，結果可取回"
                : copied ? "已複製文字・可手動貼上" : "剪貼簿忙碌・結果可取回";
        return new(copied,inserted,clipboardChanged || !copied || (mode == VoiceDeliveryMode.Auto && inserted != VoiceInsertResult.Inserted),message);
    }
    [DllImport("user32")] static extern uint GetClipboardSequenceNumber();
}
