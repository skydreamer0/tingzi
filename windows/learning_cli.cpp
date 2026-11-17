#include "learning.h"
#include <cstdio>
int wmain(int argc,wchar_t** argv){
    using namespace tingzi;
    Text action=argc>1?argv[1]:L"status";
    Learning local(localLearningDir(),action==L"on" || action==L"import");
    if(action==L"on" || action==L"off")return local.enable(action==L"on")?0:1;
    if(action==L"status"){wprintf(L"Local learning: %ls\n",local.enabled()?L"on":L"off");return 0;}
    if(action==L"export" && argc==3)return local.exportTo(argv[2])?0:1;
    if(action==L"import" && argc==3)return local.importFrom(argv[2])?0:1;
    if(action==L"forget" && argc==4)return local.forget(argv[2],argv[3])?0:1;
    if(action==L"forget-word" && argc==4)return local.forgetWord(argv[2],argv[3])?0:1;
    if(action==L"words"){
        // One word per line as UTF-8: reading, text and how many times it was committed, separated by tabs.
        std::string out;
        auto add=[&](const Text& text){
            int length=WideCharToMultiByte(CP_UTF8,0,text.data(),(int)text.size(),nullptr,0,nullptr,nullptr);
            size_t at=out.size();out.resize(at+length);
            if(length)WideCharToMultiByte(CP_UTF8,0,text.data(),(int)text.size(),&out[at],length,nullptr,nullptr);
        };
        for(const auto& word:local.allWords()){add(word.reading);out+='\t';add(word.text);out+='\t';out+=std::to_string(word.hits);out+='\n';}
        DWORD written=0;
        return out.empty() || WriteFile(GetStdHandle(STD_OUTPUT_HANDLE),out.data(),(DWORD)out.size(),&written,nullptr)?0:1;
    }
    if(action==L"clear" && argc==3 && Text(argv[2])==L"--confirm")return local.clear()?0:1;
    wprintf(L"on | off | status | export NEW.tsv | import FILE.tsv | forget CONTEXT READING | words | forget-word READING TEXT | clear --confirm\n");
    return 2;
}
