#include "engine.h"
#include <iostream>
using namespace tingzi;
int wmain(int argc,wchar_t** argv) {
    if(argc!=2)return 2;
    Dictionary d;
    if(!d.load(Text(argv[1])+L"\\"))return 2;
    Input input(d);
    input.setSegmenter(Segmenter::Path);input.setLayout(KeyLayout::Standard);
    auto expect=[](bool condition,const char* label){
        if(!condition){std::cerr<<"FAIL "<<label<<"\n";std::exit(1);}
        std::cout<<"PASS "<<label<<"\n";
    };
    auto enter=[&](const char* keys){for(const char* k=keys;*k;++k)input.press((WPARAM)*k);};
    enter("S");
    auto runs=input.displayRuns();
    expect(runs.before.empty() && runs.active==L"ㄋ" && runs.after.empty(),"first key is the active reading");
    enter("U3D04SU");
    runs=input.displayRuns();
    expect(runs.before==L"你看" && runs.active==L"ㄋㄧ" && runs.after.empty(),"composed words beside active phonetics");
    input.press(VK_BACK);
    expect(input.displayRuns().active==L"ㄋ","backspace shrinks active reading");
    input.clear();
    enter("SU3");input.press(VK_DOWN);
    auto options=input.choices();
    expect(options.size()>=2 && options[0].text==L"你" && options[1].text==L"妳","candidate order");
    input.press(VK_DOWN);
    expect(input.displayRuns().active==L"妳" && input.preview()==L"你","browse highlights without committing");
    input.press('2');expect(input.preview()==L"妳","second choice held");
    expect(input.press(VK_RETURN)==L"妳" && !input.active(),"commit once");
    expect(input.press(VK_RETURN).empty(),"no repeated commit");
    enter("U");input.press(VK_SPACE);expect(input.preview()==L"一","first tone preview");
    input.press(VK_ESCAPE);expect(!input.active(),"cancel without commit");
    enter("SU3CL3A87");input.press(VK_DOWN);options=input.choices();
    auto prefix=std::find_if(options.begin(),options.end(),[](const Choice& c){return c.text==L"你好" && c.count==2;});
    expect(prefix!=options.end(),"two-syllable prefix beside full phrase");
    size_t i=(size_t)(prefix-options.begin());
    for(size_t page=0; page<i/9; ++page)input.press(VK_NEXT);
    input.press('1'+i%9);
    expect(input.active() && input.preview().substr(0,2)==L"你好","prefix choice preserves suffix");
    input.press(VK_DOWN);options=input.choices();
    expect(std::any_of(options.begin(),options.end(),[](const Choice& c){return c.text==L"嗎" && c.count==1;}),"editable final syllable");
    input.clear();
    enter("SU3");input.press(VK_DOWN);input.press(VK_NEXT);
    expect(!input.menu().empty(),"next candidate page");
    input.press(VK_ESCAPE);expect(input.active() && !input.selecting(),"dismiss candidates");
    input.press(VK_BACK);expect(!input.active(),"backspace last completed syllable");

    // Span choice, independent of any dictionary. Fields: start, length, weight, text.
    using Spans=std::vector<WordSpan>;
    expect(chooseSpan(Spans{{0,2,5,L"甲"}})==0,"single span");
    expect(chooseSpan(Spans{{0,2,5,L"甲"},{0,2,9,L"乙"}})==1,"same span keeps the heavier word");
    expect(chooseSpan(Spans{{0,2,5,L"甲"},{0,2,5,L"乙"}})==0,"same span tie keeps the first word");
    expect(chooseSpan(Spans{{0,3,1,L"長"},{0,2,9,L"短"}})==0,"same start prefers the longer word");
    expect(chooseSpan(Spans{{0,2,5,L"前"},{1,2,5,L"後"}})==0,"overlap tie keeps the earlier word");
    expect(chooseSpan(Spans{{0,2,5,L"前"},{1,2,6,L"後"}})==1,"overlap gives way to a heavier later word");
    expect(chooseSpan(Spans{{0,3,5,L"前"},{1,2,9,L"後"}})==0,"shared end keeps the earlier word");
    expect(chooseSpan(Spans{{0,2,5,L"前"},{1,2,7,L"中"},{3,2,1,L"後"}})==0,"later word needs the full margin");
    expect(chooseSpan(Spans{{0,2,5,L"前"},{1,2,8,L"中"},{3,2,1,L"後"}})==1,"later word wins with the full margin");
    expect(chooseSpan(Spans{{0,2,5,L"前"},{1,2,8,L"中"},{2,2,6,L"後"}})==0,"later word must also clear the next one");

    input.clear();input.setSegmenter(Segmenter::Staged);
    enter("SU3");expect(input.preview()==L"你","staged single syllable keeps candidate order");
    enter("CL3");expect(input.preview()==L"你好","staged two-syllable word");
    input.press(VK_DOWN);options=input.choices();
    expect(!options.empty() && options[0].text==L"你好","staged first choice matches preview");
    input.press(VK_ESCAPE);
    expect(input.press(VK_RETURN)==L"你好" && !input.active(),"staged commit once");
    enter("SU3");input.press(VK_DOWN);input.press('2');enter("CL3");
    expect(input.preview().substr(0,1)==L"妳" && input.preview().size()==2,"staged keeps a chosen character");
    input.clear();

    expect(convertScript(L"語音輸入與體驗",true)==L"语音输入与体验","traditional to simplified");
    expect(convertScript(L"语音输入与体验",false)==L"語音輸入與體驗","simplified to traditional");
    expect(convertScript(L"abc 123，注音ㄋㄧˇ",true)==L"abc 123，注音ㄋㄧˇ","other characters pass through");
    input.setSegmenter(Segmenter::Path);input.setSimplifiedOutput(true);
    enter("SU3CL3A87");
    expect(input.preview()==L"你好嗎","preview stays traditional while composing");
    expect(input.press(VK_RETURN)==L"你好吗","commit follows simplified output");
    input.setSimplifiedOutput(false);
    enter("SU3CL3A87");
    expect(input.press(VK_RETURN)==L"你好嗎","commit stays traditional when off");

    expect(codeKey('N',KeyLayout::ETen)==L's' && codeKey(VK_OEM_7,KeyLayout::ETen)==L'h' && codeKey('5',KeyLayout::ETen)==0,"ETen keys map to the word list");
    input.setLayout(KeyLayout::ETen);
    enter("NE3HZ3");
    expect(input.press(VK_RETURN)==L"你好","ETen layout types the same words");
    enter("T");expect(input.displayRuns().active==L"ㄊ","ETen key shows its own symbol");
    input.clear();input.setLayout(KeyLayout::Standard);

    {
        wchar_t temp[MAX_PATH];GetTempPathW(MAX_PATH,temp);
        Text file=Text(temp)+L"tingzi-user-phrases-test.txt";
        {std::ofstream out(file,std::ios::binary);out<<"\xE3\x84\x8B\xE3\x84\xA7\xCB\x87-\xE3\x84\x8F\xE3\x84\xA0\xCB\x87 \xE6\x93\xAC\xE5\xA5\xBD -1.0\nbroken line\n";}
        Dictionary own;own.load(Text(argv[1])+L"\\");own.loadUser(file);DeleteFileW(file.c_str());
        Input custom(own);custom.setSegmenter(Segmenter::Path);custom.setLayout(KeyLayout::Standard);custom.setSimplifiedOutput(false);
        for(const char* k="SU3CL3";*k;++k)custom.press((WPARAM)*k);
        expect(custom.press(VK_RETURN)==L"擬好","a user phrase outranks the bundled word");
    }
    return 0;
}
