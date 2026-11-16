namespace VoiceInput.App.Services;

internal sealed record Microphone(string Id, string Name, int Number);

internal static class Microphones
{
    public static List<Microphone> Available() => [];
}
