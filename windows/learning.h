#pragma once
#include <windows.h>
#include <shlobj.h>
#include <winsqlite/winsqlite3.h>
#include <algorithm>
#include <ctime>
#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <map>
#include <optional>
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
// What the user typed over part of a dictation: `said` is the part it replaced, empty when the
// text around it could not be read.
struct Fix { Text said, typed, reading; };
// The part of `dictation` that `typed` replaced, when the text around the committed word shows it
// was typed inside that dictation: `before` must end with the start of the dictation and `after`
// must begin with the rest of it. Words typed elsewhere, or added without replacing anything, give nothing.
inline std::optional<Text> replaced(const Text& dictation,const Text& before,const Text& typed,const Text& after){
    const size_t n=dictation.size();
    if(n<2 || typed.empty())return {};
    size_t head=0,tail=0;
    for(size_t k=std::min(n,before.size());k>0;--k)if(before.compare(before.size()-k,k,dictation,0,k)==0){head=k;break;}
    for(size_t k=std::min(n,after.size());k>0;--k)if(after.compare(0,k,dictation,n-k,k)==0){tail=k;break;}
    // Nothing of the dictation is left on either side, or none of it is missing: text was added, not fixed.
    if(head+tail==0 || head+tail>=n)return {};
    Text said=dictation.substr(head,n-head-tail);
    if(said.empty() || said==typed || said.size()>typed.size()*2+2)return {};
    return said;
}
inline long long secondsNow(){return (long long)std::time(nullptr);}
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
    // Words gained a column for when they were last typed; older files get it here.
    void wordsTable(){
        exec("CREATE TABLE IF NOT EXISTS words(reading TEXT NOT NULL,text TEXT NOT NULL,hits INTEGER NOT NULL,at INTEGER NOT NULL DEFAULT 0,PRIMARY KEY(reading,text)) WITHOUT ROWID");
        sqlite3_stmt* s=nullptr;bool has=false;
        if(sqlite3_prepare_v2(db,"SELECT 1 FROM pragma_table_info('words') WHERE name='at'",-1,&s,nullptr)==SQLITE_OK)has=sqlite3_step(s)==SQLITE_ROW;
        sqlite3_finalize(s);
        if(!has)exec("ALTER TABLE words ADD COLUMN at INTEGER NOT NULL DEFAULT 0");
    }
    // Characters the user picked one after another become a word the first time the run is committed.
    bool saveWords(const std::vector<NewWord>& words){
        if(words.empty())return true;
        if(!enabled())return false;
        wordsTable();
        sqlite3_stmt* s=nullptr;
        bool ok=sqlite3_prepare_v2(db,"INSERT INTO words(reading,text,hits,at) VALUES(?,?,1,?) ON CONFLICT(reading,text) DO UPDATE SET hits=MIN(hits+1,9),at=excluded.at",-1,&s,nullptr)==SQLITE_OK;
        for(const auto& word:words){
            if(!ok)break;
            bind(s,1,word.reading);bind(s,2,word.text);sqlite3_bind_int64(s,3,secondsNow());
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
    // The dictations the voice window inserted in the last ten minutes, newest first. The voice
    // window writes them, and only while learning is on.
    std::vector<Text> recentDictations() const {
        std::vector<Text> list;
        sqlite3_stmt* s=nullptr;
        if(db && sqlite3_prepare_v2(db,"SELECT text FROM dictations WHERE at>=? AND (SELECT enabled FROM settings)=1 ORDER BY at DESC LIMIT 5",-1,&s,nullptr)==SQLITE_OK){
            sqlite3_bind_int64(s,1,secondsNow()-600);
            while(sqlite3_step(s)==SQLITE_ROW)list.push_back(static_cast<const wchar_t*>(sqlite3_column_text16(s,0)));
        }
        sqlite3_finalize(s);return list;
    }
    // Keeps the fifty latest fixes for the voice window to learn from.
    bool saveFix(const Fix& fix){
        if(fix.typed.empty() || fix.reading.empty() || !enabled())return false;
        exec("CREATE TABLE IF NOT EXISTS fixes(at INTEGER NOT NULL,said TEXT NOT NULL,typed TEXT NOT NULL,reading TEXT NOT NULL)");
        sqlite3_stmt* s=nullptr;
        bool ok=sqlite3_prepare_v2(db,"INSERT INTO fixes VALUES(?,?,?,?)",-1,&s,nullptr)==SQLITE_OK;
        if(ok){sqlite3_bind_int64(s,1,secondsNow());bind(s,2,fix.said);bind(s,3,fix.typed);bind(s,4,fix.reading);ok=sqlite3_step(s)==SQLITE_DONE;}
        sqlite3_finalize(s);
        exec("DELETE FROM fixes WHERE rowid NOT IN (SELECT rowid FROM fixes ORDER BY at DESC,rowid DESC LIMIT 50)");
        return ok;
    }
    std::vector<Fix> fixes() const {
        std::vector<Fix> list;
        sqlite3_stmt* s=nullptr;
        if(db && sqlite3_prepare_v2(db,"SELECT said,typed,reading FROM fixes ORDER BY at DESC,rowid DESC",-1,&s,nullptr)==SQLITE_OK){
            while(sqlite3_step(s)==SQLITE_ROW){
                auto col=[&](int n){return static_cast<const wchar_t*>(sqlite3_column_text16(s,n));};
                list.push_back({col(0),col(1),col(2)});
            }
        }
        sqlite3_finalize(s);return list;
    }
    bool clear(){bool ok=exec("DELETE FROM choices");exec("DELETE FROM words");exec("DELETE FROM sentences");exec("DELETE FROM fixes");exec("DELETE FROM dictations");return ok;}
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
