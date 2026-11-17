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

    expect(input.press(shiftKey|VK_OEM_COMMA)==L"，" && !input.active(),"punctuation commits at once when idle");
    enter("SU3CL3");input.press(shiftKey|VK_OEM_COMMA);enter("SU3CL3");input.press(shiftKey|'1');
    expect(input.preview()==L"你好，你好！","punctuation stays inside the sentence");
    expect(input.press(VK_RETURN)==L"你好，你好！","sentence with punctuation commits whole");
    expect(input.press(VK_OEM_4)==L"「" && input.press(shiftKey|'6')==L"……","brackets and ellipsis");
    input.press(VK_OEM_3);
    expect(input.selecting() && input.listTitle().rfind(L"符號・常用",0)==0 && input.choices().size()>20,"backtick opens the symbol list");
    expect(input.press('2')==L"。" && !input.active(),"symbol from the list commits when idle");
    enter("SU3");input.press(VK_OEM_3);input.press('1');
    expect(input.press(VK_RETURN)==L"你，","symbol from the list joins the sentence");

    enter("G4RU");input.press(VK_OEM_COMMA);enter("4");
    Text whole=input.preview();
    input.press(VK_TAB);
    Text broken=input.preview();
    input.press(VK_TAB);
    expect(whole==L"世界" && broken.size()==2 && broken!=whole && input.preview()==whole,"Tab forces a break and a second Tab lifts it");
    input.clear();

    enter("SU3");input.press(VK_DOWN);input.press(VK_TAB);
    options=input.choices();
    expect(input.listTitle()==L"近音字" && std::any_of(options.begin(),options.end(),[](const Choice& c){return c.text==L"李";})
        && std::none_of(options.begin(),options.end(),[](const Choice& c){return c.text==L"你";}),"Tab in the list shows near-sounding characters");
    input.press(VK_TAB);
    expect(input.listTitle()==L"注音選字" && input.choices().front().text==L"你","Tab again returns to the same sound");
    input.clear();

    enter("SU3CL3");input.press(VK_DOWN);
    options=input.choices();
    size_t single=0;while(single<options.size() && !(options[single].count==1 && options[single].text==L"妳"))++single;
    for(size_t p=0;p<single/9;++p)input.press(VK_NEXT);
    input.press('1'+single%9);
    input.press(VK_DOWN);input.press('2');
    auto fresh=input.newWords();
    expect(fresh.size()==1 && fresh[0].reading==L"ㄋㄧˇ-ㄏㄠˇ" && fresh[0].text.size()==2 && fresh[0].text[0]==L'妳',"picked characters are offered as a new word");
    input.clear();
    enter("SU3CL3");
    expect(input.newWords().empty(),"an untouched sentence teaches no word");
    expect(input.press(shiftKey|VK_RETURN)==L"你好" && input.addedWord() && input.addedWord()->reading==L"ㄋㄧˇ-ㄏㄠˇ" && input.addedWord()->text==L"你好","Shift+Enter commits and remembers the word");
    enter("SU3");
    expect(input.press(VK_RETURN)==L"你" && !input.addedWord(),"plain Enter remembers nothing");

    // The caret: move inside the sentence, change one character, type and delete in the middle.
    auto pick=[&](const Text& wanted,size_t count){
        auto list=input.choices();
        size_t at=0;while(at<list.size() && !(list[at].text==wanted && list[at].count==count))++at;
        if(at>=list.size())return false;
        while(input.currentPage()<at/9)input.press(VK_NEXT);
        input.press('1'+at%9);return true;
    };
    input.clear();
    enter("SU3CL3A87");
    input.press(VK_LEFT);
    runs=input.displayRuns();
    expect(runs.before==L"你好" && runs.active.empty() && runs.after==L"嗎" && input.caretOffset()==2 && input.caretInside(),"Left moves the caret inside the sentence");
    input.press(VK_DOWN);
    options=input.choices();
    auto other=std::find_if(options.begin(),options.end(),[](const Choice& c){return c.count==1 && c.text!=L"好";});
    expect(input.selecting() && other!=options.end(),"Down chooses the character left of the caret");
    Text swapped=other->text;
    expect(pick(swapped,1) && input.preview()==L"你"+swapped+L"嗎","changing a middle character keeps its neighbours");
    input.press(VK_DOWN);
    options=input.choices();
    expect(options.front().text==swapped && std::all_of(options.begin(),options.end(),[](const Choice& c){return c.count==1;}),"a settled character can be chosen again");
    expect(pick(L"好",1) && input.preview()==L"你好嗎","and changed back");
    input.press(VK_RIGHT);
    expect(!input.caretInside() && input.caretOffset()==3,"Right past the last character returns to the end");
    input.press(VK_HOME);
    expect(input.caretOffset()==0 && input.displayRuns().after==L"你好嗎","Home goes to the start");
    input.clear();
    enter("SU3A87");input.press(VK_LEFT);enter("CL3");
    expect(input.preview()==L"你好嗎" && input.caretOffset()==2,"typing at the caret inserts a syllable");
    enter("C");
    runs=input.displayRuns();
    expect(runs.before==L"你好" && runs.active==L"ㄏ" && runs.after==L"嗎" && input.preview()==L"你好ㄏ嗎","the reading being typed shows at the caret");
    input.press(VK_BACK);input.press(VK_BACK);
    expect(input.preview()==L"你嗎" && input.caretOffset()==1,"Backspace removes the syllable before the caret");
    input.press(VK_DELETE);
    expect(input.preview()==L"你" && !input.caretInside(),"Delete removes the syllable after the caret");
    input.clear();
    enter("SU3CL3A87");input.press(VK_END);
    options=input.choices();
    expect(input.selecting() && options.front().text==L"嗎","End chooses the last character");
    other=std::find_if(options.begin(),options.end(),[](const Choice& c){return c.count==1 && c.text!=L"嗎";});
    swapped=other==options.end()?Text():other->text;
    expect(!swapped.empty() && pick(swapped,1) && input.preview()==L"你好"+swapped,"and leaves the words before it alone");
    expect(input.press(VK_RETURN)==L"你好"+swapped,"the edited sentence commits whole");
    input.press(VK_OEM_3);input.press(VK_TAB);
    expect(input.listTitle().rfind(L"符號・括號",0)==0 && input.press('1')==L"「","Tab in the symbol list moves to the next group");

    {
        wchar_t temp[MAX_PATH];GetTempPathW(MAX_PATH,temp);
        Text file=Text(temp)+L"tingzi-templates-test.txt";
        {std::ofstream out(file,std::ios::binary);out<<narrow(L"ㄋㄧˇ-ㄏㄠˇ\t您好，這是範本\nno tab here\n");}
        Dictionary own;own.load(Text(argv[1])+L"\\");own.loadTemplates(file);DeleteFileW(file.c_str());
        Input custom(own);custom.setSegmenter(Segmenter::Path);custom.setLayout(KeyLayout::Standard);custom.setSimplifiedOutput(false);
        for(const char* k="SU3CL3";*k;++k)custom.press((WPARAM)*k);
        auto offered=custom.preview();custom.press(VK_DOWN);
        auto list=custom.choices();
        auto entry=std::find_if(list.begin(),list.end(),[](const Choice& c){return c.text==L"您好，這是範本";});
        expect(offered==L"你好" && entry!=list.end(),"a template is offered in the list but not chosen by itself");
        size_t at=size_t(entry-list.begin());
        for(size_t p=0;p<at/9;++p)custom.press(VK_NEXT);
        custom.press('1'+at%9);
        expect(custom.press(VK_RETURN)==L"您好，這是範本","choosing the template commits its text");
    }
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
