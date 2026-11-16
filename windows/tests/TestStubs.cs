namespace VoiceInput.App.Services;

internal sealed record Microphone(string Id, string Name, int Number);

internal static class Microphones
{
    public static List<Microphone> Available() => [];
    public static IDisposable Listen(int device, Action<float> level) => new MemoryStream();
}
