using System.Runtime.InteropServices;
using NAudio.CoreAudioApi;
using NAudio.Wave;

namespace VoiceInput.App.Services;

internal sealed record Microphone(string Id, string Name, int Number);

internal static class Microphones
{
    // WinMM maps its transient index to the Core Audio endpoint identifier.
    private const uint QueryInstanceId = 0x0811, QueryInstanceIdSize = 0x0812;
    [DllImport("winmm.dll", EntryPoint = "waveInMessage")]
    private static extern uint QuerySize(nint device, uint message, out uint bytes, nuint unused);
    [DllImport("winmm.dll", EntryPoint = "waveInMessage")]
    private static extern uint QueryId(nint device, uint message, nint buffer, nuint bytes);

    public static List<Microphone> Available()
    {
        var inputs = new List<Microphone>();
        using var endpoints = new MMDeviceEnumerator();
        for (int number = 0; number < WaveIn.DeviceCount; ++number)
        {
            if (QuerySize(number, QueryInstanceIdSize, out uint bytes, 0) != 0 || bytes is < 4 or > 65536) continue;
            nint buffer = Marshal.AllocHGlobal((int)bytes);
            try
            {
                if (QueryId(number, QueryInstanceId, buffer, bytes) != 0) continue;
                string? id = Marshal.PtrToStringUni(buffer, (int)bytes / 2)?.TrimEnd('\0');
                if (string.IsNullOrEmpty(id)) continue;
                using var device = endpoints.GetDevice(id);
                inputs.Add(new(id, device.FriendlyName, number));
            }
            catch (COMException) { /* The device was unplugged during enumeration. */ }
            finally { Marshal.FreeHGlobal(buffer); }
        }
        return inputs;
    }

    /// <summary>Reports the input peak from 0 to 1 until disposed; device -1 is the Windows default.</summary>
    public static IDisposable Listen(int device, Action<float> level)
    {
        var input = new WaveInEvent { DeviceNumber = device, WaveFormat = new WaveFormat(16000, 16, 1), BufferMilliseconds = 60 };
        input.DataAvailable += (_, e) =>
        {
            int peak = 0;
            for (int i = 0; i + 1 < e.BytesRecorded; i += 2) peak = Math.Max(peak, Math.Abs((int)BitConverter.ToInt16(e.Buffer, i)));
            level(peak / 32768f);
        };
        try { input.StartRecording(); }
        catch { input.Dispose(); throw; }
        return input;
    }

    public static Microphone? Resolve(string? preferred)
    {
        try
        {
            if (string.IsNullOrEmpty(preferred))
            {
                using var endpoints = new MMDeviceEnumerator();
                using var device = endpoints.GetDefaultAudioEndpoint(DataFlow.Capture, Role.Multimedia);
                preferred = device.ID;
            }
            return Available().Find(input => string.Equals(input.Id, preferred, StringComparison.OrdinalIgnoreCase));
        }
        catch (COMException) { return null; }
    }
}
