using System.Diagnostics;
using System.Text;
using System.Text.Json;
using VoiceInput.App.Services;
using VoiceInput.Core.Recognition;

internal sealed class OnlineRecognition : IRecognitionEngine
{
    public string DisplayName => "Codex 線上辨識";
    public async Task<RecognitionResult> TranscribeAsync(string path, RecognitionOptions options, CancellationToken token)
    {
        token.ThrowIfCancellationRequested();
        if (!string.IsNullOrEmpty(Environment.GetEnvironmentVariable("AUDIO_CODEX_ASR_URL"))) {
            var bridge = new CodexAsrRecognitionEngine();
            var result = await bridge.TranscribeAsync(path, options, token);
            return new RecognitionResult(LocalWhisperRecognitionEngine.ToTraditionalChinese(result.Text));
        }
        var cli = Path.Combine(AppContext.BaseDirectory, "codex-asr.exe");
        if (!File.Exists(cli)) throw new InvalidOperationException("缺少 codex-asr.exe，請檢查安裝包");
        if (new FileInfo(path).Length is <= 44 or > 50 * 1024 * 1024)
            throw new InvalidOperationException("音檔為空或超過 50 MiB");
        // codex-asr 0.1.2 falls back to the working directory on Windows when HOME is absent.
        var start = new ProcessStartInfo(cli) { UseShellExecute = false, CreateNoWindow = true,
            WorkingDirectory = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile),
            RedirectStandardOutput = true, RedirectStandardError = true,
            StandardOutputEncoding = Encoding.UTF8, StandardErrorEncoding = Encoding.UTF8 };
        foreach (var arg in new[] { "transcribe", Path.GetFullPath(path), "--language", "zh", "--json", "--filename", "audio.wav", "--request-timeout-seconds", "90" })
            start.ArgumentList.Add(arg);
        using var job = Process.Start(start) ?? throw new InvalidOperationException("無法啟動 Codex ASR");
        using var timeout = CancellationTokenSource.CreateLinkedTokenSource(token);
        timeout.CancelAfter(TimeSpan.FromSeconds(100));
        var output = job.StandardOutput.ReadToEndAsync();
        var error = job.StandardError.ReadToEndAsync();
        try { await job.WaitForExitAsync(timeout.Token); }
        catch (OperationCanceledException) {
            try { job.Kill(true); } catch (InvalidOperationException) { }
            await job.WaitForExitAsync(); await Task.WhenAll(output,error);
            token.ThrowIfCancellationRequested();
            throw new InvalidOperationException("Codex 線上辨識逾時");
        }
        await Task.WhenAll(output, error);
        if (job.ExitCode != 0) {
            var detail = await error;
            string reason = detail.Contains("401") || detail.Contains("403") ? "帳號或服務拒絕請求" :
                detail.Contains("429") ? "服務要求稍後再試" :
                detail.Contains("auth", StringComparison.OrdinalIgnoreCase) ? "請確認 Codex 已登入" : "請檢查網路及服務狀態";
            throw new InvalidOperationException("線上辨識失敗：" + reason);
        }
        using var json = JsonDocument.Parse(await output);
        var text = json.RootElement.TryGetProperty("text", out var value) ? value.GetString()?.Trim() : null;
        if (string.IsNullOrWhiteSpace(text)) return new RecognitionResult("");
        return new RecognitionResult(LocalWhisperRecognitionEngine.ToTraditionalChinese(text));
    }
}
