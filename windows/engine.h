#pragma once
#include <windows.h>
#include <algorithm>
#include <cmath>
#include <cwchar>
#include <filesystem>
#include <fstream>
#include <map>
#include <sstream>
#include <string>
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
struct Dictionary {
    std::map<wchar_t, Text> keys;
    std::map<Text, std::vector<Text>> byCode;
    std::map<Text, std::vector<Term>> phrases;
    std::map<Text,double> singleScore;
    size_t longestPhrase = 1;
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
                auto [entry,inserted]=singleScore.emplace(wide(key),score);
                if(!inserted)entry->second=(std::max)(entry->second,score);
                continue;
            }
            size_t count = 1 + std::count(key.begin(), key.end(), '-');
            longestPhrase = (std::max)(longestPhrase, count);
            phrases[wide(key)].push_back({wide(value), score});
        }
        for (auto& bucket : phrases) std::stable_sort(bucket.second.begin(), bucket.second.end(), [](const Term& a, const Term& b){ return a.score > b.score; });
        return !byCode.empty() && !keys.empty();
    }
    // Same line format as phrases.txt; only multi-syllable words of one character per syllable.
    void loadUser(const Text& file) {
        std::ifstream in(file, std::ios::binary);
        std::string line, key, value;
        while (std::getline(in, line)) {
            std::istringstream row(line);
            if (!(row >> key >> value) || key.find('-') == std::string::npos) continue;
            size_t count = 1 + std::count(key.begin(), key.end(), '-');
            Text reading = wide(key), text = wide(value);
            if (reading.empty() || text.size() != count) continue;
            addUser(reading, text);
        }
    }
    void addUser(const Text& reading, const Text& text, double score = userPhraseScore) {
        auto& bucket = phrases[reading];
        bucket.erase(std::remove_if(bucket.begin(), bucket.end(), [&](const Term& term){ return term.text == text; }), bucket.end());
        auto place = std::find_if(bucket.begin(), bucket.end(), [&](const Term& term){ return term.score < score; });
        bucket.insert(place, {text, score});
        longestPhrase = (std::max)(longestPhrase, size_t(1 + std::count(reading.begin(), reading.end(), L'-')));
    }
    // "reading<TAB>text": typing the reading offers the text in the candidate list, never automatically.
    void loadTemplates(const Text& file) {
        std::ifstream in(file, std::ios::binary);
        std::string line;
        while (std::getline(in, line)) {
            if (!line.empty() && line.back() == '\r') line.pop_back();
            auto tab = line.find('\t');
            if (tab == std::string::npos || line.find('-') > tab) continue;
            Text reading = wide(line.substr(0, tab)), text = wide(line.substr(tab + 1));
            if (!reading.empty() && !text.empty()) addUser(reading, text, templateScore);
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
    static bool symbol(const Text& syllable){return !syllable.empty() && syllable[0]==symbolMark;}
    void leavePicking(){picking=false;page=0;highlighted=0;nearMode=symbolMode=false;symbolGroup=0;}
    std::shared_ptr<const Preferences> preferences;
    Segmenter segmenter=configuredSegmenter();
    KeyLayout layout=configuredLayout();
    std::optional<bool> simplifiedOutput;
    static Text context(const Text& prefix){
        auto i=prefix.size()>2?prefix.size()-2:0;
        if(i && prefix[i]>=0xdc00 && prefix[i]<=0xdfff)--i;
        return prefix.substr(i);
    }
    Text preferred(const Text& left,size_t pos) const {
        if(!preferences || left.empty())return {};
        auto it=preferences->find({context(left),dict.reading(syllables[pos])});
        return it==preferences->end()?Text():it->second;
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
                auto weight=dict.singleScore.find(reading);
                if(it!=dict.byCode.end()) for(const auto& text:it->second) result.push_back({text,1,weight==dict.singleScore.end()?-12.0:weight->second});
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
    std::vector<Span> highestPath(size_t pos,size_t* firstLength=nullptr) const {
        size_t len=syllables.size();
        std::vector<double> cost(len+1, -1e100);
        std::vector<Text> text(len+1);
        std::vector<size_t> step(len+1,0);
        cost[len]=0;
        for(size_t i=len;i>pos;--i){
            if(const Segment* seg=owner(i-1)){
                // A settled stretch is taken whole, exactly as the user left it.
                const bool head=seg->at==i-1;
                step[i-1]=head?seg->count:1;
                if(head)text[i-1]=seg->text;
                cost[i-1]=cost[i-1+step[i-1]];
                continue;
            }
            for(const auto& term:wordsFor(i-1)) {
                size_t next=i-1+term.count;
                double score=term.score+cost[next];
                if(score>cost[i-1]) { cost[i-1]=score; text[i-1]=term.text; step[i-1]=term.count; }
            }
        }
        if(firstLength)*firstLength=pos<len?step[pos]:0;
        std::vector<Span> spans;
        for(size_t at=pos;at<len && step[at];at+=step[at])spans.push_back({text[at],step[at]});
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
    bool finishSyllable() {
        if(raw.empty() || !dict.byCode.count(raw)) return false;
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
    std::vector<Choice> choices() const {
        std::vector<Choice> result;
        if(symbolMode){for(const wchar_t* mark:symbolGroups[symbolGroup].marks)result.push_back({mark,0});return result;}
        auto pos=target;
        if(pos>=syllables.size()) return result;
        if(nearMode){
            static const std::vector<Text> none;
            auto same=dict.byCode.find(syllables[pos]);
            const auto& exact=same==dict.byCode.end()?none:same->second;
            for(const auto& code:nearCodes(syllables[pos])){
                auto it=dict.byCode.find(code);
                if(it==dict.byCode.end())continue;
                for(const auto& text:it->second)
                    if(std::find(exact.begin(),exact.end(),text)==exact.end() &&
                       std::none_of(result.begin(),result.end(),[&](const Choice& item){return item.text==text;}))result.push_back({text,1});
            }
            return result;
        }
        if(owner(pos)){
            // A settled character: offer the other characters with the same sound, the current one first.
            const Text current=owner(pos)->text;
            result.push_back({current,1});
            for(const auto& word:words(pos))
                if(word.count==1 && word.text!=current)result.push_back(word);
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
    bool wants(WPARAM key) const {
        if(key&literalKey)return !active();
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
        added.reset();
        if(!wants(key)) return {};
        if(key&literalKey)return Text(1,wchar_t(key&0xffff));
        const bool shifted=(key&shiftKey)!=0; key&=0xffff;
        if(key==VK_ESCAPE) { if(picking)leavePicking(); else clear(); return {}; }
        if(picking) {
            auto list=choices();
            if(list.empty()){leavePicking();return {};}
            if(key==VK_TAB){
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
                auto left=context(before);
                const auto firstSingle=wasSettled?list.begin():std::find_if(list.begin(),list.end(),[](const Choice& item){return item.count==1;});
                if(preferences && !left.empty() && picked.count==1 && firstSingle!=list.end() && firstSingle->text!=picked.text)
                    edit=Correction{{left,dict.reading(syllables[target])},picked.text};
                if(preferences && picked.count>1 && picked.text.size()==picked.count){
                    Text baseline=list.front().text.substr(0,picked.count);
                    size_t change=0,changes=0;
                    if(baseline.size()==picked.count){
                        for(size_t i=0;i<picked.count;++i)if(baseline[i]!=picked.text[i]){change=i;++changes;}
                        Text ahead=context(before+picked.text.substr(0,change));
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
            if(!raw.empty()) {if(!finishSyllable()) return {}; if(key==VK_SPACE) return {};}
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
            std::optional<NewWord> keep;
            if(shifted && syllables.size()>=2 && syllables.size()<=8 && result.size()==syllables.size()){
                Text reading;bool plain=true;
                for(const auto& syllable:syllables){
                    if(symbol(syllable)){plain=false;break;}
                    if(!reading.empty())reading+=L"-";
                    reading+=dict.reading(syllable);
                }
                if(plain)keep=NewWord{reading,result};
            }
            clear();added=keep;
            return simplifiedOutput.value_or(configuredSimplifiedOutput())?convertScript(result,true):result;
        }
        wchar_t ch=codeKey(key,layout);
        if(raw.size()>=5 || syllables.size()>=128) return {};
        raw+=ch;
        if(ch==L'3'||ch==L'4'||ch==L'6'||ch==L'7') finishSyllable();
        return {};
    }
};
}
