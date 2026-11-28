#include "engine.h"
#include <iostream>
using namespace tingzi;
int wmain(int argc,wchar_t** argv){
    if(argc!=3)return 2;
    Dictionary dict;
    if(!dict.load(Text(argv[1])+L"\\") || std::filesystem::exists(argv[2]))return 2;
    auto check=[](bool value,const char* msg){std::cout<<(value?"PASS ":"FAIL ")<<msg<<"\n";if(!value)std::exit(1);};
    auto type=[](Input& input,const std::string& keys){for(unsigned char k:keys)input.press(k);};
    check(Learning(argv[2]).clear(),"a previously absent profile is already cleared");
    {Learning store(argv[2],true);check(!store.enabled() && store.enable(true),"explicit opt-in");}
    for(int run=0;run<4;++run){
        Learning store(argv[2]);
        Input input(dict);input.setPreferences(std::make_shared<const Preferences>(store.ranks()));
        type(input,"JI394SU3");
        bool learned=run==3;
        check(input.preview()==(learned?L"我愛妳":L"我愛你"),"whole sentence across fresh sessions");
        input.press(VK_END);
        auto list=input.choices();
        check(list.at(0).text==(learned?L"妳":L"你") && list.at(1).text==(learned?L"你":L"妳"),"contextual position");
        if(run==0){input.press('2');input.press(VK_ESCAPE);check(input.corrections().empty(),"cancel discards correction");}
        if(run==1 || run==2){
            input.press('2');auto edits=input.corrections();
            check(input.press(VK_RETURN)==L"我愛妳" && edits.size()==1 && edits[0].key.first==L"我愛","explicit override committed");
            check(store.save(edits),"persist committed override");
        }
    }
    Learning reopened(argv[2]);Input input(dict);input.setPreferences(std::make_shared<const Preferences>(reopened.ranks()));
    type(input,"SU3");input.press(VK_DOWN);
    check(input.choices().at(0).text==L"你" && input.choices().at(1).text==L"妳","isolated order unchanged");
    Input other(dict);other.setPreferences(std::make_shared<const Preferences>(reopened.ranks()));
    type(other,"CL3SU3");other.press(VK_END);
    check(other.choices().front().text==L"你","different context unchanged");
    {
        auto picks=reopened.favorites();
        check(picks.size()==1 && picks[L"ㄋㄧˇ"].size()==1 && picks[L"ㄋㄧˇ"][0]==std::make_pair(Text(L"妳"),2),"picks are counted by sound for the candidate list");
    }
    {
        std::vector<NewWord> word{{L"ㄒㄧㄠˇ-ㄇㄧㄥˊ",L"曉明"}};
        check(reopened.saveWords(word) && reopened.learnedWords().empty() && reopened.allWords().size()==1,"a new word waits for a second use");
        check(reopened.saveWords(word) && reopened.allWords().size()==1 && reopened.allWords()[0].hits==2,"using it again counts, without a second entry");
        check(reopened.learnedWords().size()==1 && reopened.learnedWords()[0].text==L"曉明","a word used twice is offered");
        check(!reopened.forgetWord(L"ㄒㄧㄠˇ-ㄇㄧㄥˊ",L"小明") && reopened.forgetWord(L"ㄒㄧㄠˇ-ㄇㄧㄥˊ",L"曉明") && reopened.allWords().empty(),"one learned word can be removed");
        check(reopened.saveWords(word),"and learned again");
        check(reopened.saveSentence({L"sca",L"你好嗎"}) && reopened.saveSentence({L"jb",L"我們"}) && reopened.saveSentence({L"sca",L"你好嗎"}),"sentences are kept");
        auto recent=reopened.sentences();
        std::vector<Text> expected;
        for(wchar_t mark=0x2460;mark<0x2460+28;++mark){
            Text symbol(1,mark);
            check(reopened.saveSymbol(symbol),"record a circled number");
            expected.insert(expected.begin(),symbol);
        }
        expected.resize(27);
        check(reopened.recentSymbols()==expected,"only the newest 27 symbols are retained");
        auto oldest=expected.back();
        expected.pop_back();expected.insert(expected.begin(),oldest);
        check(reopened.saveSymbol(oldest) && reopened.recentSymbols()==expected,"picking again moves the symbol to the front");
        expected.pop_back();expected.insert(expected.begin(),L"😀");
        check(reopened.saveSymbol(L"😀") && Learning(argv[2]).recentSymbols()==expected,"a fresh connection reads the same order and the full emoji");
        check(reopened.enable(false) && reopened.recentSymbols().empty() && !reopened.saveSymbol(L"︽"),"disabled learning hides symbols and ignores picks");
        check(reopened.enable(true) && reopened.recentSymbols()==expected,"enabling learning restores the saved list");
        check(recent.size()==2 && recent[0].text==L"你好嗎" && recent[1].text==L"我們","the latest sentence comes first, once");
        check(replaced(L"請理小名回電",L"好，請",L"李曉明",L"回電。")==Text(L"理小名"),"a word typed over part of a dictation is a fix");
        check(replaced(L"他們來了",L"",L"她",L"們來了")==Text(L"他"),"a fix at the start of a dictation");
        check(!replaced(L"他們來了",L"今天",L"她們",L"出門"),"a word typed somewhere else is not a fix");
        check(!replaced(L"我們明天見",L"我們明天見",L"好嗎",L""),"text typed after a dictation is not a fix");
        check(!replaced(L"我們明天見",L"我們明天",L"後",L"見"),"text added inside a dictation is not a fix");
        check(!replaced(L"他們來了",L"",L"他",L"們來了"),"typing the same word again is not a fix");
        check(reopened.saveFix({L"理小名",L"李曉明",L"ㄌㄧˇ-ㄒㄧㄠˇ-ㄇㄧㄥˊ"}) && reopened.fixes().size()==1 && reopened.fixes()[0].said==L"理小名","a fix is kept");
        check(reopened.recentDictations().empty(),"no dictations until the voice window writes them");
        auto file=std::filesystem::path(argv[2])/L"export.tsv";
        size_t before=reopened.ranks().size();
        sqlite3* fault=nullptr;
        auto dbPath=std::filesystem::path(argv[2])/L"preferences.sqlite3";
        check(sqlite3_open16(dbPath.c_str(),&fault)==SQLITE_OK,"open synthetic learning DB for fault injection");
        check(sqlite3_exec(fault,"CREATE TRIGGER fail_symbol_clear BEFORE DELETE ON recent_symbols "
              "BEGIN SELECT RAISE(ABORT,'synthetic delete failure'); END",nullptr,nullptr,nullptr)==SQLITE_OK,
              "inject a symbol-specific delete failure");
        check(before>0 && !reopened.clear(),"clear reports a symbol-specific delete failure");
        check(reopened.ranks().size()==before && reopened.recentSymbols()==expected,
              "failed clear rolls back earlier deletes and preserves symbol history");
        check(sqlite3_exec(fault,"DROP TRIGGER fail_symbol_clear",nullptr,nullptr,nullptr)==SQLITE_OK,
              "remove the synthetic fault before normal clear");
        auto schema=[&](const char* sql,const char* msg){
            check(sqlite3_exec(fault,sql,nullptr,nullptr,nullptr)==SQLITE_OK,msg);
        };
        schema("ALTER TABLE recent_symbols RENAME TO saved_symbols; "
               "CREATE VIEW recent_symbols AS SELECT mark FROM saved_symbols","install an incompatible optional object");
        check(!reopened.clear() && reopened.ranks().size()==before,
              "clear rejects a same-name view and rolls back earlier deletes");
        schema("DROP VIEW recent_symbols; ALTER TABLE saved_symbols RENAME TO recent_symbols","restore symbol table");
        check(reopened.recentSymbols()==expected,"view rejection preserves the underlying symbols in order");
        schema("ALTER TABLE choices RENAME TO saved_choices","hide the required choices table");
        check(!reopened.clear() && reopened.recentSymbols()==expected,"clear rejects a missing required choices table");
        schema("ALTER TABLE saved_choices RENAME TO choices","restore required choices table");
        check(reopened.ranks().size()==before,"schema failures retain earlier learning");
        sqlite3_close(fault);
        check(before>0 && reopened.exportTo(file) && reopened.clear() && reopened.ranks().empty() && reopened.learnedWords().empty() && reopened.sentences().empty() && reopened.fixes().empty(),"clear removes choices, words, sentences and fixes");
        check(reopened.importFrom(file) && reopened.ranks().size()==before,"import restores exported choices");
        check(Learning(argv[2]).recentSymbols().empty(),"clear removes all saved symbols");
    }
    check(reopened.forget(L"我愛",L"ㄋㄧˇ"),"forget one reading and context");
    Input phrase(dict);phrase.setPreferences(std::make_shared<const Preferences>(reopened.ranks()));
    type(phrase,"JI394SU3");phrase.press(VK_DOWN);
    auto list=phrase.choices();
    check(list.size()>=2 && list[1].text==L"我愛妳","whole phrase alternative");
    phrase.press('2');auto edits=phrase.corrections();
    check(edits.size()==1 && edits[0].key.first==L"我愛" && edits[0].choice==L"妳","whole phrase stores just changed character");
    phrase.press(VK_BACK);
    check(phrase.corrections().empty(),"undo selection before commit");
    check(reopened.enable(false) && reopened.ranks().empty() && reopened.favorites().empty(),"off ignores existing preferences");
    return 0;
}
