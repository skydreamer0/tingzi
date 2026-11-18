// Measures how often the engine picks the intended sentence, and how quickly it answers each key.
//
//   benchmark.exe <word data folder> <sentences.tsv> [baseline.tsv] [--write-baseline <file>]
//
// The word data folder holds Tingzi.cin, phrases.txt and pairs.txt, as installed. Each case in
// sentences.tsv is typed key by key into a fresh composition, once with each segmenter, and the
// preview is compared with the intended text. Latin words in a reading are passed through as they
// are, and the Zhuyin on either side of them is typed as a composition of its own, the way a word
// in English ends a composition when it is typed.
//
// With a baseline, the run fails when any category gets fewer sentences or characters right than
// the baseline records, or when answering one key takes too long. A change that gets more right
// should update the baseline in the same commit.
#include "../engine.h"
#include <chrono>
#include <cstdio>
#include <iomanip>
#include <iostream>
using namespace tingzi;

namespace {
// Typing should never wait on the engine. Most keys must answer within the first limit; the second
// allows for the odd key that a busy build machine delays.
constexpr double typicalKeyLimitMs=20.0, slowestKeyLimitMs=250.0;

struct Case { Text category, text, reading, note; };
struct Tally {
    size_t cases=0, sentences=0, characters=0, correctCharacters=0, wrong=0, reachable=0;
};
struct Piece { bool latin; Text text; std::vector<Text> syllables; };

std::vector<Text> fields(const Text& line,wchar_t mark){
    std::vector<Text> out;size_t at=0;
    for(;;){
        const size_t next=line.find(mark,at);
        out.push_back(line.substr(at,next==Text::npos?Text::npos:next-at));
        if(next==Text::npos)return out;
        at=next+1;
    }
}
std::vector<Case> readCases(const std::filesystem::path& file){
    std::ifstream in(file,std::ios::binary);
    std::vector<Case> cases;std::string line;
    while(std::getline(in,line)){
        if(!line.empty() && line.back()=='\r')line.pop_back();
        if(line.empty() || line[0]=='#')continue;
        auto parts=fields(wide(line),L'\t');
        if(parts.size()<3)continue;
        cases.push_back({parts[0],parts[1],parts[2],parts.size()>3?parts[3]:Text()});
    }
    return cases;
}
bool zhuyin(wchar_t ch){return (ch>=0x3105 && ch<=0x312f) || ch==L'ˊ' || ch==L'ˇ' || ch==L'ˋ' || ch==L'˙';}
// Splits a reading into compositions of Zhuyin syllables and the Latin words between them.
std::vector<Piece> pieces(const Text& reading){
    std::vector<Piece> out;
    for(const auto& token:fields(reading,L' ')){
        if(token.empty())continue;
        const bool latin=!zhuyin(token[0]) && !(token.size()>1 && token[0]==L'^' && zhuyin(token[1]));
        if(latin || out.empty() || out.back().latin)out.push_back({latin,{},{}});
        if(latin)out.back().text=token;else out.back().syllables.push_back(token);
    }
    return out;
}
// The keys for one syllable on the standard layout; a syllable without a tone mark ends with Space.
// Two marks describe fast typing: a leading ^ presses the tone before the syllable's last key, and a
// trailing ! leaves out the Space that ends a first-tone syllable.
std::vector<WPARAM> keysFor(const Dictionary& d,Text syllable,bool& known){
    static std::map<Text,wchar_t> byName;
    if(byName.empty())for(const auto& [key,name]:d.keys)byName[name]=key;
    const bool early=!syllable.empty() && syllable.front()==L'^';
    const bool noSpace=!syllable.empty() && syllable.back()==L'!';
    if(early)syllable.erase(0,1);
    if(noSpace)syllable.pop_back();
    std::vector<WPARAM> keys;
    for(wchar_t ch:syllable){
        auto found=byName.find(Text(1,ch));
        if(found==byName.end()){known=false;continue;}
        const wchar_t k=found->second;
        WPARAM vk=0;
        if(k>=L'a' && k<=L'z')vk=WPARAM(k-32);
        else if(k>=L'0' && k<=L'9')vk=WPARAM(k);
        else switch(k){
            case L',':vk=VK_OEM_COMMA;break; case L'.':vk=VK_OEM_PERIOD;break; case L';':vk=VK_OEM_1;break;
            case L'/':vk=VK_OEM_2;break; case L'-':vk=VK_OEM_MINUS;break; default:known=false;
        }
        if(vk)keys.push_back(vk);
    }
    const wchar_t last=syllable.empty()?0:syllable.back();
    const bool toned=last==L'ˊ' || last==L'ˇ' || last==L'ˋ' || last==L'˙';
    if(!toned && !noSpace)keys.push_back(VK_SPACE);
    if(early && toned && keys.size()>=3)std::swap(keys[keys.size()-1],keys[keys.size()-2]);
    return keys;
}
std::vector<WPARAM> keysFor(const Dictionary& d,const std::vector<Text>& syllables,bool& known){
    std::vector<WPARAM> keys;
    for(const auto& syllable:syllables){auto more=keysFor(d,syllable,known);keys.insert(keys.end(),more.begin(),more.end());}
    return keys;
}
size_t distance(const Text& a,const Text& b){
    std::vector<size_t> row(b.size()+1);
    for(size_t j=0;j<=b.size();++j)row[j]=j;
    for(size_t i=1;i<=a.size();++i){
        size_t diagonal=row[0];row[0]=i;
        for(size_t j=1;j<=b.size();++j){
            const size_t above=row[j];
            row[j]=(std::min)({row[j]+1,row[j-1]+1,diagonal+(a[i-1]==b[j-1]?0:1)});
            diagonal=above;
        }
    }
    return row[b.size()];
}
// True when the intended text at `at` is on the first page of the candidate list there, or of the
// near-sound list that Tab switches to.
bool reachable(const Dictionary& d,Segmenter mode,const std::vector<WPARAM>& keys,size_t at,size_t count,const Text& intended){
    Input input(d);input.setSegmenter(mode);input.setLayout(KeyLayout::Standard);input.setSimplifiedOutput(false);
    for(WPARAM key:keys)input.press(key);
    if(at+1==count)input.press(VK_END);
    else{input.press(VK_HOME);for(size_t i=0;i<=at;++i)input.press(VK_RIGHT);input.press(VK_DOWN);}
    auto onFirstPage=[&]{
        const auto list=input.choices();
        for(size_t i=0;i<list.size() && i<9;++i){
            const Text& text=list[i].text;
            if(!text.empty() && intended.compare(at,text.size(),text)==0)return true;
        }
        return false;
    };
    if(onFirstPage())return true;
    input.press(VK_TAB);
    return onFirstPage();
}
const char* name(Segmenter mode){return mode==Segmenter::Path?"path":"staged";}
}

