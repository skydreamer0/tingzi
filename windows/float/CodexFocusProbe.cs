using System.Diagnostics;
using System.Globalization;
using System.Reflection;
using System.Text;

internal static class CodexFocusProbe
{
    const string Argument = "--voice-focus-probe";
    static readonly SemaphoreSlim slot = new(1, 1);
    internal static bool IsProbe(string[] args) => args.Length > 0 && args[0] == Argument;

    internal static int Run(string[] args)
    {
        if (args.Length != 2 || !long.TryParse(args[1], NumberStyles.Integer, CultureInfo.InvariantCulture, out var handle)) return 2;
        try {
            if (!CodexDraft.IsCodexEditor((nint)handle, out var editor) || editor == null) return 3;
            var identity = string.Join(",", editor.GetRuntimeId());
            using var output = new StreamWriter(Console.OpenStandardOutput(), new UTF8Encoding(false));
            output.Write(identity);
            return 0;
        } catch (Exception) { return 3; }
    }

    internal static Task<string?> QueryAsync(nint window, CancellationToken token) => Task.Run(async () => {
        var executable = Environment.ProcessPath;
        if (executable == null) return null;
        var start = new ProcessStartInfo(executable);
        if (Path.GetFileNameWithoutExtension(executable).Equals("dotnet", StringComparison.OrdinalIgnoreCase))
            start.ArgumentList.Add(Assembly.GetExecutingAssembly().Location);
        start.ArgumentList.Add(Argument);
        start.ArgumentList.Add(((long)window).ToString(CultureInfo.InvariantCulture));
        return await RunBoundedAsync(start, token).ConfigureAwait(false);
    });

    internal static async Task<string?> RunBoundedAsync(ProcessStartInfo start, CancellationToken token)
    {
        if (token.IsCancellationRequested || !await slot.WaitAsync(0).ConfigureAwait(false)) return null;
        Process? child = null;
        bool reusable = true;
        using var deadline = CancellationTokenSource.CreateLinkedTokenSource(token);
        deadline.CancelAfter(TimeSpan.FromMilliseconds(900));
        try {
            deadline.Token.ThrowIfCancellationRequested();
            start.UseShellExecute = false;
            start.CreateNoWindow = true;
            start.RedirectStandardOutput = true;
            start.RedirectStandardError = true;
            start.WindowStyle = ProcessWindowStyle.Hidden;
            child = Process.Start(start);
            if (child == null) return null;
            var output = child.StandardOutput.ReadToEndAsync(deadline.Token);
            await child.WaitForExitAsync(deadline.Token).ConfigureAwait(false);
            var identity = await output.ConfigureAwait(false);
            return child.ExitCode == 0 && identity.Length is > 0 and < 512 &&
                identity.All(c => char.IsAsciiDigit(c) || c is ',' or '-') ? identity : null;
        } catch (Exception ex) when (ex is OperationCanceledException or InvalidOperationException or System.ComponentModel.Win32Exception or IOException) {
            return null;
        } finally {
            if (child != null) {
                try {
                    if (!child.HasExited) child.Kill(entireProcessTree: true);
                    using var reap = new CancellationTokenSource(TimeSpan.FromMilliseconds(500));
                    await child.WaitForExitAsync(reap.Token).ConfigureAwait(false);
                } catch (Exception) { reusable = false; }
                child.Dispose();
            }
            // A failed termination quarantines the slot instead of accumulating blocked probes.
            if (reusable) slot.Release();
        }
    }
}
