#include "engine.h"
#include <iostream>
using namespace tingzi;
int wmain(int argc,wchar_t** argv){
    if(argc!=4)return 2;
    Dictionary dict;
    if(!dict.load(Text(argv[1])+L"\\"))return 2;
    bool train=Text(argv[3])==L"train";
    Learning store(argv[2],train);
    if(train && !store.enable(true))return 2;
    for(int round=0;round<(train?2:1);++round){
        Input input(dict);input.setPreferences(std::make_shared<const Preferences>(store.ranks()));
        for(auto key:std::string("JI394SU3"))input.press(key);
        input.press(VK_END);
        if(input.choices().front().text!=(train?L"你":L"妳"))return 1;
        input.press(train?'2':'1');
        auto edits=input.corrections();
        if(input.press(VK_RETURN)!=L"我愛妳")return 1;
        if(train && !store.save(edits))return 1;
    }
    std::cout<<"PASS "<<(train?"trained two committed context corrections":"fresh process restored contextual choice")<<"\n";
    return 0;
}