int wmain(int argc,wchar_t** argv){
    if(argc<3)return 2;
    std::filesystem::path baselineFile,writeTo;
    for(int i=3;i<argc;++i){
        if(Text(argv[i])==L"--write-baseline" && i+1<argc)writeTo=argv[++i];
        else baselineFile=argv[i];
    }
    Dictionary d;
    const Text dir=Text(argv[1])+L"\\";
    if(!d.load(dir))return 2;
    d.loadPairs(dir+L"pairs.txt");
    const auto cases=readCases(argv[2]);
    if(cases.empty())return 2;

    using Clock=std::chrono::steady_clock;
    std::map<std::pair<std::string,std::string>,Tally> tallies;
    std::vector<double> keyTimes;
    std::ostringstream misses;
    for(Segmenter mode:{Segmenter::Path,Segmenter::Staged}){
        misses<<"\n### "<<name(mode)<<"\n\n";
        for(const auto& c:cases){
            auto& tally=tallies[{name(mode),narrow(c.category)}];
            Text got,plainWant;size_t wrong=0,reach=0;bool known=true;
            for(wchar_t ch:c.text)if(ch!=L' ')plainWant+=ch;
            for(const auto& piece:pieces(c.reading)){
                if(piece.latin){got+=piece.text;continue;}
                const auto keys=keysFor(d,piece.syllables,known);
                Input input(d);input.setSegmenter(mode);input.setLayout(KeyLayout::Standard);input.setSimplifiedOutput(false);
                for(WPARAM key:keys){
                    const auto start=Clock::now();
                    input.press(key);
                    const Text shown=input.preview();
                    keyTimes.push_back(std::chrono::duration<double,std::milli>(Clock::now()-start).count());
                    (void)shown;
                }
                const Text part=input.preview();
                // The intended characters for this piece, by position in the whole sentence.
                const Text want=plainWant.substr((std::min)(got.size(),plainWant.size()),part.size());
                if(part.size()==piece.syllables.size() && want.size()==part.size())
                    for(size_t i=0;i<part.size();++i)if(part[i]!=want[i]){
                        ++wrong;
                        if(reachable(d,mode,keys,i,part.size(),want))++reach;
                    }
                got+=part;
            }
            if(!known)std::cerr<<"unknown Zhuyin in: "<<narrow(c.text)<<"\n";
            const size_t miss=distance(got,plainWant);
            ++tally.cases;
            tally.characters+=plainWant.size();
            tally.correctCharacters+=plainWant.size()>miss?plainWant.size()-miss:0;
            tally.wrong+=wrong;tally.reachable+=reach;
            if(got==plainWant)++tally.sentences;
            else misses<<"- "<<narrow(c.category)<<"：想要「"<<narrow(plainWant)<<"」，得到「"<<narrow(got)<<"」"
                          <<(c.note.empty()?"":"（"+narrow(c.note)+"）")<<"\n";
        }
    }

    std::cout<<"## Word choice\n\n| segmenter | category | cases | sentences right | characters right | wrong characters on the first page |\n|---|---|---|---|---|---|\n";
    std::cout<<std::fixed<<std::setprecision(1);
    for(const auto& [key,t]:tallies){
        std::cout<<"| "<<key.first<<" | "<<key.second<<" | "<<t.cases<<" | "<<t.sentences<<" ("<<100.0*t.sentences/t.cases<<"%) | "
                 <<t.correctCharacters<<"/"<<t.characters<<" ("<<100.0*t.correctCharacters/t.characters<<"%) | "
                 <<t.reachable<<"/"<<t.wrong<<" |\n";
    }
    std::sort(keyTimes.begin(),keyTimes.end());
    auto at=[&](double share){return keyTimes[(std::min)(keyTimes.size()-1,size_t(share*keyTimes.size()))];};
    std::cout<<std::setprecision(3)<<"\nPer key, in ms: median "<<at(0.5)<<", 95th percentile "<<at(0.95)<<", slowest "<<keyTimes.back()
             <<" ("<<keyTimes.size()<<" keys)\n";
    std::cout<<"\n## Sentences not typed as intended\n"<<misses.str();

    if(!writeTo.empty()){
        std::ofstream out(writeTo,std::ios::binary);
        out<<"# segmenter\tcategory\tsentences right\tcharacters right\twrong characters on the first page\n";
        for(const auto& [key,t]:tallies)out<<key.first<<'\t'<<key.second<<'\t'<<t.sentences<<'\t'<<t.correctCharacters<<'\t'<<t.reachable<<'\n';
    }
    bool ok=at(0.95)<=typicalKeyLimitMs && keyTimes.back()<=slowestKeyLimitMs;
    if(!ok)std::cout<<"FAIL keys took longer than "<<typicalKeyLimitMs<<" ms at the 95th percentile or "<<slowestKeyLimitMs<<" ms at the slowest\n";
    if(!baselineFile.empty()){
        std::ifstream in(baselineFile,std::ios::binary);std::string line;
        if(!in){std::cout<<"FAIL no baseline at the given path\n";return 1;}
        while(std::getline(in,line)){
            if(!line.empty() && line.back()=='\r')line.pop_back();
            if(line.empty() || line[0]=='#')continue;
            std::istringstream row(line);std::string mode,category;size_t sentences=0,characters=0,reach=0;
            std::getline(row,mode,'\t');std::getline(row,category,'\t');row>>sentences>>characters>>reach;
            const auto found=tallies.find({mode,category});
            if(found==tallies.end()){std::cout<<"FAIL "<<mode<<" "<<category<<" is in the baseline but has no cases\n";ok=false;continue;}
            const auto& t=found->second;
            if(t.sentences<sentences || t.correctCharacters<characters || t.reachable<reach){
                std::cout<<"FAIL "<<mode<<" "<<category<<": "<<t.sentences<<" sentences, "<<t.correctCharacters<<" characters and "<<t.reachable
                         <<" fixes on the first page, the baseline has "<<sentences<<", "<<characters<<" and "<<reach<<"\n";
                ok=false;
            } else if(t.sentences>sentences || t.correctCharacters>characters || t.reachable>reach)
                std::cout<<"BETTER "<<mode<<" "<<category<<": update baseline.tsv to "<<t.sentences<<", "<<t.correctCharacters<<" and "<<t.reachable<<"\n";
        }
    }
    return ok?0:1;
}
