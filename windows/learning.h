#pragma once
#include <windows.h>
#include <shlobj.h>
#include <winsqlite/winsqlite3.h>
#include <algorithm>
#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <map>
#include <string>
#include <vector>

namespace tingzi {
using Text=std::wstring;
using LearnKey=std::pair<Text,Text>;
using Preferences=std::map<LearnKey,Text>;
struct Correction { LearnKey key; Text choice; };
struct NewWord { Text reading, text; };
// A learned word with how many times it has been committed.
struct LearnedWord { Text reading, text; int hits; };
// A sentence the user committed, with the first key of each syllable, so typing only those keys recalls it.
struct Sentence { Text codes, text; };
inline Text localLearningDir(){
    PWSTR path=nullptr;
    if(FAILED(SHGetKnownFolderPath(FOLDERID_LocalAppData,KF_FLAG_NO_PACKAGE_REDIRECTION,nullptr,&path)))return {};
#ifdef TINGZI_STANDALONE
    Text dir=Text(path)+L"\\TingziStandaloneRC";
#else
    Text dir=Text(path)+L"\\Tingzi";
#endif
    CoTaskMemFree(path);return dir;
}
class Learning {
    sqlite3* db=nullptr;
    static void bind(sqlite3_stmt* s,int i,const Text& t){
        sqlite3_bind_text16(s,i,t.data(),(int)(t.size()*sizeof(wchar_t)),SQLITE_TRANSIENT);
    }
    bool exec(const char* sql) const {return db && sqlite3_exec(db,sql,nullptr,nullptr,nullptr)==SQLITE_OK;}
public:
    explicit Learning(const Text& dir=localLearningDir(),bool create=false){
        if(dir.empty())return;
        std::error_code error;
        auto path=std::filesystem::path(dir)/L"preferences.sqlite3";
        if(!create && !std::filesystem::is_regular_file(path,error))return;
        if(create)std::filesystem::create_directories(dir,error);
        if(error || sqlite3_open16(path.c_str(),&db)!=SQLITE_OK)return;
        sqlite3_busy_timeout(db,350);
        if(create){
            exec("CREATE TABLE IF NOT EXISTS settings(enabled INTEGER NOT NULL)");
            exec("CREATE UNIQUE INDEX IF NOT EXISTS one_setting ON settings((1))");
            exec("INSERT OR IGNORE INTO settings VALUES(0)");
            exec("CREATE TABLE IF NOT EXISTS choices(context TEXT NOT NULL,reading TEXT NOT NULL,choice TEXT NOT NULL,hits INTEGER NOT NULL,PRIMARY KEY(context,reading)) WITHOUT ROWID");
        }
    }
    Learning(const Learning&)=delete;
    ~Learning(){if(db)sqlite3_close(db);}
    bool enable(bool value){return exec(value?"UPDATE settings SET enabled=1":"UPDATE settings SET enabled=0");}
    bool enabled() const {
        sqlite3_stmt* s=nullptr;
        bool ok=db && sqlite3_prepare_v2(db,"SELECT enabled FROM settings",-1,&s,nullptr)==SQLITE_OK && sqlite3_step(s)==SQLITE_ROW && sqlite3_column_int(s,0)==1;
        sqlite3_finalize(s);return ok;
    }
    Preferences ranks() const {
        Preferences ranks;
        sqlite3_stmt* s=nullptr;
        if(db && sqlite3_prepare_v2(db,"SELECT context,reading,choice FROM choices WHERE hits>=2 AND (SELECT enabled FROM settings)=1",-1,&s,nullptr)==SQLITE_OK){
            while(sqlite3_step(s)==SQLITE_ROW){
                auto col=[&](int n){return static_cast<const wchar_t*>(sqlite3_column_text16(s,n));};
                ranks[{col(0),col(1)}]=col(2);
            }
        }
        sqlite3_finalize(s);return ranks;
    }
    bool save(const std::vector<Correction>& edits){
        if(edits.empty())return true;
        if(!exec("BEGIN IMMEDIATE"))return false;
        bool ok=enabled();
        sqlite3_stmt* s=nullptr;
        const char* sql="INSERT INTO choices VALUES(?,?,?,1) ON CONFLICT(context,reading) DO UPDATE SET hits=CASE WHEN choice=excluded.choice THEN MIN(hits+1,2) ELSE 1 END,choice=excluded.choice";
        ok=ok && sqlite3_prepare_v2(db,sql,-1,&s,nullptr)==SQLITE_OK;
        // A given context counts once per committed composition.
        Preferences distinct;for(const auto& edit: edits)distinct[edit.key]=edit.choice;
        for(const auto& [key,choice]:distinct){
            if(!ok)break;
            bind(s,1,key.first);bind(s,2,key.second);bind(s,3,choice);
            ok=sqlite3_step(s)==SQLITE_DONE;sqlite3_reset(s);
        }
        sqlite3_finalize(s);
        if(ok && exec("COMMIT"))return true;
        exec("ROLLBACK");return false;
    }
    // Characters the user picked one after another become a word the first time the run is committed.
    bool saveWords(const std::vector<NewWord>& words){
        if(words.empty())return true;
        if(!enabled())return false;
        exec("CREATE TABLE IF NOT EXISTS words(reading TEXT NOT NULL,text TEXT NOT NULL,hits INTEGER NOT NULL,PRIMARY KEY(reading,text)) WITHOUT ROWID");
        sqlite3_stmt* s=nullptr;
        bool ok=sqlite3_prepare_v2(db,"INSERT INTO words VALUES(?,?,1) ON CONFLICT(reading,text) DO UPDATE SET hits=MIN(hits+1,9)",-1,&s,nullptr)==SQLITE_OK;
        for(const auto& word:words){
            if(!ok)break;
            bind(s,1,word.reading);bind(s,2,word.text);
            ok=sqlite3_step(s)==SQLITE_DONE;sqlite3_reset(s);
        }
        sqlite3_finalize(s);return ok;
    }
    std::vector<NewWord> learnedWords() const {
        std::vector<NewWord> words;
        sqlite3_stmt* s=nullptr;
        if(db && sqlite3_prepare_v2(db,"SELECT reading,text FROM words WHERE (SELECT enabled FROM settings)=1",-1,&s,nullptr)==SQLITE_OK){
            while(sqlite3_step(s)==SQLITE_ROW){
                auto col=[&](int n){return static_cast<const wchar_t*>(sqlite3_column_text16(s,n));};
                words.push_back({col(0),col(1)});
            }
        }
        sqlite3_finalize(s);return words;
    }
    // Every learned word, whether or not learning is switched on, for the user to review.
    std::vector<LearnedWord> allWords() const {
        std::vector<LearnedWord> words;
        sqlite3_stmt* s=nullptr;
        if(db && sqlite3_prepare_v2(db,"SELECT reading,text,hits FROM words ORDER BY text,reading",-1,&s,nullptr)==SQLITE_OK){
            while(sqlite3_step(s)==SQLITE_ROW){
                auto col=[&](int n){return static_cast<const wchar_t*>(sqlite3_column_text16(s,n));};
                words.push_back({col(0),col(1),sqlite3_column_int(s,2)});
            }
        }
        sqlite3_finalize(s);return words;
    }
    bool forgetWord(const Text& reading,const Text& text){
        sqlite3_stmt* s=nullptr;
        bool ok=db && sqlite3_prepare_v2(db,"DELETE FROM words WHERE reading=? AND text=?",-1,&s,nullptr)==SQLITE_OK;
        if(ok){bind(s,1,reading);bind(s,2,text);ok=sqlite3_step(s)==SQLITE_DONE && sqlite3_changes(db)==1;}
        sqlite3_finalize(s);return ok;
    }
    // Keeps the hundred most recent sentences; saying one again moves it to the front.
    bool saveSentence(const Sentence& said){
        if(said.codes.empty() || said.text.empty() || !enabled())return false;
        exec("CREATE TABLE IF NOT EXISTS sentences(text TEXT PRIMARY KEY,codes TEXT NOT NULL,seq INTEGER NOT NULL) WITHOUT ROWID");
        sqlite3_stmt* s=nullptr;
        bool ok=sqlite3_prepare_v2(db,"INSERT OR REPLACE INTO sentences VALUES(?,?,(SELECT COALESCE(MAX(seq),0)+1 FROM sentences))",-1,&s,nullptr)==SQLITE_OK;
        if(ok){bind(s,1,said.text);bind(s,2,said.codes);ok=sqlite3_step(s)==SQLITE_DONE;}
        sqlite3_finalize(s);
        exec("DELETE FROM sentences WHERE seq<=(SELECT MAX(seq) FROM sentences)-100");
        return ok;
    }
    // Most recent first; empty while learning is off.
    std::vector<Sentence> sentences() const {
        std::vector<Sentence> list;
        sqlite3_stmt* s=nullptr;
        if(db && sqlite3_prepare_v2(db,"SELECT codes,text FROM sentences WHERE (SELECT enabled FROM settings)=1 ORDER BY seq DESC",-1,&s,nullptr)==SQLITE_OK){
            while(sqlite3_step(s)==SQLITE_ROW){
                auto col=[&](int n){return static_cast<const wchar_t*>(sqlite3_column_text16(s,n));};
                list.push_back({col(0),col(1)});
            }
        }
        sqlite3_finalize(s);return list;
    }
    bool clear(){bool ok=exec("DELETE FROM choices");exec("DELETE FROM words");exec("DELETE FROM sentences");return ok;}
    // Reads the file exportTo writes; rows already present are replaced.
    bool importFrom(const std::filesystem::path& path){
        std::ifstream in(path,std::ios::binary);
        if(!db || !in || !exec("BEGIN IMMEDIATE"))return false;
        sqlite3_stmt* s=nullptr;
        bool ok=sqlite3_prepare_v2(db,"INSERT INTO choices VALUES(?,?,?,?) ON CONFLICT(context,reading) DO UPDATE SET choice=excluded.choice,hits=excluded.hits",-1,&s,nullptr)==SQLITE_OK;
        std::string line;bool first=true;
        while(ok && std::getline(in,line)){
            if(!line.empty() && line.back()=='\r')line.pop_back();
            if(first){first=false;if(line.rfind("context\t",0)==0)continue;}
            std::vector<std::string> fields;
            for(size_t at=0;;){
                auto tab=line.find('\t',at);
                fields.push_back(line.substr(at,tab==std::string::npos?tab:tab-at));
                if(tab==std::string::npos)break;
                at=tab+1;
            }
            if(fields.size()!=4 || fields[0].empty() || fields[1].empty() || fields[2].empty())continue;
            for(int i=0;i<3;++i)sqlite3_bind_text(s,i+1,fields[i].data(),(int)fields[i].size(),SQLITE_TRANSIENT);
            sqlite3_bind_int(s,4,std::clamp(std::atoi(fields[3].c_str()),1,2));
            ok=sqlite3_step(s)==SQLITE_DONE;sqlite3_reset(s);
        }
        sqlite3_finalize(s);
        if(ok && exec("COMMIT"))return true;
        exec("ROLLBACK");return false;
    }
    bool forget(const Text& context,const Text& reading){
        sqlite3_stmt* s=nullptr;
        bool ok=db && sqlite3_prepare_v2(db,"DELETE FROM choices WHERE context=? AND reading=?",-1,&s,nullptr)==SQLITE_OK;
        if(ok){bind(s,1,context);bind(s,2,reading);ok=sqlite3_step(s)==SQLITE_DONE;}
        sqlite3_finalize(s);return ok;
    }
    bool exportTo(const std::filesystem::path& path){
        if(std::filesystem::exists(path))return false;
        sqlite3_stmt* s=nullptr;
        if(!db || sqlite3_prepare_v2(db,"SELECT context,reading,choice,hits FROM choices ORDER BY context,reading",-1,&s,nullptr)!=SQLITE_OK)return false;
        std::ofstream out(path,std::ios::binary);out<<"context\treading\tchoice\tcommits\n";
        int rc;
        while((rc=sqlite3_step(s))==SQLITE_ROW){
            for(int i=0;i<3;++i)out<<reinterpret_cast<const char*>(sqlite3_column_text(s,i))<<'\t';
            out<<sqlite3_column_int(s,3)<<'\n';
        }
        sqlite3_finalize(s);out.close();return rc==SQLITE_DONE && bool(out);
    }
};
}
