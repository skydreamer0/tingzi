#pragma once
#include <windows.h>
#include <shlobj.h>
#include <winsqlite/winsqlite3.h>
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
    bool clear(){return exec("DELETE FROM choices");}
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
