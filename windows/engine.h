#pragma once
#include <windows.h>
#include <algorithm>
#include <cmath>
#include <cwchar>
#include <filesystem>
#include <fstream>
#include <map>
#include <optional>
#include <sstream>
#include <string>
#include <unordered_map>
#include <unordered_set>
#include <vector>
#include <memory>
#include <optional>
#include "learning.h"

namespace tingzi {
using Text = std::wstring;
struct Term { Text text; double score; };
struct Choice { Text text; size_t count; double score=-12.0; };
#ifdef TINGZI_STANDALONE
inline constexpr wchar_t engineKey[]=L"Software\\TingziStandaloneRC\\Engine";
#else
inline constexpr wchar_t engineKey[]=L"Software\\Tingzi\\Engine";
#endif
// Path keeps the highest total score over the whole sentence. Staged settles long words
// first and then works left to right, so an earlier word keeps its place unless a later
// one is clearly stronger.
enum class Segmenter { Path, Staged };
inline Segmenter configuredSegmenter() {
    DWORD value=0, bytes=sizeof(value);
    if(RegGetValueW(HKEY_CURRENT_USER,engineKey,L"StagedSegmentation",RRF_RT_REG_DWORD,nullptr,&value,&bytes)==ERROR_SUCCESS && value==1)
        return Segmenter::Staged;
    return Segmenter::Path;
}
// Standard is the Daqian layout the word list is keyed by; ETen keys are translated to it.
enum class KeyLayout { Standard, ETen };
inline KeyLayout configuredLayout() {
    DWORD value=0, bytes=sizeof(value);
    if(RegGetValueW(HKEY_CURRENT_USER,engineKey,L"KeyboardLayout",RRF_RT_REG_DWORD,nullptr,&value,&bytes)==ERROR_SUCCESS && value==1)
        return KeyLayout::ETen;
    return KeyLayout::Standard;
}
// Character-level script conversion from Windows; one-to-many characters follow the system table.
inline Text convertScript(const Text& text,bool simplified) {
    if(text.empty())return text;
    const DWORD flag=simplified?LCMAP_SIMPLIFIED_CHINESE:LCMAP_TRADITIONAL_CHINESE;
    int length=LCMapStringEx(LOCALE_NAME_INVARIANT,flag,text.c_str(),int(text.size()),nullptr,0,nullptr,nullptr,0);
    if(length<=0)return text;
    Text result(size_t(length),L'\0');
    length=LCMapStringEx(LOCALE_NAME_INVARIANT,flag,text.c_str(),int(text.size()),&result[0],length,nullptr,nullptr,0);
    return length>0?result:text;
}
inline bool configuredSimplifiedOutput() {
    DWORD value=0, bytes=sizeof(value);
    return RegGetValueW(HKEY_CURRENT_USER,engineKey,L"SimplifiedOutput",RRF_RT_REG_DWORD,nullptr,&value,&bytes)==ERROR_SUCCESS && value==1;
}
// Words the user added rank above the bundled ones with the same reading.
inline constexpr double userPhraseScore=-2.0;
// Low enough that a template is offered in the list and never chosen for the sentence.
inline constexpr double templateScore=-100.0;
struct WordSpan { size_t at, count; int weight; Text text; };
// A later word must lead by this much before it displaces an earlier one.
inline constexpr int displaceMargin=3;
// Picks one word from a run of overlapping spans. Spans arrive ordered by start, and spans
// sharing a start and length are adjacent. Returns an index into spans.
inline size_t chooseSpan(const std::vector<WordSpan>& spans) {
    const size_t n=spans.size();
    auto same=[&](size_t i,size_t j){return spans[i].at==spans[j].at && spans[i].count==spans[j].count;};
    auto strongest=[&](size_t from,size_t to){
        size_t pick=from;
        for(size_t i=from+1;i<to;++i)if(spans[i].weight>spans[pick].weight)pick=i;
        return pick;
    };
    auto end=[&](size_t i){return spans[i].at+spans[i].count;};
    auto last=[&](size_t i){return end(i)-1;};
    size_t firstEnd=1; while(firstEnd<n && same(0,firstEnd))++firstEnd;
    size_t a=strongest(0,firstEnd);
    if(firstEnd==n)return a;
    size_t secondEnd=firstEnd+1; while(secondEnd<n && same(firstEnd,secondEnd))++secondEnd;
    size_t b=strongest(firstEnd,secondEnd);
    if(secondEnd==n){
        if(spans[a].at==spans[b].at)return spans[a].count<spans[b].count?b:a;
        if(end(a)==end(b))return a;
        if(spans[b].at==last(a))return spans[b].weight>spans[a].weight?b:a;
        return a;
    }
    size_t thirdEnd=secondEnd+1; while(thirdEnd<n && same(secondEnd,thirdEnd))++thirdEnd;
    size_t c=strongest(secondEnd,thirdEnd);
    if(spans[a].at==spans[b].at && spans[b].count<spans[a].count)std::swap(a,b);
    if(end(a)>spans[c].at){
        if(last(a)==spans[b].at && last(a)==spans[c].at){
            auto rank=[&](size_t i){return spans[i].weight*4+int(spans[i].count);};
            size_t pick=rank(a)<rank(b)?b:a;
            if(rank(pick)<rank(c))pick=c;
            if(pick!=a && spans[b].count>spans[c].count)pick=b;
            if(spans[b].count<spans[c].count)pick=c;
            return pick;
        }
        if(spans[c].at==spans[a].at)return spans[c].count>spans[b].count?c:b;
        return spans[b].at==spans[a].at?b:a;
    }
    bool contested=false;
    if(spans[a].at==spans[b].at || last(a)==spans[b].at){
        if(spans[a].weight+displaceMargin>spans[b].weight)return a;
        contested=true;
    }
    if(spans[c].at==last(b) || end(c)==end(b)){
        if(spans[c].weight+displaceMargin>spans[b].weight)return a;
        contested=true;
    }
    return contested?b:a;
}
inline Text wide(const std::string& bytes) {
    int length = MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, bytes.data(), static_cast<int>(bytes.size()), nullptr, 0);
    Text result(length, L'\0');
    if (length) MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, bytes.data(), static_cast<int>(bytes.size()), &result[0], length);
    return result;
}
inline Text folder(HMODULE module) {
    wchar_t path[32768];
    DWORD len = GetModuleFileNameW(module, path, 32768);
    return Text(path, len).substr(0, Text(path, len).find_last_of(L"\\/") + 1);
}
// Which part of a syllable a key fills: 0 initial, 1 medial, 2 final, 3 tone; -1 for no Zhuyin key.
inline int syllablePart(wchar_t key){
    static constexpr wchar_t order[]=L"1qaz2wsxedcrfv5tgbyhnujm8ik,9ol.0p;/-7634";
    const wchar_t* at=key?std::wcschr(order,key):nullptr;
    if(!at)return -1;
    const size_t i=size_t(at-order);
    return i<21?0:i<24?1:i<37?2:3;
}
// The keys of one syllable in reading order, initial, medial, final then tone, as the word list spells
// them; empty when two keys fill the same part.
inline Text syllableOrder(const Text& keys){
    Text parts[4];
    for(const wchar_t key:keys){
        const int at=syllablePart(key);
        if(at<0 || !parts[at].empty())return {};
        parts[at]=Text(1,key);
    }
    return parts[0]+parts[1]+parts[2]+parts[3];
}
struct Dictionary {
    std::map<wchar_t, Text> keys;
    std::map<Text, std::vector<Text>> byCode;
    std::map<Text, std::vector<Term>> phrases;
    // How common each character is under each of its readings.
    std::map<std::pair<Text,Text>,double> charScore;
    size_t longestPhrase = 1;
    // Every set of keys some syllable contains, in reading order: the keys typed so far can still
    // become a syllable, whatever order the rest arrive in, when they are one of these.
    std::unordered_set<Text> shapes;
    bool reachable(const Text& ordered) const {return !ordered.empty() && shapes.count(ordered)!=0;}
    struct Applied { Text reading, text; std::optional<double> previous; };
    std::vector<Applied> userLayer;
    // Words that occur together more often than chance: for each second word, the first words that favour it
    // and by how many powers of ten. Words are kept as hashes; the table is loaded into every application.
    using Followers=std::vector<std::pair<unsigned long long,float>>;
    std::unordered_map<unsigned long long,Followers> pairs;
    static unsigned long long wordHash(const Text& word){
        unsigned long long hash=1469598103934665603ull;
        for(wchar_t unit:word){hash^=unit;hash*=1099511628211ull;}
        return hash;
    }
    // "first<TAB>second<TAB>bonus" per line.
    void loadPairs(const Text& file) {
        std::ifstream in(file, std::ios::binary);
        std::string line;
        while (std::getline(in, line)) {
            if (!line.empty() && line.back() == '\r') line.pop_back();
            const auto one = line.find('\t'), two = one == std::string::npos ? one : line.find('\t', one + 1);
            if (two == std::string::npos) continue;
            const float bonus = std::strtof(line.c_str() + two + 1, nullptr);
            if (bonus <= 0) continue;
            pairs[wordHash(wide(line.substr(one + 1, two - one - 1)))].push_back({wordHash(wide(line.substr(0, one))), bonus});
        }
        for (auto& [second, firsts] : pairs) std::sort(firsts.begin(), firsts.end());
    }
    const Followers* followers(unsigned long long second) const {
        auto found=pairs.find(second);
        return found==pairs.end()?nullptr:&found->second;
    }
    // A character the word list does not score ranks below every one it does.
    double scoreOf(const Text& reading,const Text& text) const {
        auto found=charScore.find({reading,text});
        return found==charScore.end()?-12.0:found->second;
    }
    bool load(const Text& dir) {
        std::ifstream cin(dir + L"Tingzi.cin", std::ios::binary);
        std::string line, key, value;
        enum { none, names, entries } section = none;
        while (std::getline(cin, line)) {
            if (!line.empty() && line.back() == '\r') line.pop_back();
            if (line == "%keyname begin") { section = names; continue; }
            if (line == "%chardef begin") { section = entries; continue; }
            if (line.empty() || line.front() == '%') { section = none; continue; }
            std::istringstream row(line);
            if (!(row >> key >> value)) continue;
            if (section == names && key.size() == 1) keys[key[0]] = wide(value);
            if (section == entries) byCode[wide(key)].push_back(wide(value));
        }
        std::ifstream phraseFile(dir + L"phrases.txt", std::ios::binary);
        double score;
        while (std::getline(phraseFile, line)) {
            if (line.empty() || static_cast<unsigned char>(line.front()) < 0x80) continue;
            std::istringstream row(line);
            if (!(row >> key >> value >> score)) continue;
            if(key.find('-')==std::string::npos){
                auto [entry,inserted]=charScore.emplace(std::make_pair(wide(key),wide(value)),score);
                if(!inserted)entry->second=(std::max)(entry->second,score);
                continue;
            }
            size_t count = 1 + std::count(key.begin(), key.end(), '-');
            longestPhrase = (std::max)(longestPhrase, count);
            phrases[wide(key)].push_back({wide(value), score});
        }
        for (auto& bucket : phrases) std::stable_sort(bucket.second.begin(), bucket.second.end(), [](const Term& a, const Term& b){ return a.score > b.score; });
        // Characters with the same sound are offered most common first; the character table's own order breaks ties.
        for (auto& [code, characters] : byCode) {
            const Text sound = reading(code);
            std::stable_sort(characters.begin(), characters.end(), [&](const Text& a, const Text& b){ return scoreOf(sound, a) > scoreOf(sound, b); });
        }
        for (const auto& [code, characters] : byCode) {
            if (code.size() > 4 || syllableOrder(code) != code) continue;
            for (unsigned mask = 1; mask < (1u << code.size()); ++mask) {
                Text kept;
                for (size_t i = 0; i < code.size(); ++i) if (mask & (1u << i)) kept += code[i];
                shapes.insert(kept);
            }
        }
        return !byCode.empty() && !keys.empty();
    }
    struct UserWord { Text reading, text; double score; };
    // Same line format as phrases.txt; only multi-syllable words of one character per syllable.
    static std::vector<UserWord> readUser(const Text& file) {
        std::vector<UserWord> words;
        std::ifstream in(file, std::ios::binary);
        std::string line, key, value;
        while (std::getline(in, line)) {
            std::istringstream row(line);
            if (!(row >> key >> value) || key.find('-') == std::string::npos) continue;
            size_t count = 1 + std::count(key.begin(), key.end(), '-');
            Text reading = wide(key), text = wide(value);
            if (reading.empty() || text.size() != count) continue;
            words.push_back({reading, text, userPhraseScore});
        }
        return words;
    }
    void loadUser(const Text& file) { for (const auto& word : readUser(file)) addUser(word.reading, word.text, word.score); }
    static void insertTerm(std::vector<Term>& bucket, const Text& text, double score) {
        auto place = std::find_if(bucket.begin(), bucket.end(), [&](const Term& term){ return term.score < score; });
        bucket.insert(place, {text, score});
    }
    void addUser(const Text& reading, const Text& text, double score = userPhraseScore) {
        auto& bucket = phrases[reading];
        bucket.erase(std::remove_if(bucket.begin(), bucket.end(), [&](const Term& term){ return term.text == text; }), bucket.end());
        insertTerm(bucket, text, score);
        longestPhrase = (std::max)(longestPhrase, size_t(1 + std::count(reading.begin(), reading.end(), L'-')));
    }
    // "reading<TAB>text": typing the reading offers the text in the candidate list, never automatically.
    static std::vector<UserWord> readTemplates(const Text& file) {
        std::vector<UserWord> words;
        std::ifstream in(file, std::ios::binary);
        std::string line;
        while (std::getline(in, line)) {
            if (!line.empty() && line.back() == '\r') line.pop_back();
            auto tab = line.find('\t');
            if (tab == std::string::npos || line.find('-') > tab) continue;
            Text reading = wide(line.substr(0, tab)), text = wide(line.substr(tab + 1));
            if (!reading.empty() && !text.empty()) words.push_back({reading, text, templateScore});
        }
        return words;
    }
    void loadTemplates(const Text& file) { for (const auto& word : readTemplates(file)) addUser(word.reading, word.text, word.score); }
    // Replaces the words given here last time with these, restoring any bundled word one of them
    // displaced, so words the user adds or removes take effect in programs that are already open.
    void setUserWords(const std::vector<UserWord>& words) {
        for (auto it = userLayer.rbegin(); it != userLayer.rend(); ++it) {
            auto found = phrases.find(it->reading);
            if (found == phrases.end()) continue;
            auto& bucket = found->second;
            bucket.erase(std::remove_if(bucket.begin(), bucket.end(), [&](const Term& term){ return term.text == it->text; }), bucket.end());
            if (it->previous) insertTerm(bucket, it->text, *it->previous);
            if (bucket.empty()) phrases.erase(found);
        }
        userLayer.clear();
        for (const auto& word : words) {
            std::optional<double> previous;
            auto found = phrases.find(word.reading);
            if (found != phrases.end())
                for (const auto& term : found->second) if (term.text == word.text) { previous = term.score; break; }
            userLayer.push_back({word.reading, word.text, previous});
            addUser(word.reading, word.text, word.score);
        }
    }
    Text reading(const Text& code) const {
        Text result;
        for (auto ch : code) { auto it = keys.find(ch); if (it != keys.end()) result += it->second; }
        return result;
    }
};
inline std::string narrow(const Text& text) {
    int length = WideCharToMultiByte(CP_UTF8, 0, text.data(), static_cast<int>(text.size()), nullptr, 0, nullptr, nullptr);
    std::string result(length, '\0');
    if (length) WideCharToMultiByte(CP_UTF8, 0, text.data(), static_cast<int>(text.size()), &result[0], length, nullptr, nullptr);
    return result;
}
inline Text userPhraseFile() { return localLearningDir() + L"\\user-phrases.txt"; }
inline Text userTemplateFile() { return localLearningDir() + L"\\user-templates.txt"; }
inline bool appendUserPhrase(const Text& reading, const Text& text) {
    std::error_code error;
    std::filesystem::create_directories(localLearningDir(), error);
    std::ofstream out(userPhraseFile(), std::ios::binary | std::ios::app);
    out << narrow(reading) << ' ' << narrow(text) << " 0\n";
    return bool(out);
}
// Set on a key pressed while only Shift is held.
inline constexpr WPARAM shiftKey=0x10000;
// Set on a key that carries one finished character, typed while full-width letters are on.
inline constexpr WPARAM literalKey=0x20000;
// Set on a digit pressed while only Ctrl is held: keep that many characters before the caret as a word.
inline constexpr WPARAM controlKey=0x40000;
// First character of a pseudo-syllable that carries a symbol through the composition.
inline constexpr wchar_t symbolMark=L'\x01';
// The symbol list, by kind. The first group opens with the list; Tab moves to the next.
struct SymbolGroup { const wchar_t* name; std::vector<const wchar_t*> marks; };
inline const SymbolGroup symbolGroups[]={
    {L"常用",{L"，",L"。",L"、",L"？",L"！",L"：",L"；",L"「",L"」",L"『",L"』",L"（",L"）",L"……",L"——",L"～",L"・",L"《",L"》",L"〈",L"〉",L"【",L"】",L"＄",L"％",L"＆",L"＠"}},
    {L"括號",{L"「",L"」",L"『",L"』",L"（",L"）",L"《",L"》",L"〈",L"〉",L"【",L"】",L"〔",L"〕",L"｛",L"｝",L"［",L"］",L"“",L"”",L"‘",L"’"}},
    {L"箭頭",{L"→",L"←",L"↑",L"↓",L"↔",L"⇒",L"⇐",L"⇔",L"↗",L"↘",L"↙",L"↖",L"▶",L"◀",L"▲",L"▼"}},
    {L"數學",{L"＋",L"－",L"×",L"÷",L"＝",L"≠",L"≒",L"＜",L"＞",L"≦",L"≧",L"±",L"√",L"∞",L"％",L"‰",L"°",L"π",L"∑",L"∴",L"∵"}},
    {L"單位",{L"℃",L"℉",L"㎝",L"㎜",L"㎞",L"㎡",L"㎏",L"㎎",L"㏄",L"＄",L"￥",L"€",L"£",L"＃",L"＠",L"＆",L"＊"}},
    {L"編號",{L"①",L"②",L"③",L"④",L"⑤",L"⑥",L"⑦",L"⑧",L"⑨",L"⑩",L"Ⅰ",L"Ⅱ",L"Ⅲ",L"Ⅳ",L"Ⅴ",L"Ⅵ",L"Ⅶ",L"Ⅷ",L"Ⅸ",L"Ⅹ",L"㈠",L"㈡",L"㈢",L"㈣",L"㈤"}},
    {L"圖形",{L"○",L"●",L"◎",L"△",L"▲",L"□",L"■",L"◇",L"◆",L"☆",L"★",L"※",L"§",L"♪",L"♥",L"✓",L"✗",L"…",L"─",L"│"}},
    {L"注音",{L"ㄅ",L"ㄆ",L"ㄇ",L"ㄈ",L"ㄉ",L"ㄊ",L"ㄋ",L"ㄌ",L"ㄍ",L"ㄎ",L"ㄏ",L"ㄐ",L"ㄑ",L"ㄒ",L"ㄓ",L"ㄔ",L"ㄕ",L"ㄖ",L"ㄗ",L"ㄘ",L"ㄙ",L"ㄧ",L"ㄨ",L"ㄩ",L"ㄚ",L"ㄛ",L"ㄜ",L"ㄝ",L"ㄞ",L"ㄟ",L"ㄠ",L"ㄡ",L"ㄢ",L"ㄣ",L"ㄤ",L"ㄥ",L"ㄦ",L"ˊ",L"ˇ",L"ˋ",L"˙"}},
};
inline wchar_t keyChar(WPARAM key) {
    if (key >= 'A' && key <= 'Z') return wchar_t(key + 32);
    if (key >= '0' && key <= '9') return wchar_t(key);
    switch (key) {
    case VK_OEM_COMMA: return L','; case VK_OEM_PERIOD: return L'.';
    case VK_OEM_1: return L';'; case VK_OEM_2: return L'/'; case VK_OEM_MINUS: return L'-';
    case VK_OEM_7: return L'\''; case VK_OEM_PLUS: return L'=';
    default: return 0;
    }
}
// The key as the word list spells it, or 0 when the layout has no symbol on that key.
inline wchar_t codeKey(WPARAM key,KeyLayout layout=KeyLayout::Standard) {
    const wchar_t ch=keyChar(key);
    if(layout==KeyLayout::Standard || !ch)return ch;
    static constexpr wchar_t eten[]  =L"bpmfdtnlvkhg7c,./j;'sexuaorwiqzy890-=1234";
    static constexpr wchar_t daqian[]=L"1qaz2wsxedcrfv5tgbyhnujm8ik,9ol.0p;/-7634";
    const wchar_t* at=std::wcschr(eten,ch);
    return at?daqian[at-eten]:0;
}
// Full-width punctuation for a key, or empty when the key has none in this layout.
inline Text punctuation(WPARAM key,bool shifted,KeyLayout layout) {
    if(shifted){
        switch(key){
        case VK_OEM_COMMA: return L"，"; case VK_OEM_PERIOD: return L"。"; case VK_OEM_2: return L"？"; case VK_OEM_1: return L"：";
        case VK_OEM_7: return L"；"; case '1': return L"！"; case '9': return L"（"; case '0': return L"）";
        case VK_OEM_4: return L"『"; case VK_OEM_6: return L"』"; case '6': return L"……"; case VK_OEM_MINUS: return L"——";
        case VK_OEM_3: return L"～"; case VK_OEM_5: return L"・";
        default: return {};
        }
    }
    switch(key){
    case VK_OEM_4: return L"「"; case VK_OEM_6: return L"」"; case VK_OEM_5: return L"、";
    case VK_OEM_7: return layout==KeyLayout::Standard?Text(L"、"):Text();
    default: return {};
    }
}
class Input {
    // A stretch of the sentence the user has settled: `count` syllables from `at`, shown as `text`.
    // Settled stretches may sit anywhere; everything between them is converted automatically.
    struct Segment { size_t at; Text text; size_t count; std::optional<Correction> correction; bool picked=false; };
    // One piece of the converted sentence.
    struct Span { Text text; size_t count; };
    const Dictionary& dict;
    std::vector<Text> syllables;
    std::vector<Segment> settled; // ordered by position, never overlapping
    Text raw;
    bool picking = false;
    size_t page = 0;
    size_t highlighted=0;
    bool nearMode=false, symbolMode=false;
    // True while the list offers sentences typed before, recalled from the first key of each syllable.
    bool recallMode=false;
    std::shared_ptr<const std::vector<Sentence>> sentences;
    std::optional<Sentence> committed;
    size_t symbolGroup=0;
    // The syllable whose character is being chosen while the list is open.
    size_t target=0;
    // True when the user moved the caret to reach the target, rather than choosing from the left.
    bool targetByCaret=false;
    // Where typing and deleting happen, counted in syllables. Unset means the end of the sentence.
    std::optional<size_t> caret;
    // Longest first word allowed after Tab forced a break; 0 leaves it to the segmenter.
    size_t firstLimit=0;
    std::optional<NewWord> added;
    std::optional<NewWord> typed;
    // How many syllables at the end were ended by the next key rather than by Space or a tone, so
    // later keys may still show they belong together. `loose` is the count the current key started with.
    size_t implied=0, loose=0;
    // The syllable before, as it was before it took the key typed after its tone, and that key; set
    // while the keys after it may still show the key began the next syllable. `owed` is `lent` as the
    // current key found it.
    struct Loan { Text syllable; wchar_t key=0; };
    std::optional<Loan> lent, owed;
    static bool symbol(const Text& syllable){return !syllable.empty() && syllable[0]==symbolMark;}
    void leavePicking(){picking=false;page=0;highlighted=0;nearMode=symbolMode=recallMode=false;symbolGroup=0;}
    // Earlier sentences whose syllables begin with the keys typed so far, most recent first.
    std::vector<Sentence> recalled() const {
        std::vector<Sentence> found;
        if(sentences && syllables.empty() && raw.size()>=2)
            for(const auto& said:*sentences)if(said.codes.rfind(raw,0)==0)found.push_back(said);
        return found;
    }
    Text scripted(const Text& text) const {return simplifiedOutput.value_or(configuredSimplifiedOutput())?convertScript(text,true):text;}
    std::shared_ptr<const Preferences> preferences;
    std::shared_ptr<const Favorites> favorites;
    Segmenter segmenter=configuredSegmenter();
    KeyLayout layout=configuredLayout();
    std::optional<bool> simplifiedOutput;
    static Text context(const Text& prefix){
        auto i=prefix.size()>2?prefix.size()-2:0;
        if(i && prefix[i]>=0xdc00 && prefix[i]<=0xdfff)--i;
        return prefix.substr(i);
    }
    // The key a correction is remembered under: the two characters before it, or a mark for the start of the sentence.
    static Text place(const Text& left){return left.empty()?Text(L"^"):context(left);}
    Text preferred(const Text& left,size_t pos) const {
        if(!preferences)return {};
        auto it=preferences->find({place(left),dict.reading(syllables[pos])});
        return it==preferences->end()?Text():it->second;
    }
    // What the user picked for this syllable before, in any sentence, the latest pick first.
    const std::vector<std::pair<Text,int>>* favored(size_t pos) const {
        if(!favorites || pos>=syllables.size() || symbol(syllables[pos]))return nullptr;
        auto found=favorites->find(dict.reading(syllables[pos]));
        return found==favorites->end()?nullptr:&found->second;
    }
    // Moves the characters the user picked before for this sound, latest first, to `from`, ahead of
    // every other entry after it, so they are on the first page whatever words follow.
    void favor(std::vector<Choice>& list,size_t pos,size_t from) const {
        const auto* picks=favored(pos);
        if(!picks || from>list.size())return;
        std::vector<Choice> front;
        for(const auto& [text,count]:*picks){
            (void)count;
            auto at=std::find_if(list.begin()+from,list.end(),[&](const Choice& c){return c.count==1 && c.text==text;});
            if(at!=list.end()){front.push_back(*at);list.erase(at);}
        }
        list.insert(list.begin()+from,front.begin(),front.end());
    }
    const Segment* owner(size_t pos) const {
        for(const auto& seg:settled)if(pos>=seg.at && pos<seg.at+seg.count)return &seg;
        return nullptr;
    }
    // The first syllable the user has not settled; choosing from the left starts here.
    size_t firstFree() const {
        size_t pos=0;
        for(const auto& seg:settled){if(seg.at!=pos)break;pos+=seg.count;}
        return pos;
    }
    // Where the next settled stretch at or after `pos` begins.
    size_t nextSettled(size_t pos) const {
        for(const auto& seg:settled)if(seg.at>=pos)return seg.at;
        return syllables.size();
    }
    void place(Segment seg){
        settled.erase(std::remove_if(settled.begin(),settled.end(),[&](const Segment& old){return old.at<seg.at+seg.count && seg.at<old.at+old.count;}),settled.end());
        settled.insert(std::find_if(settled.begin(),settled.end(),[&](const Segment& old){return old.at>seg.at;}),std::move(seg));
    }
    // Breaks the stretch covering `pos` into single characters so one of them can change.
    // A stretch whose text is not one character per syllable, such as a template, is released instead.
    void split(size_t pos){
        auto it=std::find_if(settled.begin(),settled.end(),[&](const Segment& seg){return pos>=seg.at && pos<seg.at+seg.count;});
        if(it==settled.end() || it->count==1)return;
        const Segment whole=*it;
        settled.erase(it);
        if(whole.text.size()!=whole.count)return;
        for(size_t i=0;i<whole.count;++i)place({whole.at+i,whole.text.substr(i,1),1,{},whole.picked});
    }
    void insertSyllable(Text syllable){
        const size_t at=caret.value_or(syllables.size());
        if(at<syllables.size() && owner(at) && owner(at)->at!=at)split(at);
        if(const Segment* inside=owner(at);inside && inside->at!=at)return;
        for(auto& seg:settled)if(seg.at>=at)++seg.at;
        syllables.insert(syllables.begin()+at,std::move(syllable));
        if(caret)caret=at+1;
    }
    void removeSyllable(size_t at){
        split(at);
        settled.erase(std::remove_if(settled.begin(),settled.end(),[&](const Segment& seg){return seg.at==at;}),settled.end());
        for(auto& seg:settled)if(seg.at>at)--seg.at;
        syllables.erase(syllables.begin()+at);
        if(caret && *caret>=syllables.size())caret.reset();
    }
    std::vector<Choice> words(size_t pos) const {
        std::vector<Choice> result;
        if(symbol(syllables[pos]))return {{syllables[pos].substr(1),1,0.0}};
        Text reading;
        for(size_t end=pos; end<syllables.size() && end-pos<dict.longestPhrase; ++end) {
            if(symbol(syllables[end]))break;
            if (end!=pos) reading+=L"-";
            reading+=dict.reading(syllables[end]);
            if(end==pos) {
                auto it=dict.byCode.find(syllables[pos]);
                if(it!=dict.byCode.end()) for(const auto& text:it->second) result.push_back({text,1,dict.scoreOf(reading,text)});
            } else {
                auto it=dict.phrases.find(reading);
                if(it!=dict.phrases.end()) for(const auto& term:it->second) result.push_back({term.text,end-pos+1,term.score});
            }
        }
        return result;
    }
    // Words that may start at a free syllable: none may run into a settled stretch.
    std::vector<Choice> wordsFor(size_t pos) const {
        auto list=words(pos);
        const size_t room=nextSettled(pos)-pos;
        const size_t limit=(firstLimit && pos==firstFree())?(std::min)(firstLimit,room):room;
        list.erase(std::remove_if(list.begin(),list.end(),[&](const Choice& word){return word.count>limit;}),list.end());
        return list;
    }
    // The most likely sentence from `pos` on. Each word counts for how common it is, plus a bonus when the
    // word before it is one it often follows, so the choice of a word depends on its neighbour.
    std::vector<Span> highestPath(size_t pos,size_t* firstLength=nullptr) const {
        const size_t len=syllables.size();
        struct Edge { size_t count; Text text; double best; int back; unsigned long long hash; };
        std::vector<Edge> edges;
        // For each position, the words that end there, and the best of them.
        std::vector<std::vector<int>> ending(len+1);
        std::vector<int> strongest(len+1,-1);
        std::vector<char> reached(len+1,0);
        if(pos<=len)reached[pos]=1;
        for(size_t at=pos;at<len;++at){
            if(!reached[at])continue;
            std::vector<Choice> options;
            if(const Segment* seg=owner(at)){
                // A settled stretch is taken whole, exactly as the user left it.
                const bool head=seg->at==at;
                options.push_back({head?seg->text:Text(),head?seg->count:1,0.0});
            } else options=wordsFor(at);
            const auto& before=ending[at];
            for(auto& term:options){
                const unsigned long long hash=Dictionary::wordHash(term.text);
                double lead=0;int back=-1;
                if(!before.empty()){
                    back=strongest[at];lead=edges[back].best;
                    if(const auto* favoured=dict.followers(hash)){
                        for(int candidate:before){
                            const auto found=std::lower_bound(favoured->begin(),favoured->end(),std::make_pair(edges[candidate].hash,0.0f));
                            if(found==favoured->end() || found->first!=edges[candidate].hash)continue;
                            const double total=edges[candidate].best+found->second;
                            if(total>lead){lead=total;back=candidate;}
                        }
                    }
                }
                const size_t next=at+term.count;
                const int index=int(edges.size());
                edges.push_back({term.count,std::move(term.text),term.score+lead,back,hash});
                ending[next].push_back(index);
                if(strongest[next]<0 || edges[index].best>edges[strongest[next]].best)strongest[next]=index;
                reached[next]=1;
            }
        }
        std::vector<Span> spans;
        for(int at=strongest[len];at>=0;at=edges[at].back)spans.push_back({edges[at].text,edges[at].count});
        std::reverse(spans.begin(),spans.end());
        if(firstLength)*firstLength=spans.empty()?0:spans.front().count;
        return spans;
    }
    // Syllables that sound close: the usual initial and final confusions, in every tone.
    std::vector<Text> nearCodes(const Text& code) const {
        Text base=code; wchar_t tone=0;
        if(!base.empty() && std::wcschr(L"3467",base.back())){tone=base.back();base.pop_back();}
        std::vector<Text> bases{base};
        static constexpr const wchar_t* pairs[]={L"5y",L"th",L"gn",L"sx",L"zc",L"bx",L"p/",L"0;"};
        for(size_t p=0;p<8 && !base.empty();++p) for(int d=0;d<2;++d){
            Text variant=base;
            wchar_t& slot=p<6?variant.front():variant.back();
            if(slot==pairs[p][d]){slot=pairs[p][1-d];bases.push_back(variant);}
        }
        std::vector<Text> codes;
        for(const auto& stem:bases) for(wchar_t mark:{tone,L'\0',L'6',L'3',L'4',L'7'}){
            Text candidate=mark?stem+mark:stem;
            if(candidate!=code && std::find(codes.begin(),codes.end(),candidate)==codes.end())codes.push_back(candidate);
        }
        // A key touching the intended one on the standard layout, keeping the tone, when that spells a syllable.
        // Each row sits half a key to the right of the one above it.
        static const Text rows[]={L"1234567890-",L"qwertyuiop",L"asdfghjkl;",L"zxcvbnm,./"};
        for(size_t i=0;i<base.size();++i) for(int r=0;r<4;++r){
            const size_t c=rows[r].find(base[i]);
            if(c==Text::npos)continue;
            const std::pair<int,int> touching[]={{r,int(c)-1},{r,int(c)+1},{r-1,int(c)},{r-1,int(c)+1},{r+1,int(c)},{r+1,int(c)-1}};
            for(auto [row,column]:touching){
                if(row<0 || row>3 || column<0 || column>=int(rows[row].size()))continue;
                Text candidate=base;candidate[i]=rows[row][size_t(column)];
                if(tone)candidate+=tone;
                if(candidate!=code && dict.byCode.count(candidate) && std::find(codes.begin(),codes.end(),candidate)==codes.end())codes.push_back(candidate);
            }
        }
        return codes;
    }
    // Phrase scores are log10 frequencies from about -8 to -2; eight steps per unit keeps
    // the displacement margin near a factor of two and a half in frequency.
    static int spanWeight(double score){return int(std::lround((score+8.0)*8.0));}
    std::vector<Span> staged(size_t pos) const {
        const size_t len=syllables.size();
        if(pos>=len)return {};
        std::vector<Text> placed(len);
        std::vector<char> taken(len,0);
        std::vector<std::vector<Choice>> options(len);
        for(size_t at=pos;at<len;++at){if(owner(at))taken[at]=1;else options[at]=wordsFor(at);}
        auto open=[&](size_t at,size_t count){
            for(size_t i=at;i<at+count;++i)if(taken[i])return false;
            return true;
        };
        auto settle=[&](size_t shortest,size_t longest){
            size_t cursor=pos;
            for(;;){
                std::vector<WordSpan> run;
                size_t covered=cursor;
                for(size_t at=cursor;at<len && run.size()<40;++at){
                    if(at>=covered && !run.empty())break;
                    for(size_t count=longest;count>=shortest;--count)
                        for(const auto& word:options[at])
                            if(word.count==count && word.text.size()==count && open(at,count)){
                                run.push_back({at,count,spanWeight(word.score),word.text});
                                covered=(std::max)(covered,at+count);
                            }
                }
                if(run.empty())return;
                const auto& word=run[chooseSpan(run)];
                for(size_t i=0;i<word.count;++i){taken[word.at+i]=1;placed[word.at+i]=word.text.substr(i,1);}
                cursor=word.at+word.count;
            }
        };
        settle(4,(std::max)(size_t(4),dict.longestPhrase));
        settle(2,3);
        settle(1,1);
        std::vector<Span> result;
        for(size_t at=pos;at<len;++at){
            if(const Segment* seg=owner(at)){
                if(seg->at==at)result.push_back({seg->text,seg->count});
                continue;
            }
            if(placed[at].empty())return highestPath(pos);
            result.push_back({placed[at],1});
        }
        return result;
    }
    // The sentence from `pos` on, as pieces. `pos` must not fall inside a settled stretch.
    std::vector<Span> convert(size_t pos) const {
        auto spans=(segmenter==Segmenter::Staged && !firstLimit)?staged(pos):highestPath(pos);
        if(preferences && !preferences->empty()){
            Text left=leftOf(pos,false);
            size_t at=pos;
            for(auto& span:spans){
                if(!owner(at) && span.text.size()==span.count){
                    for(size_t i=0;i<span.count;++i){
                        // At the very start there is no context to go by, so a remembered choice only replaces
                        // a character standing alone, never the first character of a word.
                        if(left.empty() && span.count>1){left+=span.text[i];continue;}
                        auto choice=preferred(left,at+i);
                        auto candidates=dict.byCode.find(syllables[at+i]);
                        if(choice.size()==1 && candidates!=dict.byCode.end() &&
                            std::find(candidates->second.begin(),candidates->second.end(),choice)!=candidates->second.end())span.text[i]=choice[0];
                        left+=span.text[i];
                    }
                } else left+=span.text;
                at+=span.count;
            }
        }
        return spans;
    }
    static Text join(const std::vector<Span>& spans){Text text;for(const auto& span:spans)text+=span.text;return text;}
    Text best(size_t pos) const {return join(convert(pos));}
    // The whole sentence cut at a syllable: the part before it when `tail` is false, else the part from it on.
    // A piece that is not one character per syllable stays whole, on the side where it starts.
    Text cut(size_t pos,bool tail,bool withPreferences=true) const {
        Text before,after;size_t at=0;
        for(const auto& span:(withPreferences?convert(0):highestPath(0))){
            if(at+span.count<=pos)before+=span.text;
            else if(at>=pos)after+=span.text;
            else if(span.text.size()==span.count){before+=span.text.substr(0,pos-at);after+=span.text.substr(pos-at);}
            else after+=span.text;
            at+=span.count;
        }
        return tail?after:before;
    }
    // Text to the left of a syllable. Preferences read their own left context, so they ask without them.
    Text leftOf(size_t pos,bool withPreferences=true) const {return pos?cut(pos,false,withPreferences):Text();}
    static bool toneOf(wchar_t key){return key==L'3'||key==L'4'||key==L'6'||key==L'7';}
    // True when the keys spell a syllable someone would say. A lone initial or tone is in the character
    // table only as its own symbol, except ㄓㄔㄕㄖㄗㄘㄙ, which are syllables by themselves.
    bool spoken(const Text& code) const {
        if(!dict.byCode.count(code))return false;
        if(code.size()!=1)return true;
        const int at=syllablePart(code[0]);
        return at==1 || at==2 || (at==0 && std::wcschr(L"5tgbyhn",code[0]));
    }
    // The `count` syllables before the caret were ended only because the next key could not join them.
    // When they and `keys` together are the keys of one syllable, in any order, returns that syllable's keys.
    Text takenBack(size_t count,const Text& keys) const {
        const size_t end=caret.value_or(syllables.size());
        if(count==0 || count>end)return {};
        Text all;
        for(size_t at=end-count;at<end;++at){
            if(symbol(syllables[at]) || owner(at))return {};
            all+=syllables[at];
        }
        return syllableOrder(all+keys);
    }
    void takeBack(size_t count,const Text& joined){
        for(size_t i=0;i<count;++i){
            const size_t gone=caret.value_or(syllables.size())-1;
            if(caret)caret=gone;
            removeSyllable(gone);
        }
        raw=joined;
    }
    // True when some syllable is spelled by keys that begin with `keys`.
    bool startsCode(const Text& keys) const {
        auto at=dict.byCode.lower_bound(keys);
        return at!=dict.byCode.end() && at->first.compare(0,keys.size(),keys)==0;
    }
    // True when `keys`, typed at the start, may be the first keys of an earlier sentence.
    bool recallsFrom(const Text& keys) const {
        if(!sentences || !syllables.empty())return false;
        return std::any_of(sentences->begin(),sentences->end(),[&](const Sentence& said){return said.codes.rfind(keys,0)==0;});
    }
    // A single key left over after a toned syllable, and followed straight away by the next syllable,
    // is the last key of that syllable typed after its tone: ㄨˇ then ㄛ is ㄨㄛˇ. Typed with its own
    // Space or tone, the key stays a syllable of its own, as in ㄨˇ ㄢ.
    bool reattach() {
        if(raw.size()!=1 || caret || syllables.empty() || symbol(syllables.back()) || owner(syllables.size()-1))return false;
        const Text& last=syllables.back();
        const wchar_t tone=last.back();
        if(!(tone==L'3'||tone==L'4'||tone==L'6'||tone==L'7'))return false;
        const Text joined=last.substr(0,last.size()-1)+raw+tone;
        if(!dict.byCode.count(joined))return false;
        lent=Loan{last,raw[0]};
        syllables.back()=joined;raw.clear();
        return true;
    }
    // With `settle` false, as when a tone has just been typed, keys that spell nothing yet do not ask
    // back a key given to the syllable before: the tone may have come a key early, and the next key tells.
    bool finishSyllable(bool settle=true) {
        if(raw.empty())return false;
        if(!spoken(raw)){
            // Keys typed out of order, or after a syllable that was ended too soon, are put in reading order.
            if(const Text ordered=syllableOrder(raw);ordered!=raw && spoken(ordered))raw=ordered;
            else for(size_t count=loose;count>0;--count)
                if(const Text joined=takenBack(count,raw);spoken(joined)){takeBack(count,joined);loose-=count;break;}
            // A key given to the syllable before goes to this one when only that makes it a syllable:
            // ㄋㄧˇ ㄠ ㄏ ˇ is ㄋㄧˇ ㄏㄠˇ, not ㄋㄧㄠˇ and a ㄏˇ that spells nothing.
            if(settle && !spoken(raw) && owed && !caret && !syllables.empty() && !owner(syllables.size()-1))
                if(const Text joined=syllableOrder(Text(1,owed->key)+raw);spoken(joined)){syllables.back()=owed->syllable;raw=joined;}
        }
        if(!dict.byCode.count(raw)) return false;
        insertSyllable(raw); raw.clear(); return true;
    }
    // Keeps the characters around a choice as they are shown, so fixing one character in the middle
    // of a word does not change its neighbours.
    void anchorAround(size_t at,size_t count){
        size_t pos=0;
        for(const auto& span:convert(0)){
            const size_t end=pos+span.count;
            if(pos<at+count && at<end && !owner(pos) && span.text.size()==span.count)
                for(size_t i=pos;i<end;++i)if(i<at || i>=at+count)place({i,span.text.substr(i-pos,1),1,{},false});
            pos=end;
        }
    }
    void openList(size_t pos,bool byCaret){
        split(pos);
        if(const Segment* seg=owner(pos))pos=seg->at;
        target=pos;targetByCaret=byCaret;page=0;highlighted=0;picking=true;
    }
public:
    explicit Input(const Dictionary& d):dict(d){}
    void setPreferences(std::shared_ptr<const Preferences> p){preferences=std::move(p);}
    void setFavorites(std::shared_ptr<const Favorites> f){favorites=std::move(f);}
    void setSentences(std::shared_ptr<const std::vector<Sentence>> list){sentences=std::move(list);}
    // The sentence the last key committed, for the caller to remember; unset for short or symbol-only text.
    std::optional<Sentence> committedSentence() const {return committed;}
    void setSegmenter(Segmenter mode){segmenter=mode;}
    void setLayout(KeyLayout keys){layout=keys;}
    // Unset follows the user setting each time text is committed.
    void setSimplifiedOutput(std::optional<bool> mode){simplifiedOutput=mode;}
    bool learningActive() const {return bool(preferences);}
    std::vector<Correction> corrections() const {
        std::vector<Correction> edits;for(const auto& seg:settled)if(seg.correction)edits.push_back(*seg.correction);
        return edits;
    }
    size_t currentPage() const { return page; }
    size_t currentIndex() const { return highlighted; }
    bool active() const { return !raw.empty() || !syllables.empty(); }
    bool selecting() const { return picking; }
    void clear(){ raw.clear(); syllables.clear(); settled.clear(); firstLimit=0; caret.reset(); leavePicking(); }
    Text preview() const {
        if(!caret)return best(0)+dict.reading(raw);
        return cut(*caret,false)+dict.reading(raw)+cut(*caret,true);
    }
    // How many characters of the preview come before the caret.
    size_t caretOffset() const {
        return (caret?cut(*caret,false).size():best(0).size())+dict.reading(raw).size();
    }
    // True when the caret sits inside the sentence rather than at its end.
    bool caretInside() const {return caret.has_value();}
    struct DisplayRuns { Text before, active, after; };
    DisplayRuns displayRuns() const {
        if (picking) {
            auto list=choices();
            if(recallMode && highlighted<list.size())return {{},list[highlighted].text,{}};
            if(symbolMode && highlighted<list.size()){
                const size_t at=caret.value_or(syllables.size());
                return {cut(at,false),list[highlighted].text,cut(at,true)};
            }
            if(highlighted<list.size()) {
                const auto& choice=list[highlighted];
                const size_t next=target+choice.count;
                const Segment* seg=next<syllables.size()?owner(next):nullptr;
                return {leftOf(target),choice.text,(seg && seg->at!=next)?cut(next,true):best(next)};
            }
        }
        if(caret)return {cut(*caret,false),dict.reading(raw),cut(*caret,true)};
        return {best(0),dict.reading(raw),{}};
    }
    std::vector<Choice> choices() const {return choices(true);}
    // With `favoredFirst` false, the order before the user's own picks are moved forward.
    std::vector<Choice> choices(bool favoredFirst) const {
        std::vector<Choice> result;
        if(recallMode){for(const auto& said:recalled())result.push_back({said.text,0});return result;}
        if(symbolMode){for(const wchar_t* mark:symbolGroups[symbolGroup].marks)result.push_back({mark,0});return result;}
        auto pos=target;
        if(pos>=syllables.size()) return result;
        if(nearMode){
            static const std::vector<Text> none;
            auto same=dict.byCode.find(syllables[pos]);
            const auto& exact=same==dict.byCode.end()?none:same->second;
            // A character that makes a word with the one shown beside it comes first; otherwise the more common one.
            const Text before=leftOf(pos),after=cut(pos+1,true);
            auto partOfWord=[&](const Text& code,const Text& text){
                double best=-100;
                auto look=[&](const Text& reading,const Text& word){
                    auto found=dict.phrases.find(reading);
                    if(found==dict.phrases.end())return;
                    for(const auto& term:found->second)if(term.text==word)best=(std::max)(best,term.score);
                };
                const Text sound=dict.reading(code);
                if(pos && !before.empty() && !symbol(syllables[pos-1]))look(dict.reading(syllables[pos-1])+L"-"+sound,before.substr(before.size()-1)+text);
                if(pos+1<syllables.size() && !after.empty() && !symbol(syllables[pos+1]))look(sound+L"-"+dict.reading(syllables[pos+1]),text+after.substr(0,1));
                return best;
            };
            for(const auto& code:nearCodes(syllables[pos])){
                auto it=dict.byCode.find(code);
                if(it==dict.byCode.end())continue;
                for(const auto& text:it->second)
                    if(std::find(exact.begin(),exact.end(),text)==exact.end() &&
                       std::none_of(result.begin(),result.end(),[&](const Choice& item){return item.text==text;})){
                        const double word=partOfWord(code,text);
                        result.push_back({text,1,word>-100?100+word:dict.scoreOf(dict.reading(code),text)});
                    }
            }
            std::stable_sort(result.begin(),result.end(),[](const Choice& a,const Choice& b){return a.score>b.score;});
            return result;
        }
        if(owner(pos)){
            // A settled character: offer the other characters with the same sound, the current one first.
            const Text current=owner(pos)->text;
            result.push_back({current,1});
            for(const auto& word:words(pos))
                if(word.count==1 && word.text!=current)result.push_back(word);
            if(favoredFirst)favor(result,pos,1);
            return result;
        }
        // Everything up to the next settled stretch can be taken at once.
        const size_t room=nextSettled(pos)-pos;
        Text whole;size_t taken=0;
        for(const auto& span:convert(pos)){if(taken>=room)break;whole+=span.text;taken+=span.count;}
        result.push_back({whole,room});
        const Text primary=result.front().text;
        if(primary.size()>1 && primary.size()==room){
            static const std::vector<Text> none;
            auto found=dict.byCode.find(syllables[pos+room-1]);
            const auto& last=found==dict.byCode.end()?none:found->second;
            for(size_t i=0;i<(std::min)(size_t(2),last.size());++i){
                if(last[i].size()==1 && primary.back()!=last[i][0])
                    result.push_back({primary.substr(0,primary.size()-1)+last[i],room});
            }
        }
        auto options=words(pos);
        options.erase(std::remove_if(options.begin(),options.end(),[&](const Choice& word){return word.count>room;}),options.end());
        std::stable_sort(options.begin(),options.end(),[](const Choice& a,const Choice& b){return a.count>b.count;});
        for(const auto& c:options) {
            if(std::none_of(result.begin(),result.end(),[&](const Choice& item){return c.count==item.count && c.text==item.text;})) result.push_back(c);
        }
        if(favoredFirst)favor(result,pos,1);
        return result;
    }
    Text menu() const {
        if(!picking) return {};
        auto list=choices(); Text text;
        for(size_t i=page*9;i<list.size() && i<page*9+9;++i)
            text+=std::to_wstring(i%9+1)+L" "+list[i].text+L"  ";
        if(list.size()>9) text+=L"  PgUp/PgDn";
        return text;
    }
    Text listTitle() const {
        if(recallMode)return L"打過的句子";
        if(symbolMode)return Text(L"符號・")+symbolGroups[symbolGroup].name+L"（Tab 換類別）";
        return nearMode?L"近音字":L"注音選字";
    }
    // Runs of characters the user picked one by one, which are not yet a word in the list.
    std::vector<NewWord> newWords() const {
        std::vector<NewWord> found;
        Text reading,text;size_t run=0,next=0;
        auto flush=[&]{
            if(run>=2 && run<=4){
                auto known=dict.phrases.find(reading);
                if(known==dict.phrases.end() || std::none_of(known->second.begin(),known->second.end(),[&](const Term& term){return term.text==text;}))
                    found.push_back({reading,text});
            }
            reading.clear();text.clear();run=0;
        };
        for(const auto& seg:settled){
            if(seg.at!=next)flush();
            if(seg.picked && seg.count==1 && seg.text.size()==1 && !symbol(syllables[seg.at])){
                if(run)reading+=L"-";
                reading+=dict.reading(syllables[seg.at]);text+=seg.text;++run;
            } else flush();
            next=seg.at+seg.count;
        }
        flush();
        return found;
    }
    // The word the last key asked to remember, when Shift+Enter committed the composition.
    std::optional<NewWord> addedWord() const {return added;}
    // What Enter just committed with its reading, before any conversion to Simplified; unset when
    // it held symbols or more than eight syllables.
    std::optional<NewWord> typedWord() const {return typed;}
    bool wants(WPARAM key) const {
        if(key&literalKey)return !active();
        if(key&controlKey){
            const WPARAM digit=key&0xffff;
            return !picking && raw.empty() && digit>='2' && digit<='8' && caret.value_or(syllables.size())>=size_t(digit-'0');
        }
        const bool shifted=(key&shiftKey)!=0; key&=0xffff;
        if(picking) return !shifted && ((key>='1' && key<='9') || key==VK_NEXT || key==VK_PRIOR || key==VK_UP || key==VK_DOWN || key==VK_LEFT || key==VK_RIGHT || key==VK_SPACE || key==VK_ESCAPE || key==VK_RETURN || key==VK_BACK || key==VK_TAB);
        if(shifted) return !punctuation(key,true,layout).empty() || (key==VK_RETURN && active());
        if(dict.keys.count(codeKey(key,layout))) return true;
        if(!punctuation(key,false,layout).empty() || key==VK_OEM_3) return true;
        return active() && (key==VK_BACK || key==VK_ESCAPE || key==VK_RETURN || key==VK_SPACE || key==VK_DOWN || key==VK_END || key==VK_TAB
            || key==VK_LEFT || key==VK_RIGHT || key==VK_HOME || key==VK_DELETE);
    }
    // Returns committed text separately from the remaining preedit.
    Text press(WPARAM key) {
        added.reset();committed.reset();typed.reset();
        loose=implied;implied=0;owed=lent;lent.reset();
        if(!wants(key)) return {};
        if(key&literalKey)return Text(1,wchar_t(key&0xffff));
        if(key&controlKey){
            // The characters before the caret, as they stand, become one of the user's own words.
            const size_t count=size_t((key&0xffff)-'0'),end=caret.value_or(syllables.size()),from=end-count;
            Text reading;
            for(size_t at=from;at<end;++at){
                if(symbol(syllables[at]))return {};
                if(at!=from)reading+=L"-";
                reading+=dict.reading(syllables[at]);
            }
            const Text text=cut(end,false).substr(cut(from,false).size());
            if(text.size()!=count)return {};
            place({from,text,count,{},true});
            added=NewWord{reading,text};
            return {};
        }
        const bool shifted=(key&shiftKey)!=0; key&=0xffff;
        if(key==VK_ESCAPE) { if(picking)leavePicking(); else clear(); return {}; }
        if(picking) {
            auto list=choices();
            if(list.empty()){leavePicking();return {};}
            if(key==VK_TAB){
                if(recallMode)return {};
                if(symbolMode){symbolGroup=(symbolGroup+1)%std::size(symbolGroups);page=0;highlighted=0;return {};}
                nearMode=!nearMode;page=0;highlighted=0;
                if(choices().empty())nearMode=false;
                return {};
            }
            if(key==VK_NEXT || key==VK_SPACE || key==VK_PRIOR){
                if(key==VK_PRIOR){if(page)--page;}else if((page+1)*9<list.size())++page;
                highlighted=page*9;return {};
            }
            if(key==VK_DOWN || key==VK_UP || key==VK_LEFT || key==VK_RIGHT){
                const int delta=(key==VK_DOWN || key==VK_RIGHT)?1:-1;
                highlighted=static_cast<size_t>(std::clamp<int>(int(highlighted)+delta,0,int(list.size()-1)));
                page=highlighted/9;return {};
            }
            if(key==VK_BACK){leavePicking(); return {};}
            size_t idx=key==VK_RETURN?highlighted:page*9+key-'1';
            if(idx<list.size()){
                const Choice picked=list[idx];
                if(recallMode){
                    const auto found=recalled();
                    clear();
                    // Choosing it again moves it back to the front of the recent sentences.
                    if(idx<found.size())committed=found[idx];
                    return scripted(picked.text);
                }
                if(symbolMode){
                    leavePicking();
                    if(!active())return picked.text;
                    if(syllables.size()<128)insertSyllable(Text(1,symbolMark)+picked.text);
                    return {};
                }
                const bool wasSettled=owner(target)!=nullptr;
                if(targetByCaret && !wasSettled)anchorAround(target,picked.count);
                if(nearMode){place({target,picked.text,1,{},false});leavePicking();firstLimit=0;return {};}
                std::optional<Correction> edit;
                const Text before=leftOf(target);
                auto left=place(before);
                // A pick counts against the order the dictionary offers, not one the user's earlier picks changed.
                const auto plain=favorites?choices(false):list;
                const auto firstSingle=wasSettled?plain.begin():std::find_if(plain.begin(),plain.end(),[](const Choice& item){return item.count==1;});
                // Picking another character than the one picked last time also counts, so the earlier pick is taken as a slip.
                const auto* picks=favored(target);
                const bool changed=(firstSingle!=plain.end() && firstSingle->text!=picked.text) || (picks && !picks->empty() && picks->front().first!=picked.text);
                if(preferences && !left.empty() && picked.count==1 && changed)
                    edit=Correction{{left,dict.reading(syllables[target])},picked.text};
                if(preferences && picked.count>1 && picked.text.size()==picked.count){
                    Text baseline=list.front().text.substr(0,picked.count);
                    size_t change=0,changes=0;
                    if(baseline.size()==picked.count){
                        for(size_t i=0;i<picked.count;++i)if(baseline[i]!=picked.text[i]){change=i;++changes;}
                        Text ahead=place(before+picked.text.substr(0,change));
                        auto valid=dict.byCode.find(syllables[target+change]);
                        if(changes==1 && !ahead.empty() && valid!=dict.byCode.end()){
                            Text selected=picked.text.substr(change,1);
                            if(std::find(valid->second.begin(),valid->second.end(),selected)!=valid->second.end())
                                edit=Correction{{ahead,dict.reading(syllables[target+change])},selected};
                        }
                    }
                }
                place({target,picked.text,picked.count,edit,true});leavePicking();firstLimit=0;
            }
            return {};
        }
        if(const Text mark=punctuation(key,shifted,layout);!mark.empty()){
            if(!raw.empty() && !finishSyllable())return {};
            if(!active())return mark;
            if(syllables.size()<128)insertSyllable(Text(1,symbolMark)+mark);
            return {};
        }
        if(key==VK_OEM_3){
            if(!raw.empty() && !finishSyllable())return {};
            symbolMode=true;symbolGroup=0;picking=true;page=0;highlighted=0;return {};
        }
        if(key==VK_LEFT || key==VK_RIGHT || key==VK_HOME){
            // The caret moves only between finished syllables.
            if(!raw.empty())return {};
            size_t at=caret.value_or(syllables.size());
            if(key==VK_HOME)at=0;
            else if(key==VK_LEFT){if(at)--at;}
            else if(at<syllables.size())++at;
            // A template is one piece; the caret steps over it rather than into it.
            if(const Segment* seg=owner(at);seg && seg->at!=at && seg->text.size()!=seg->count)at=key==VK_RIGHT?seg->at+seg->count:seg->at;
            if(at>=syllables.size())caret.reset();else caret=at;
            return {};
        }
        if(key==VK_DELETE){
            if(raw.empty() && caret && *caret<syllables.size()){firstLimit=0;removeSyllable(*caret);}
            return {};
        }
        if(key==VK_BACK){
            firstLimit=0;
            if(!raw.empty()) raw.pop_back();
            else if(caret){
                if(*caret){const size_t gone=*caret-1;caret=gone;removeSyllable(gone);}
            }
            else if(!syllables.empty()){
                // A settled last stretch is released first; another press removes the syllable.
                const Segment* last=owner(syllables.size()-1);
                if(last){const size_t at=last->at;settled.erase(std::remove_if(settled.begin(),settled.end(),[&](const Segment& seg){return seg.at==at;}),settled.end());}
                else syllables.pop_back();
            }
            return {};
        }
        if(key==VK_TAB){
            // Each press makes the first word one syllable shorter; after one syllable the limit lifts.
            if(!raw.empty() && !finishSyllable())return {};
            if(firstFree()>=syllables.size())return {};
            size_t current=0;highestPath(firstFree(),&current);
            firstLimit=current>1?current-1:0;
            return {};
        }
        if(key==VK_DOWN || key==VK_SPACE || key==VK_END) {
            if(!raw.empty()) {
                if(!finishSyllable()){
                    // Keys that spell no syllable may be the first key of each syllable of an earlier sentence.
                    if(!recalled().empty()){recallMode=true;picking=true;page=0;highlighted=0;}
                    return {};
                }
                if(key==VK_SPACE) return {};
            }
            if(syllables.empty())return {};
            // End reaches the last character; a moved caret chooses the character on its left;
            // otherwise choosing runs from the left, one unsettled word at a time.
            if(key==VK_END)openList(syllables.size()-1,true);
            else if(caret)openList(*caret?*caret-1:0,true);
            else if(firstFree()<syllables.size())openList(firstFree(),false);
            return {};
        }
        if(key==VK_RETURN) {
            if(!raw.empty() && !finishSyllable()) return {};
            Text result=best(0);
            std::optional<NewWord> keep,whole;
            if(!syllables.empty() && syllables.size()<=8 && result.size()==syllables.size()){
                Text reading;bool plain=true;
                for(const auto& syllable:syllables){
                    if(symbol(syllable)){plain=false;break;}
                    if(!reading.empty())reading+=L"-";
                    reading+=dict.reading(syllable);
                }
                if(plain)whole=NewWord{reading,result};
            }
            if(shifted && syllables.size()>=2 && syllables.size()<=8 && result.size()==syllables.size()){
                Text reading;bool plain=true;
                for(const auto& syllable:syllables){
                    if(symbol(syllable)){plain=false;break;}
                    if(!reading.empty())reading+=L"-";
                    reading+=dict.reading(syllable);
                }
                if(plain)keep=NewWord{reading,result};
            }
            // Sentences of three or more syllables can be recalled later from their first keys.
            Text codes;
            for(const auto& syllable:syllables)if(!symbol(syllable))codes+=syllable[0];
            std::optional<Sentence> said;
            if(codes.size()>=3 && codes.size()<=16)said=Sentence{codes,result};
            clear();added=keep;committed=said;typed=whole;
            return scripted(result);
        }
        wchar_t ch=codeKey(key,layout);
        const bool toneKey=ch==L'3'||ch==L'4'||ch==L'6'||ch==L'7';
        if(!raw.empty() && syllables.size()<128){
            // The keys of one syllable may come in any order: each takes its own place, initial, medial,
            // final, then tone. Keys that already spell a syllable are left as they are unless the new key
            // follows them in reading order, so a first-tone syllable typed without Space still ends there.
            if(!toneKey && syllableOrder(raw)==raw && !recallsFrom(raw+ch)){
                const int place=syllablePart(ch);
                const bool said=spoken(raw);
                bool filled=false,forward=true;
                for(const wchar_t k:raw){const int at=syllablePart(k);if(at==place)filled=true;if(at>place)forward=false;}
                const bool toned=toneOf(raw.back());
                auto keep=[&](const Text& keys){
                    raw=keys;implied=loose;lent=owed;
                    if(toned && dict.byCode.count(raw) && finishSyllable(false)){implied=0;lent.reset();}
                };
                if(!filled){
                    const Text joined=syllableOrder(raw+ch);
                    if(forward?(startsCode(joined) || (!said && dict.reachable(joined))):(!said && dict.reachable(joined))){keep(joined);return {};}
                } else if(!said && !toned){
                    // A second key for a part that is already filled replaces the first, as when the wrong
                    // initial was pressed: ㄉ then ㄊ is ㄊ.
                    Text changed=raw;
                    for(wchar_t& k:changed)if(syllablePart(k)==place)k=ch;
                    if(dict.reachable(changed)){keep(changed);return {};}
                }
                // Syllables ended only by the keys after them are taken back when this key shows they were
                // one syllable all along: ㄕ ㄛ then ㄨ is ㄕㄨㄛ.
                for(size_t count=loose;count>0;--count)
                    if(const Text joined=takenBack(count,raw+ch);dict.reachable(joined)){
                        takeBack(count,joined);implied=loose-count;
                        if(toneOf(raw.back()) && dict.byCode.count(raw) && finishSyllable(false))implied=0;
                        return {};
                    }
            }
            // A key that cannot continue the syllable starts the next one when the keys so far already
            // spell a first-tone syllable, as when Space was left out between two syllables.
            if(!toneKey && !startsCode(raw+ch) && !recallsFrom(raw+ch)){
                const size_t before=syllables.size();
                if(reattach() || finishSyllable()){
                    if(syllables.size()>before){implied=loose+1;lent.reset();}
                    raw=Text(1,ch);
                    return {};
                }
            }
        }
        // A syllable is at most a few keys; a run of first keys recalling a sentence may be longer.
        if(raw.size()>=(syllables.empty()?size_t(16):size_t(5)) || syllables.size()>=128) return {};
        raw+=ch;implied=loose;lent=owed;
        if(toneKey && finishSyllable(false)){implied=0;lent.reset();}
        return {};
    }
};
}
