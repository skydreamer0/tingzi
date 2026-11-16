#include "profile.h"
#include <cwchar>
#include <cstdio>
int keyboardAction(const wchar_t* action){
    const bool enable=wcscmp(action,L"enable")==0;
    const bool disable=wcscmp(action,L"disable")==0;
    if(enable||disable) {
        HMODULE dll=LoadLibraryExW(L"input.dll",nullptr,LOAD_LIBRARY_SEARCH_SYSTEM32);
        if(!dll)return 1;
        using Install=BOOL(WINAPI*)(LPCWSTR,DWORD);
        auto install=reinterpret_cast<Install>(GetProcAddress(dll,"InstallLayoutOrTip"));
        wchar_t cls[40],profile[40],tip[96];
        StringFromGUID2(serviceId,cls,40);StringFromGUID2(profileId,profile,40);
        swprintf_s(tip,L"0x0404:%ls%ls",cls,profile);
        BOOL success=install && install(tip,disable?1U:0U);
        FreeLibrary(dll);wprintf(L"InstallLayoutOrTip returned %d\n",success);return success?0:1;
    }
    bool change=wcscmp(action,L"switch")==0;
    if(!change && wcscmp(action,L"status")!=0)return 2;
    HRESULT hr=CoInitializeEx(nullptr,COINIT_APARTMENTTHREADED);
    if(FAILED(hr))return 1;
    ComPtr<ITfInputProcessorProfileMgr> profiles;
    hr=CoCreateInstance(CLSID_TF_InputProcessorProfiles,nullptr,CLSCTX_INPROC_SERVER,IID_PPV_ARGS(&profiles));
    if(SUCCEEDED(hr) && change){
        hr=profiles->ActivateProfile(TF_PROFILETYPE_INPUTPROCESSOR,languageId,serviceId,profileId,nullptr,TF_IPPMF_FORSESSION);
        wprintf(L"ActivateProfile: 0x%08lX\n",(unsigned long)hr);
    }
    TF_INPUTPROCESSORPROFILE active{};
    if(SUCCEEDED(hr))hr=profiles->GetActiveProfile(GUID_TFCAT_TIP_KEYBOARD,&active);
    bool selected=hr==S_OK && active.clsid==serviceId && active.guidProfile==profileId;
    wprintf(L"Session keyboard matches Tingzi: %d (0x%08lX)\n",selected,(unsigned long)hr);
    profiles.Reset();CoUninitialize();return selected?0:1;
}
int wmain(int argc,wchar_t** argv){
    if(argc==1){
        int hr=keyboardAction(L"enable");
        return hr?hr:keyboardAction(L"switch");
    }
    if(argc==2)return keyboardAction(argv[1]);
    wprintf(L"TingziKeyboard: enable | switch | status | disable\n");
    return 2;
}
