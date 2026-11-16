#include "profile.h"
#include <filesystem>
#include <fstream>
#include <algorithm>
#include <cstdio>
namespace fs=std::filesystem;
namespace {
struct Binding {
    HKEY key=nullptr;
    const wchar_t* name=nullptr;
    std::wstring old;
    ~Binding(){if(key)RegCloseKey(key);}
    bool read(REGSAM view,const std::wstring& path,const wchar_t* valueName){
        name=valueName;
        if(RegOpenKeyExW(HKEY_LOCAL_MACHINE,path.c_str(),0,view|KEY_QUERY_VALUE|KEY_SET_VALUE,&key))return false;
        wchar_t value[32768];DWORD n=sizeof(value);
        if(RegGetValueW(key,nullptr,name,RRF_RT_REG_SZ,nullptr,value,&n))return false;
        old=value;return true;
    }
    bool open(REGSAM view,const fs::path& original,const fs::path& next) {
        wchar_t cls[40];StringFromGUID2(serviceId,cls,40);
        auto path=std::wstring(L"SOFTWARE\\Classes\\CLSID\\")+cls+L"\\InprocServer32";
        if(!read(view,path,nullptr))return false;
        for(auto dir:{L"v0.6.0",L"v0.6.1",L"v0.6.2"})if(!_wcsicmp(old.c_str(),(original.parent_path()/dir/original.filename()).c_str()))return true;
        return !_wcsicmp(old.c_str(),original.c_str()) || !_wcsicmp(old.c_str(),next.c_str());
    }
    bool set(const fs::path& value){
        return RegSetValueExW(key,name,0,REG_SZ,(const BYTE*)value.c_str(),(DWORD)((value.native().size()+1)*sizeof(wchar_t)))==ERROR_SUCCESS;
    }
};
bool safeDirectory(const fs::path& path){
    DWORD attr=GetFileAttributesW(path.c_str());
    return attr!=INVALID_FILE_ATTRIBUTES && (attr&FILE_ATTRIBUTE_DIRECTORY) && !(attr&FILE_ATTRIBUTE_REPARSE_POINT);
}
bool copyProduct(const fs::path& from,const fs::path& to){
    if(CopyFileW(from.c_str(),to.c_str(),TRUE))return true;
    if(GetLastError()!=ERROR_FILE_EXISTS || (GetFileAttributesW(to.c_str())&FILE_ATTRIBUTE_REPARSE_POINT))return false;
    if(fs::file_size(from)!=fs::file_size(to))return false;
    std::ifstream a(from,std::ios::binary),b(to,std::ios::binary);
    return a&&b&&std::equal(std::istreambuf_iterator<char>(a),{},std::istreambuf_iterator<char>(b));
}
int run(){
    wchar_t pf[MAX_PATH],self[32768];DWORD bytes=sizeof(pf);
    if(RegGetValueW(HKEY_LOCAL_MACHINE,L"SOFTWARE\\Microsoft\\Windows\\CurrentVersion",L"ProgramFilesDir",
        RRF_RT_REG_SZ|RRF_SUBKEY_WOW6464KEY,nullptr,pf,&bytes))return 2;
    GetModuleFileNameW(nullptr,self,32768);
    const fs::path source=fs::path(self).parent_path(),root=fs::path(pf)/L"Tingzi",dest=root/L"v0.6.2";
    if(!safeDirectory(root))return 3;
    Binding x64,x86;
    auto a=L"TingziTextService64.dll",b=L"TingziTextService32.dll";
    if(!x64.open(KEY_WOW64_64KEY,root/a,dest/a)||!x86.open(KEY_WOW64_32KEY,root/b,dest/b))return 4;
    Binding icon64,icon32,name64,name32;
    wchar_t cls[40],profile[40];StringFromGUID2(serviceId,cls,40);StringFromGUID2(profileId,profile,40);
    auto key=std::wstring(L"SOFTWARE\\Microsoft\\CTF\\TIP\\")+cls+L"\\LanguageProfile\\0x00000404\\"+profile;
    if(!icon64.read(KEY_WOW64_64KEY,key,L"IconFile") || !icon32.read(KEY_WOW64_32KEY,key,L"IconFile"))return 4;
    if(!name64.read(KEY_WOW64_64KEY,key,L"Description") || !name32.read(KEY_WOW64_32KEY,key,L"Description"))return 4;
    {
        fs::create_directory(dest);if(!safeDirectory(dest))return 5;
        fs::create_directory(dest/L"TingziFloat");if(!safeDirectory(dest/L"TingziFloat"))return 5;
        const wchar_t* names[]={L"README.md",L"THIRD-PARTY-NOTICES.txt",L"TingziFloat\\LICENSE",L"TingziFloat\\NAudio-LICENSE.txt",L"TingziFloat\\codex-asr.exe",L"TingziFloat\\codex-asr-LICENSE.txt",a,b,L"TingziPad.exe",L"TingziKeyboard.exe",L"TingziLearning.exe",L"TingziSetup64.exe",L"TingziSetup32.exe",L"TingziFloat\\TingziFloat.exe",
          L"TingziFloat\\TingziFloat.dll",L"TingziFloat\\TingziFloat.runtimeconfig.json",L"TingziFloat\\TingziFloat.deps.json",
          L"TingziFloat\\NAudio.dll",L"TingziFloat\\NAudio.Core.dll",L"TingziFloat\\NAudio.WinMM.dll",L"TingziFloat\\NAudio.Wasapi.dll",
          L"TingziFloat\\NAudio.Midi.dll",L"TingziFloat\\NAudio.Asio.dll",L"TingziFloat\\NAudio.WinForms.dll"};
        for(const auto* name:names)if(!copyProduct(source/name,dest/name))return 6;
        const fs::path active=fs::path(x64.old).parent_path();
        const fs::path previous=active;
        const wchar_t* local[]={L"Tingzi.cin",L"phrases.txt",L"TingziFloat\\voice-root.txt",L"McBopomofo-LICENSE.txt"};
        for(const auto* name:local){
            if(fs::exists(dest/name))continue;
            // An existing word list or voice root wins over the one shipped in the package.
            auto from=fs::is_regular_file(previous/name)?previous/name:source/name;
            if(fs::is_regular_file(from) && !copyProduct(from,dest/name))return 6;
        }

    }
    auto to=dest;
    if(!safeDirectory(to)||!fs::is_regular_file(to/a)||!fs::is_regular_file(to/b))return 5;
    Binding* keys[]={&x64,&x86,&icon64,&icon32,&name64,&name32};
    fs::path values[]={to/a,to/b,to/a,to/a,fs::path(profileName),fs::path(profileName)};
    for(int i=0;i<6;++i)if(!keys[i]->set(values[i])){
        bool restored=true;while(i>0){--i;if(!keys[i]->set(keys[i]->old))restored=false;}
        return restored?8:11;
    }
    return 0;
}
}
int wmain(int argc,wchar_t** argv){
    if(argc!=2 || wcscmp(argv[1],L"upgrade"))return 1;
    HANDLE mutex=CreateMutexW(nullptr,FALSE,L"Local\\TingziUpgrade");
    if(!mutex)return 9;
    DWORD wait=WaitForSingleObject(mutex,5000);int rc=9;
    if(wait==WAIT_OBJECT_0||wait==WAIT_ABANDONED){
        try{rc=run();}catch(const fs::filesystem_error&){rc=10;}
        ReleaseMutex(mutex);
    }
    CloseHandle(mutex);wprintf(L"Tingzi upgrade result: %d\n",rc);return rc;
}
