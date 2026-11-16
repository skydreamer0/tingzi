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

    public static string TemplatePath => Path.Combine(Path.GetDirectoryName(FilePath)!, "user-templates.txt");

    /// <summary>Templates are "reading TAB text"; the text may contain spaces.</summary>
    public static List<(string Reading, string Word)> LoadTemplates(string? path = null)
    {
        var entries = new List<(string, string)>();
        try
        {
            foreach (var line in File.ReadLines(path ?? TemplatePath))
            {
                int tab = line.IndexOf('\t');
                if (tab > 0 && tab < line.Length - 1 && line[..tab].Contains('-')) entries.Add((line[..tab], line[(tab + 1)..]));
            }
        }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException) { }
        return entries;
    }

    public static void SaveTemplates(IEnumerable<(string Reading, string Word)> entries, string? path = null) =>
        Write(path ?? TemplatePath, entries.Select(entry => entry.Reading + "\t" + entry.Word.Replace('\t', ' ').Replace('\r', ' ').Replace('\n', ' ')));

    /// <summary>Replaces same-sounding characters in recognised speech with the user's own words.</summary>
    public static string Correct(string text, IEnumerable<(string Reading, string Word)> words, Lexicon lexicon)
    {
        var characters = text.ToCharArray();
        foreach (var (reading, word) in words.OrderByDescending(entry => entry.Word.Length))
        {
            var sounds = reading.Split('-');
            if (word.Length < 2 || sounds.Length != word.Length) continue;
            for (int at = 0; at + word.Length <= characters.Length; at++)
            {
                bool differs = false, matches = true;
                for (int i = 0; i < word.Length && matches; i++)
                {
                    if (characters[at + i] == word[i]) continue;
                    differs = true;
                    matches = lexicon.For(characters[at + i]).Contains(sounds[i]);
                }
                if (!matches || !differs) continue;
                word.CopyTo(0, characters, at, word.Length);
                at += word.Length - 1;
            }
        }
        return new string(characters);
    }

    static readonly object gate = new();
    static (string Path, Task<Lexicon?> Loaded)? shared;
    /// <summary>The word list's readings, read once in the background and kept.</summary>
    public static Task<Lexicon?> SharedLexicon(string phraseFile)
    {
        lock (gate)
        {
            if (shared?.Path == phraseFile) return shared.Value.Loaded;
            var loading = Task.Run(() =>
            {
                try { return (Lexicon?)Lexicon.Read(phraseFile); }
                catch (Exception ex) when (ex is IOException or UnauthorizedAccessException) { return null; }
            });
            shared = (phraseFile, loading);
            return loading;
        }
    }

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

    public static void Save(IEnumerable<(string Reading, string Word)> words, string? path = null) =>
        Write(path ?? FilePath, words.Select(word => $"{word.Reading} {word.Word} 0"));

    static void Write(string path, IEnumerable<string> lines)
    {
        Directory.CreateDirectory(Path.GetDirectoryName(path)!);
        var tmp = path + "." + Guid.NewGuid().ToString("N") + ".tmp";
        try { File.WriteAllLines(tmp, lines, new System.Text.UTF8Encoding(false)); File.Move(tmp, path, true); }
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
