using Microsoft.Win32;

/// <summary>Zhuyin choices the text service reads from the registry when an application loads it.</summary>
internal sealed record ZhuyinOptions(int Layout = 0, bool Staged = false, int CandidateSize = 24, int LineSize = 44)
{
    public static readonly int[] CandidateSizes = { 20, 24, 32 }, LineSizes = { 0, 36, 44, 52 };
    const string EngineKey = @"Software\Tingzi\Engine", LookKey = @"Software\Tingzi\Appearance";

    public static ZhuyinOptions Load()
    {
        try
        {
            using var engine = Registry.CurrentUser.OpenSubKey(EngineKey);
            using var look = Registry.CurrentUser.OpenSubKey(LookKey);
            int Number(RegistryKey? key, string name, int fallback) => key?.GetValue(name) is int value ? value : fallback;
            int candidate = Number(look, "CandidateDIP", 24), line = Number(look, "CompositionDIP", 44);
            return new(Number(engine, "KeyboardLayout", 0) == 1 ? 1 : 0, Number(engine, "StagedSegmentation", 0) == 1,
                CandidateSizes.Contains(candidate) ? candidate : 24, LineSizes.Contains(line) ? line : 44);
        }
        catch (Exception ex) when (ex is System.Security.SecurityException or UnauthorizedAccessException or IOException) { return new(); }
    }

    public void Save()
    {
        using (var engine = Registry.CurrentUser.CreateSubKey(EngineKey))
        {
            engine.SetValue("KeyboardLayout", Layout, RegistryValueKind.DWord);
            engine.SetValue("StagedSegmentation", Staged ? 1 : 0, RegistryValueKind.DWord);
        }
        using var look = Registry.CurrentUser.CreateSubKey(LookKey);
        look.SetValue("CandidateDIP", CandidateSize, RegistryValueKind.DWord);
        look.SetValue("CompositionDIP", LineSize, RegistryValueKind.DWord);
    }
}

/// <summary>Words the user added, stored beside the learning data in the same line format as the word list.</summary>
internal static class UserPhrases
{
    public const int Longest = 8;
    public static string FilePath => Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "Tingzi", "user-phrases.txt");

    public static List<(string Reading, string Word)> Load(string? path = null)
    {
        var words = new List<(string, string)>();
        try
        {
            foreach (var line in File.ReadLines(path ?? FilePath))
            {
                var parts = line.Split(' ', StringSplitOptions.RemoveEmptyEntries);
                if (parts.Length >= 2 && parts[0].Contains('-')) words.Add((parts[0], parts[1]));
            }
        }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException) { }
        return words;
    }

    public static void Save(IEnumerable<(string Reading, string Word)> words, string? path = null)
    {
        path ??= FilePath;
        Directory.CreateDirectory(Path.GetDirectoryName(path)!);
        var tmp = path + "." + Guid.NewGuid().ToString("N") + ".tmp";
        try { File.WriteAllLines(tmp, words.Select(word => $"{word.Reading} {word.Word} 0"), new System.Text.UTF8Encoding(false)); File.Move(tmp, path, true); }
        finally { File.Delete(tmp); }
    }

    /// <summary>The readings each character has in the installed word list, most common first.</summary>
    public sealed class Lexicon
    {
        readonly Dictionary<char, List<(string Reading, double Score)>> readings = new();

        public static Lexicon Read(string phraseFile)
        {
            var lexicon = new Lexicon();
            foreach (var line in File.ReadLines(phraseFile))
            {
                if (line.Length == 0 || line[0] < 0x80) continue;
                var parts = line.Split(' ', StringSplitOptions.RemoveEmptyEntries);
                if (parts.Length < 3 || parts[0].Contains('-') || parts[1].Length != 1) continue;
                if (!double.TryParse(parts[2], System.Globalization.NumberStyles.Float, System.Globalization.CultureInfo.InvariantCulture, out var score)) continue;
                if (!lexicon.readings.TryGetValue(parts[1][0], out var list)) lexicon.readings[parts[1][0]] = list = new();
                if (!list.Any(entry => entry.Reading == parts[0])) list.Add((parts[0], score));
            }
            foreach (var list in lexicon.readings.Values) list.Sort((a, b) => b.Score.CompareTo(a.Score));
            return lexicon;
        }

        public IReadOnlyList<string> For(char character) =>
            readings.TryGetValue(character, out var list) ? list.Select(entry => entry.Reading).ToArray() : Array.Empty<string>();
    }
}
