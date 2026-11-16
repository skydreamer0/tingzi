#include "profile.h"
#include <string>
#include <cstdio>
#include <cwchar>
#include <filesystem>
#include <shlobj.h>
namespace fs=std::filesystem;
namespace {
std::wstring guid(REFGUID id){wchar_t s[40];StringFromGUID2(id,s,40);return s;}
HRESULT report(const wchar_t* step,HRESULT hr){wprintf(L"%ls: 0x%08lX\n",step,(unsigned long)hr);return hr;}
#ifdef TINGZI_STANDALONE
HRESULT userInput(bool install){
    HMODULE input=LoadLibraryExW(L"input.dll",nullptr,LOAD_LIBRARY_SEARCH_SYSTEM32);
    if(!input)return HRESULT_FROM_WIN32(GetLastError());
    auto apply=reinterpret_cast<BOOL(WINAPI*)(LPCWSTR,DWORD)>(GetProcAddress(input,"InstallLayoutOrTip"));
    wchar_t lang[16];swprintf_s(lang,L"0x%04x:",languageId);
    auto id=std::wstring(lang)+guid(serviceId)+guid(profileId);
    HRESULT hr=!apply?HRESULT_FROM_WIN32(ERROR_PROC_NOT_FOUND):apply(id.c_str(),install?0:1)?S_OK:E_FAIL;
    FreeLibrary(input);return report(install?L"Add current-user input method":L"Remove current-user input method",hr);
}
#endif
HRESULT createClass(HKEY hive,const fs::path& dll,bool& created) {
    auto key=L"Software\\Classes\\CLSID\\"+guid(serviceId);
    HKEY cls=nullptr,sub=nullptr;DWORD disposition;
    auto err=RegCreateKeyExW(hive,key.c_str(),0,nullptr,0,KEY_READ|KEY_WRITE,nullptr,&cls,&disposition);
    if(err)return HRESULT_FROM_WIN32(err);
    created=disposition==REG_CREATED_NEW_KEY;
    err=RegCreateKeyExW(cls,L"InprocServer32",0,nullptr,0,KEY_READ|KEY_WRITE,nullptr,&sub,nullptr);
    if(!err){
        auto path=dll.wstring();wchar_t previous[32768]{};DWORD bytes=sizeof(previous);
        auto existing=RegGetValueW(sub,nullptr,nullptr,RRF_RT_REG_SZ,nullptr,previous,&bytes);
        if(!created && existing==ERROR_SUCCESS && _wcsicmp(previous,path.c_str())!=0)err=ERROR_ALREADY_EXISTS;
        if(!err)err=RegSetValueExW(sub,nullptr,0,REG_SZ,(BYTE*)path.c_str(),DWORD((path.size()+1)*sizeof(wchar_t)));
        if(!err)err=RegSetValueExW(sub,L"ThreadingModel",0,REG_SZ,(BYTE*)L"Apartment",sizeof(L"Apartment"));
        RegCloseKey(sub);
    }
    RegCloseKey(cls);return HRESULT_FROM_WIN32(err);
}
HRESULT removeClass(HKEY hive){
    auto key=L"Software\\Classes\\CLSID\\"+guid(serviceId);
    auto error=RegDeleteTreeW(hive,key.c_str());
    return error==ERROR_FILE_NOT_FOUND?S_OK:HRESULT_FROM_WIN32(error);
}
// GetProfile returns E_FAIL both for absence and for lookup failures. Prove absence by enumeration.
HRESULT profilePresent(ITfInputProcessorProfileMgr* profiles,bool& found) {
    ComPtr<IEnumTfInputProcessorProfiles> list;
    HRESULT hr=profiles->EnumProfiles(languageId,&list);
    if(FAILED(hr))return hr;
    TF_INPUTPROCESSORPROFILE p{};ULONG count=0;
    while((hr=list->Next(1,&p,&count))==S_OK && count==1){
        if(p.clsid==serviceId && p.guidProfile==profileId){found=true;return S_OK;}
    }
    found=false;return hr==S_FALSE?S_OK:E_FAIL;
}
HRESULT run(const std::wstring& action,const fs::path& dll){
    // Machine COM registration must only refer to a DLL in a protected installation directory.
    wchar_t pf[32768]{};DWORD bytes=sizeof(pf);
    LSTATUS known=RegGetValueW(HKEY_LOCAL_MACHINE,L"SOFTWARE\\Microsoft\\Windows\\CurrentVersion",L"ProgramFilesDir",
        RRF_RT_REG_SZ|RRF_SUBKEY_WOW6464KEY,nullptr,pf,&bytes);
    if(known!=ERROR_SUCCESS)return report(L"Read Program Files directory",HRESULT_FROM_WIN32(known));
#ifdef TINGZI_STANDALONE
    const fs::path installDir=fs::path(pf)/L"TingziStandaloneRC";
#else
    const fs::path installDir=fs::path(pf)/L"Tingzi";
#endif
    bool machine=_wcsicmp(dll.parent_path().c_str(),installDir.c_str())==0;
    HKEY hive=machine?HKEY_LOCAL_MACHINE:HKEY_CURRENT_USER;
    ComPtr<ITfInputProcessorProfileMgr> profiles;
    HRESULT hr=CoCreateInstance(CLSID_TF_InputProcessorProfiles,nullptr,CLSCTX_INPROC_SERVER,IID_PPV_ARGS(&profiles));
    if(FAILED(hr))return report(L"TSF profile manager",hr);
    ComPtr<ITfCategoryMgr> categories;
    hr=CoCreateInstance(CLSID_TF_CategoryMgr,nullptr,CLSCTX_INPROC_SERVER,IID_PPV_ARGS(&categories));
    if(FAILED(hr))return report(L"TSF category manager",hr);
    TF_INPUTPROCESSORPROFILE profile{};
    auto inspect=[&](){return profiles->GetProfile(TF_PROFILETYPE_INPUTPROCESSOR,languageId,serviceId,profileId,nullptr,&profile);};
    if(action==L"status"){
        hr=report(L"GetProfile",inspect());
        if(hr==S_OK) wprintf(L"profile flags = 0x%lx\n",profile.dwFlags);
        ComPtr<ITfTextInputProcessorEx> service;
        HRESULT com=report(L"Open text service",CoCreateInstance(serviceId,nullptr,CLSCTX_INPROC_SERVER,IID_PPV_ARGS(&service)));
#ifdef TINGZI_STANDALONE
        ComPtr<ITfInputProcessorProfiles> userProfiles;
        BOOL enabled=FALSE;
        HRESULT user=profiles.As(&userProfiles);
        if(SUCCEEDED(user))user=userProfiles->IsEnabledLanguageProfile(serviceId,languageId,profileId,&enabled);
        report(L"Current-user profile enabled",SUCCEEDED(user)&&enabled?S_OK:E_FAIL);
        if(FAILED(user)||!enabled)return E_FAIL;
#endif
        return hr==S_OK?com:E_FAIL;
    }
    if(action==L"remove"){
#ifdef TINGZI_STANDALONE
        hr=userInput(false);if(FAILED(hr))return hr;
#endif
        bool present=false;
        hr=report(L"Inspect profile before removal",profilePresent(profiles.Get(),present));
        if(FAILED(hr))return hr;
        if(present){
            hr=report(L"UnregisterProfile",profiles->UnregisterProfile(serviceId,languageId,profileId,0));
            if(FAILED(hr))return hr;
            hr=profilePresent(profiles.Get(),present);
            if(FAILED(hr)||present)return report(L"Verify profile removal",FAILED(hr)?hr:E_FAIL);
        }
        categories->UnregisterCategory(serviceId,GUID_TFCAT_TIP_KEYBOARD,serviceId);
        return report(L"Remove COM class",removeClass(hive));
    }
    if(action!=L"install")return E_INVALIDARG;
    if(!fs::is_regular_file(dll))return HRESULT_FROM_WIN32(ERROR_FILE_NOT_FOUND);
    bool created=false;hr=report(L"Register COM class",createClass(hive,dll,created));
    if(FAILED(hr)){if(created)removeClass(hive);return hr;}
    bool previousProfile=inspect()==S_OK;
    ComPtr<ITfTextInputProcessorEx> service;
    hr=report(L"Open text service",CoCreateInstance(serviceId,nullptr,CLSCTX_INPROC_SERVER,IID_PPV_ARGS(&service)));
    if(FAILED(hr)){if(created)removeClass(hive);return hr;}
    // Register the profile first. A failed registration leaves the user's other keyboards alone.
    auto icon=dll.wstring();
    hr=report(L"RegisterProfile",profiles->RegisterProfile(serviceId,languageId,profileId,profileName,
        ARRAYSIZE(profileName)-1,icon.c_str(),(ULONG)icon.size(),0,nullptr,0,TRUE,0));
    if(SUCCEEDED(hr))hr=report(L"Register keyboard category",categories->RegisterCategory(serviceId,GUID_TFCAT_TIP_KEYBOARD,serviceId));
#ifdef TINGZI_STANDALONE
    if(SUCCEEDED(hr)){
        ComPtr<ITfInputProcessorProfiles> userProfiles;
        hr=profiles.As(&userProfiles);
        if(SUCCEEDED(hr))hr=report(L"Enable current-user language profile",userProfiles->EnableLanguageProfile(serviceId,languageId,profileId,TRUE));
        if(SUCCEEDED(hr))hr=userInput(true);
    }
#endif
    if(SUCCEEDED(hr) && inspect()!=S_OK)hr=report(L"Verify registered profile",HRESULT_FROM_WIN32(ERROR_NOT_FOUND));
    if(FAILED(hr)){
        if(!previousProfile && inspect()==S_OK){
            categories->UnregisterCategory(serviceId,GUID_TFCAT_TIP_KEYBOARD,serviceId);
            profiles->UnregisterProfile(serviceId,languageId,profileId,0);
        }
        if(created)removeClass(hive);
    }
    return hr;
}
}
int wmain(int argc,wchar_t** argv){
    if(argc!=2){wprintf(L"Tingzi IME setup: install | status | remove\n");return 2;}
    wchar_t path[32768];GetModuleFileNameW(nullptr,path,32768);
    HRESULT hr=CoInitializeEx(nullptr,COINIT_APARTMENTTHREADED);
    if(FAILED(hr))return 1;
#ifdef TINGZI_STANDALONE
    const wchar_t* dll=sizeof(void*)==8?L"TingziStandaloneService64.dll":L"TingziStandaloneService32.dll";
#else
    const wchar_t* dll=sizeof(void*)==8?L"TingziTextService64.dll":L"TingziTextService32.dll";
#endif
    try {hr=run(argv[1],fs::path(path).parent_path()/dll);}catch(const std::exception&){hr=E_FAIL;}
    CoUninitialize();return FAILED(hr)?1:0;
}
