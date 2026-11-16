// Prints what each segmenter produces for the same keys, one line per input.
// Usage: segment_compare <dictionary folder> <keys> [<keys> ...]
// Keys use the standard Zhuyin layout, for example SU3CL3 for ㄋㄧˇㄏㄠˇ.
#include "engine.h"
#include <fcntl.h>
#include <io.h>
#include <iostream>
using namespace tingzi;
int wmain(int argc,wchar_t** argv) {
    if(argc<3)return 2;
    Dictionary dictionary;
    if(!dictionary.load(Text(argv[1])+L"\\"))return 2;
    _setmode(_fileno(stdout),_O_U8TEXT);
    auto convert=[&](const wchar_t* keys,Segmenter mode){
        Input input(dictionary);
        input.setSegmenter(mode);
        for(const wchar_t* key=keys;*key;++key){
            WPARAM code=(WPARAM)towupper(*key);
            switch(*key){
            case L' ': code=VK_SPACE; break; case L',': code=VK_OEM_COMMA; break;
            case L'.': code=VK_OEM_PERIOD; break; case L';': code=VK_OEM_1; break;
            case L'/': code=VK_OEM_2; break; case L'-': code=VK_OEM_MINUS; break;
            }
            input.press(code);
        }
        return input.preview();
    };
    int differences=0;
    for(int i=2;i<argc;++i){
        Text path=convert(argv[i],Segmenter::Path), staged=convert(argv[i],Segmenter::Staged);
        if(path!=staged)++differences;
        std::wcout<<argv[i]<<L"\t"<<path<<L"\t"<<staged<<(path==staged?L"":L"\tDIFFERENT")<<L"\n";
    }
    std::wcout<<L"inputs="<<argc-2<<L" different="<<differences<<L"\n";
    return 0;
}
