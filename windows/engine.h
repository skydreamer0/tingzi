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
// First character of a pseudo-syllable that carries a symbol through the composition.
inline constexpr wchar_t symbolMark=L'\x01';
inline constexpr const wchar_t* symbolTable[]={L"，",L"。",L"、",L"？",L"！",L"：",L"；",L"「",L"」",L"『",L"』",L"（",L"）",L"……",L"——",L"～",L"・",L"《",L"》",L"〈",L"〉",L"【",L"】",L"＄",L"％",L"＆",L"＠"};
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
    struct Segment { Text text; size_t count; std::optional<Correction> correction; bool picked=false; };
    const Dictionary& dict;
    std::vector<Text> syllables;
    std::vector<Segment> locked;
    Text raw;
    bool picking = false;
    size_t page = 0;
    size_t highlighted=0;
    bool nearMode=false, symbolMode=false;
    // Longest first word allowed after Tab forced a break; 0 leaves it to the segmenter.
    size_t firstLimit=0;
    std::optional<NewWord> added;
    static bool symbol(const Text& syllable){return !syllable.empty() && syllable[0]==symbolMark;}
    void leavePicking(){picking=false;page=0;highlighted=0;nearMode=symbolMode=false;}
    std::shared_ptr<const Preferences> preferences;
    Segmenter segmenter=configuredSegmenter();
    KeyLayout layout=configuredLayout();
    std::optional<bool> simplifiedOutput;
    Text prefix() const { Text s;for(const auto& seg:locked)s+=seg.text;return s; }
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
    size_t start() const { size_t n=0; for(const auto& s: locked) n+=s.count; return n; }
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
    std::vector<Choice> wordsFor(size_t pos) const {
        auto list=words(pos);
        if(firstLimit && pos==start())
            list.erase(std::remove_if(list.begin(),list.end(),[&](const Choice& word){return word.count>firstLimit;}),list.end());
        return list;
    }
    Text highestPath(size_t pos,size_t* firstLength=nullptr) const {
        size_t len=syllables.size();
        std::vector<double> cost(len+1, -1e100);
        std::vector<Text> text(len+1);
        std::vector<size_t> step(len+1,0);
        cost[len]=0;
        for(size_t i=len;i>pos;--i) for(const auto& term:wordsFor(i-1)) {
            size_t next=i-1+term.count;
            double score=term.score+cost[next];
            if(score>cost[i-1]) { cost[i-1]=score; text[i-1]=term.text+text[next]; step[i-1]=term.count; }
        }
        if(firstLength)*firstLength=pos<len?step[pos]:0;
        return text[pos];
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
    Text staged(size_t pos) const {
        const size_t len=syllables.size();
        if(pos>=len)return {};
        std::vector<Text> placed(len);
        std::vector<char> taken(len,0);
        std::vector<std::vector<Choice>> options(len);
        for(size_t at=pos;at<len;++at)options[at]=wordsFor(at);
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
        Text result;
        for(size_t at=pos;at<len;++at){
            if(placed[at].empty())return highestPath(pos);
            result+=placed[at];
        }
        return result;
    }
    Text best(size_t pos) const {
        size_t len=syllables.size();
        Text result=(segmenter==Segmenter::Staged && !firstLimit)?staged(pos):highestPath(pos);
        if(preferences && !preferences->empty() && result.size()==len-pos){
            Text left=prefix();
            for(size_t i=0;i<result.size();++i){
                auto choice=preferred(left,pos+i);
                auto candidates=dict.byCode.find(syllables[pos+i]);
                if(choice.size()==1 && candidates!=dict.byCode.end() &&
                    std::find(candidates->second.begin(),candidates->second.end(),choice)!=candidates->second.end())result[i]=choice[0];
                left+=result[i];
            }
        }
        return result;
    }
    bool finishSyllable() {
        if(raw.empty() || !dict.byCode.count(raw)) return false;
        syllables.push_back(raw); raw.clear(); return true;
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
        std::vector<Correction> edits;for(const auto& seg:locked)if(seg.correction)edits.push_back(*seg.correction);
        return edits;
    }
    size_t currentPage() const { return page; }
    size_t currentIndex() const { return highlighted; }
    bool active() const { return !raw.empty() || !syllables.empty(); }
    bool selecting() const { return picking; }
    void clear(){ raw.clear(); syllables.clear(); locked.clear(); firstLimit=0; leavePicking(); }
    Text preview() const {
        Text text;
        for(const auto& s:locked) text+=s.text;
        return text+best(start())+dict.reading(raw);
    }
    struct DisplayRuns { Text before, active, after; };
    DisplayRuns displayRuns() const {
        if (picking) {
            auto list=choices();
            if(symbolMode && highlighted<list.size())return {prefix()+best(start()),list[highlighted].text,{}};
            if(highlighted<list.size()) {
                const auto& choice=list[highlighted];
                return {prefix(),choice.text,best(start()+choice.count)};
            }
        }
        return {prefix()+best(start()),dict.reading(raw),{}};
    }
    std::vector<Choice> choices() const {
        std::vector<Choice> result;
        if(symbolMode){for(const wchar_t* mark:symbolTable)result.push_back({mark,0});return result;}
        auto pos=start();
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
        result.push_back({best(pos),syllables.size()-pos});
        const Text primary=result.front().text;
        if(primary.size()>1 && primary.size()==syllables.size()-pos){
            static const std::vector<Text> none;
            auto found=dict.byCode.find(syllables.back());
            const auto& last=found==dict.byCode.end()?none:found->second;
            for(size_t i=0;i<(std::min)(size_t(2),last.size());++i){
                if(last[i].size()==1 && primary.back()!=last[i][0])
                    result.push_back({primary.substr(0,primary.size()-1)+last[i],syllables.size()-pos});
            }
        }
        auto options=words(pos);
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
    Text listTitle() const {return symbolMode?L"符號":nearMode?L"近音字":L"注音選字";}
    // Runs of characters the user picked one by one, which are not yet a word in the list.
    std::vector<NewWord> newWords() const {
        std::vector<NewWord> found;
        Text reading,text;size_t run=0,at=0;
        auto flush=[&]{
            if(run>=2 && run<=4){
                auto known=dict.phrases.find(reading);
                if(known==dict.phrases.end() || std::none_of(known->second.begin(),known->second.end(),[&](const Term& term){return term.text==text;}))
                    found.push_back({reading,text});
            }
            reading.clear();text.clear();run=0;
        };
        for(const auto& seg:locked){
            if(seg.picked && seg.count==1 && seg.text.size()==1 && at<syllables.size() && !symbol(syllables[at])){
                if(run)reading+=L"-";
                reading+=dict.reading(syllables[at]);text+=seg.text;++run;
            } else flush();
            at+=seg.count;
        }
        flush();
        return found;
    }
    // The word the last key asked to remember, when Shift+Enter committed the composition.
    std::optional<NewWord> addedWord() const {return added;}
    bool wants(WPARAM key) const {
        const bool shifted=(key&shiftKey)!=0; key&=0xffff;
        if(picking) return !shifted && ((key>='1' && key<='9') || key==VK_NEXT || key==VK_PRIOR || key==VK_UP || key==VK_DOWN || key==VK_LEFT || key==VK_RIGHT || key==VK_SPACE || key==VK_ESCAPE || key==VK_RETURN || key==VK_BACK || key==VK_TAB);
        if(shifted) return !punctuation(key,true,layout).empty() || (key==VK_RETURN && active());
        if(dict.keys.count(codeKey(key,layout))) return true;
        if(!punctuation(key,false,layout).empty() || key==VK_OEM_3) return true;
        return active() && (key==VK_BACK || key==VK_ESCAPE || key==VK_RETURN || key==VK_SPACE || key==VK_DOWN || key==VK_END || key==VK_TAB);
    }
    // Returns committed text separately from the remaining preedit.
    Text press(WPARAM key) {
        added.reset();
        if(!wants(key)) return {};
        const bool shifted=(key&shiftKey)!=0; key&=0xffff;
        if(key==VK_ESCAPE) { if(picking)leavePicking(); else clear(); return {}; }
        if(picking) {
            auto list=choices();
            if(list.empty()){leavePicking();return {};}
            if(key==VK_TAB){
                if(symbolMode)return {};
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
                    if(syllables.size()<128)syllables.push_back(Text(1,symbolMark)+picked.text);
                    return {};
                }
                if(nearMode){locked.push_back({picked.text,1,{},false});leavePicking();firstLimit=0;return {};}
                std::optional<Correction> edit;
                auto left=context(prefix());
                const auto firstSingle=std::find_if(list.begin(),list.end(),[](const Choice& item){return item.count==1;});
                if(preferences && !left.empty() && picked.count==1 && firstSingle!=list.end() && firstSingle->text!=picked.text)
                    edit=Correction{{left,dict.reading(syllables[start()])},picked.text};
                if(preferences && picked.count>1 && picked.text.size()==picked.count){
                    Text baseline=list.front().text.substr(0,picked.count);
                    size_t change=0,changes=0;
                    if(baseline.size()==picked.count){
                        for(size_t i=0;i<picked.count;++i)if(baseline[i]!=picked.text[i]){change=i;++changes;}
                        Text before=context(prefix()+picked.text.substr(0,change));
                        auto valid=dict.byCode.find(syllables[start()+change]);
                        if(changes==1 && !before.empty() && valid!=dict.byCode.end()){
                            Text selected=picked.text.substr(change,1);
                            if(std::find(valid->second.begin(),valid->second.end(),selected)!=valid->second.end())
                                edit=Correction{{before,dict.reading(syllables[start()+change])},selected};
                        }
                    }
                }
                locked.push_back({picked.text,picked.count,edit,true});leavePicking();firstLimit=0;
            }
            return {};
        }
        if(const Text mark=punctuation(key,shifted,layout);!mark.empty()){
            if(!raw.empty() && !finishSyllable())return {};
            if(!active())return mark;
            if(syllables.size()<128)syllables.push_back(Text(1,symbolMark)+mark);
            return {};
        }
        if(key==VK_OEM_3){
            if(!raw.empty() && !finishSyllable())return {};
            symbolMode=true;picking=true;page=0;highlighted=0;return {};
        }
        if(key==VK_BACK){
            firstLimit=0;
            if(!raw.empty()) raw.pop_back();
            else if(!locked.empty() && start()==syllables.size()) locked.pop_back();
            else if(!syllables.empty()) syllables.pop_back();
            return {};
        }
        if(key==VK_TAB){
            // Each press makes the first word one syllable shorter; after one syllable the limit lifts.
            if(!raw.empty() && !finishSyllable())return {};
            if(start()>=syllables.size())return {};
            size_t current=0;highestPath(start(),&current);
            firstLimit=current>1?current-1:0;
            return {};
        }
        if(key==VK_DOWN || key==VK_SPACE || key==VK_END) {
            if(!raw.empty()) {if(!finishSyllable()) return {}; if(key==VK_SPACE) return {};}
            if(key==VK_END && start()<syllables.size()){
                Text tail=best(start());
                if(tail.size()==syllables.size()-start())
                    for(size_t i=0;i+1<tail.size();++i)locked.push_back({tail.substr(i,1),1,{}});
            }
            page=0;highlighted=0;
            picking = start()<syllables.size(); return {};
        }
        if(key==VK_RETURN) {
            if(!raw.empty() && !finishSyllable()) return {};
            Text result=preview();
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
