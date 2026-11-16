using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Text;
using NAudio.Wave;
using VoiceInput.Core.Recognition;

namespace VoiceInput.App.Services;

// The executable and model are supplied explicitly by the local launcher.
public sealed class LocalWhisperRecognitionEngine : IRecognitionEngine
{
    private readonly string executable;
    private readonly string model;
    public string DisplayName => "本機語音辨識";

    public LocalWhisperRecognitionEngine(string executable, string model)
    {
        this.executable = executable;
        this.model = model;
    }

    public async Task<RecognitionResult> TranscribeAsync(string audioPath, RecognitionOptions options, CancellationToken cancellationToken)
    {
        if (!File.Exists(executable) || !File.Exists(model))
            throw new InvalidOperationException("找不到本機語音引擎或模型，請使用啟動工具重新開啟。");

        var directory = Path.Combine(Path.GetTempPath(), "TingziVoice", "local-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(directory);
        try
        {
            var wavPath = Path.Combine(directory, "input.wav");
            using (var reader = new AudioFileReader(audioPath))
            using (var resampler = new MediaFoundationResampler(reader, new WaveFormat(16000, 16, 1)))
            {
                resampler.ResamplerQuality = 60;
                WaveFileWriter.CreateWaveFile(wavPath, resampler);
            }
            cancellationToken.ThrowIfCancellationRequested();
            var outputBase = Path.Combine(directory, "result");
            var start = new ProcessStartInfo(executable)
            {
                UseShellExecute = false, CreateNoWindow = true,
                RedirectStandardOutput = true, RedirectStandardError = true,
                WorkingDirectory = Path.GetDirectoryName(executable)!,
                StandardOutputEncoding = Encoding.UTF8, StandardErrorEncoding = Encoding.UTF8,
            };
            foreach (var argument in new[] { "-m", model, "-f", wavPath, "-l", options.Language ?? "zh", "-otxt", "-of", outputBase, "-np", "-nt", "-ng",
                // Half the logical processors, between four and eight, measured fastest for short clips.
                "-t", Math.Clamp(Environment.ProcessorCount / 2, 4, 8).ToString(System.Globalization.CultureInfo.InvariantCulture) })
                start.ArgumentList.Add(argument);
            using var process = Process.Start(start) ?? throw new InvalidOperationException("無法啟動本機語音辨識。");
            var stdout = process.StandardOutput.ReadToEndAsync();
            var stderr = process.StandardError.ReadToEndAsync();
            try { await process.WaitForExitAsync(cancellationToken); }
            catch (OperationCanceledException)
            {
                try { process.Kill(entireProcessTree: true); } catch (InvalidOperationException) { }
                await process.WaitForExitAsync();
                throw;
            }
            await Task.WhenAll(stdout, stderr);
            if (process.ExitCode != 0 || !File.Exists(outputBase + ".txt"))
                throw new InvalidOperationException("本機語音辨識失敗，請檢查音檔或模型。");
            var text = (await File.ReadAllTextAsync(outputBase + ".txt", Encoding.UTF8, cancellationToken)).Trim();
            return new RecognitionResult(ToTraditionalChinese(text));
        }
        finally
        {
            try { Directory.Delete(directory, recursive: true); } catch (IOException) { } catch (UnauthorizedAccessException) { }
        }
    }

    internal static string ToTraditionalChinese(string value)
    {
        if (value.Length == 0) return value;
        var result = new char[value.Length * 2 + 1];
        var length = LCMapStringEx("zh-TW", 0x04000000, value, value.Length, result, result.Length, IntPtr.Zero, IntPtr.Zero, IntPtr.Zero);
        return length > 0 ? new string(result, 0, length) : value;
    }

    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern int LCMapStringEx(string locale, uint flags, string source, int sourceLength,
        [Out] char[] destination, int destinationLength, IntPtr version, IntPtr reserved, IntPtr sortHandle);
}
