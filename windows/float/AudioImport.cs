using System.IO.Pipes;
using System.Text;

internal static class AudioImport
{
    public const string RetryLocal = "--retry-local", Discard = "--discard-retry";
    public static bool IsRetryAction(string value) => value is RetryLocal or Discard;
    const string Pipe = "Tingzi.AudioImport.1";
    public static bool IsWave(string path) => Path.IsPathFullyQualified(path)
        && !path.StartsWith(@"\\", StringComparison.Ordinal) && Path.GetExtension(path).Equals(".wav", StringComparison.OrdinalIgnoreCase)
        && DriveInfo.GetDrives().Any(d => d.DriveType == DriveType.Fixed && d.Name.Equals(Path.GetPathRoot(path), StringComparison.OrdinalIgnoreCase))
        && File.Exists(path);

    public static bool Send(string path)
    {
        try {
            using var client = new NamedPipeClientStream(".", Pipe, PipeDirection.InOut, PipeOptions.CurrentUserOnly | PipeOptions.Asynchronous);
            using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(3));
            client.ConnectAsync(timeout.Token).GetAwaiter().GetResult();
            var data = Encoding.UTF8.GetBytes(path);
            if (data.Length > 32767) return false;
            using var writer = new BinaryWriter(client, Encoding.UTF8, true);
            writer.Write((ushort)data.Length); writer.Write(data); writer.Flush();
            var reply = new byte[1];
            client.ReadExactlyAsync(reply, timeout.Token).AsTask().GetAwaiter().GetResult();
            return reply[0] == 1;
        } catch (Exception ex) when (ex is IOException or TimeoutException or OperationCanceledException or UnauthorizedAccessException) { return false; }
    }

    // Runs on the WinForms synchronization context. An empty request opens the file picker.
    public static async Task Listen(Func<string, bool> accept, CancellationToken end)
    {
        while (!end.IsCancellationRequested) {
            using var server = new NamedPipeServerStream(Pipe, PipeDirection.InOut, 1, PipeTransmissionMode.Byte,
                PipeOptions.CurrentUserOnly | PipeOptions.Asynchronous);
            try {
                await server.WaitForConnectionAsync(end);
                using var expires = CancellationTokenSource.CreateLinkedTokenSource(end);
                expires.CancelAfter(TimeSpan.FromSeconds(3));
                var length = new byte[2];
                await server.ReadExactlyAsync(length, expires.Token);
                int size = BitConverter.ToUInt16(length);
                if (size > 32767) continue;
                var data = new byte[size];
                await server.ReadExactlyAsync(data, expires.Token);
                var path = Encoding.UTF8.GetString(data);
                bool ok = (path.Length == 0 || IsRetryAction(path) || IsWave(path)) && accept(path);
                await server.WriteAsync(new byte[] { (byte)(ok ? 1 : 0) }, expires.Token);
            } catch (Exception ex) when (ex is IOException or OperationCanceledException or UnauthorizedAccessException or ArgumentException) { }
        }
    }
}
