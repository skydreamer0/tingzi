#pragma once
#include <windows.h>
#include <algorithm>
#include <cmath>
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
    Text reading(const Text& code) const {
        Text result;
        for (auto ch : code) { auto it = keys.find(ch); if (it != keys.end()) result += it->second; }
        return result;
    }
};
inline wchar_t codeKey(WPARAM key) {
    if (key >= 'A' && key <= 'Z') return wchar_t(key + 32);
    if (key >= '0' && key <= '9') return wchar_t(key);
    switch (key) {
    case VK_OEM_COMMA: return L','; case VK_OEM_PERIOD: return L'.';
    case VK_OEM_1: return L';'; case VK_OEM_2: return L'/'; case VK_OEM_MINUS: return L'-';
    default: return 0;
    }
}
class Input {
    struct Segment { Text text; size_t count; std::optional<Correction> correction; };
    const Dictionary& dict;
    std::vector<Text> syllables;
    std::vector<Segment> locked;
    Text raw;
    bool picking = false;
    size_t page = 0;
    size_t highlighted=0;
    std::shared_ptr<const Preferences> preferences;
    Segmenter segmenter=configuredSegmenter();
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
        Text reading;
        for(size_t end=pos; end<syllables.size() && end-pos<dict.longestPhrase; ++end) {
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
    Text highestPath(size_t pos) const {
        size_t len=syllables.size();
        std::vector<double> cost(len+1, -1e100);
        std::vector<Text> text(len+1);
        cost[len]=0;
        for(size_t i=len;i>pos;--i) for(const auto& term:words(i-1)) {
            size_t next=i-1+term.count;
            double score=term.score+cost[next];
            if(score>cost[i-1]) { cost[i-1]=score; text[i-1]=term.text+text[next]; }
        }
        return text[pos];
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
        for(size_t at=pos;at<len;++at)options[at]=words(at);
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
        Text result=segmenter==Segmenter::Staged?staged(pos):highestPath(pos);
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
    bool learningActive() const {return bool(preferences);}
    std::vector<Correction> corrections() const {
        std::vector<Correction> edits;for(const auto& seg:locked)if(seg.correction)edits.push_back(*seg.correction);
        return edits;
    }
    size_t currentPage() const { return page; }
    size_t currentIndex() const { return highlighted; }
    bool active() const { return !raw.empty() || !syllables.empty(); }
    bool selecting() const { return picking; }
    void clear(){ raw.clear(); syllables.clear(); locked.clear(); picking=false; page=0;highlighted=0; }
    Text preview() const {
        Text text;
        for(const auto& s:locked) text+=s.text;
        return text+best(start())+dict.reading(raw);
    }
    struct DisplayRuns { Text before, active, after; };
    DisplayRuns displayRuns() const {
        if (picking) {
            auto list=choices();
            if(highlighted<list.size()) {
                const auto& choice=list[highlighted];
                return {prefix(),choice.text,best(start()+choice.count)};
            }
        }
        return {prefix()+best(start()),dict.reading(raw),{}};
    }
    std::vector<Choice> choices() const {
        std::vector<Choice> result;
        auto pos=start();
        if(pos>=syllables.size()) return result;
        result.push_back({best(pos),syllables.size()-pos});
        const Text primary=result.front().text;
        if(primary.size()>1 && primary.size()==syllables.size()-pos){
            const auto& last=dict.byCode.at(syllables.back());
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
    bool wants(WPARAM key) const {
        if(picking) return (key>='1' && key<='9') || key==VK_NEXT || key==VK_PRIOR || key==VK_UP || key==VK_DOWN || key==VK_LEFT || key==VK_RIGHT || key==VK_SPACE || key==VK_ESCAPE || key==VK_RETURN || key==VK_BACK;
        if(dict.keys.count(codeKey(key))) return true;
        return active() && (key==VK_BACK || key==VK_ESCAPE || key==VK_RETURN || key==VK_SPACE || key==VK_DOWN || key==VK_END);
    }
    // Returns committed text separately from the remaining preedit.
    Text press(WPARAM key) {
        if(!wants(key)) return {};
        if(key==VK_ESCAPE) { if(picking){picking=false;page=0;} else clear(); return {}; }
        if(picking) {
            auto list=choices();
            if(list.empty())return {};
            if(key==VK_NEXT || key==VK_SPACE || key==VK_PRIOR){
                if(key==VK_PRIOR){if(page)--page;}else if((page+1)*9<list.size())++page;
                highlighted=page*9;return {};
            }
            if(key==VK_DOWN || key==VK_UP || key==VK_LEFT || key==VK_RIGHT){
                const int delta=(key==VK_DOWN || key==VK_RIGHT)?1:-1;
                highlighted=static_cast<size_t>(std::clamp<int>(int(highlighted)+delta,0,int(list.size()-1)));
                page=highlighted/9;return {};
            }
            if(key==VK_BACK){picking=false; page=0; return {};}
            size_t idx=key==VK_RETURN?highlighted:page*9+key-'1';
            if(idx<list.size()){
                const auto& picked=list[idx];
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
                        if(changes==1 && !before.empty()){
                            const auto& valid=dict.byCode.at(syllables[start()+change]);
                            Text selected=picked.text.substr(change,1);
                            if(std::find(valid.begin(),valid.end(),selected)!=valid.end())
                                edit=Correction{{before,dict.reading(syllables[start()+change])},selected};
                        }
                    }
                }
                locked.push_back({picked.text,picked.count,edit});picking=false;page=0;highlighted=0;
            }
            return {};
        }
        if(key==VK_BACK){
            if(!raw.empty()) raw.pop_back();
            else if(!locked.empty() && start()==syllables.size()) locked.pop_back();
            else if(!syllables.empty()) syllables.pop_back();
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
            Text result=preview(); clear(); return result;
        }
        wchar_t ch=codeKey(key);
        if(raw.size()>=5 || syllables.size()>=128) return {};
        raw+=ch;
        if(ch==L'3'||ch==L'4'||ch==L'6'||ch==L'7') finishSyllable();
        return {};
    }
};
}
