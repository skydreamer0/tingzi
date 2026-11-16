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
    if(action==L"clear" && argc==3 && Text(argv[2])==L"--confirm")return local.clear()?0:1;
    wprintf(L"on | off | status | export NEW.tsv | import FILE.tsv | forget CONTEXT READING | clear --confirm\n");
    return 2;
}
