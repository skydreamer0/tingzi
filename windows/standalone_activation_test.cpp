#include "profile.h"
#include <iostream>
static int vendorAttempts=0;
static constexpr CLSID vendorTestId={0x6ae9c62b,0x4354,0x4c22,{0xb1,0x4d,0x06,0x16,0xe4,0xaf,0xd0,0xbd}};
static HRESULT interceptedCreate(REFCLSID cls, IUnknown* outer, DWORD context, REFIID iid, void** out) {
    if(cls==vendorTestId){++vendorAttempts;*out=nullptr;return E_ACCESSDENIED;}
    return CoCreateInstance(cls,outer,context,iid,out);
}
#define CoCreateInstance interceptedCreate
#include "service.cpp"
#undef CoCreateInstance
#include "standalone_test_thread.h"
#include "standalone_test_context.h"
struct ThreadKeyboardState {
    BYTE original[256]{};
    BYTE neutral[256]{};
    bool captured=GetKeyboardState(original)!=FALSE;
    ~ThreadKeyboardState(){if(captured)SetKeyboardState(original);}
};
int wmain(int argc,wchar_t** argv){
    ThreadKeyboardState keyboard;
    if(!keyboard.captured)return 2;
    std::cout<<"Initial thread modifiers ctrl="<<unsigned(keyboard.original[VK_CONTROL])
             <<" shift="<<unsigned(keyboard.original[VK_SHIFT])<<" alt="<<unsigned(keyboard.original[VK_MENU])
             <<" lwin="<<unsigned(keyboard.original[VK_LWIN])<<" rwin="<<unsigned(keyboard.original[VK_RWIN])<<"\n";
    if(!SetKeyboardState(keyboard.neutral))return 2;
    module=GetModuleHandleW(nullptr);
    TestThread mgr;
    ITfTextInputProcessorEx* service=nullptr;
    HMODULE tested=nullptr;
    if(argc==2){
        tested=LoadLibraryW(argv[1]);
        if(!tested)return 2;
        auto get=reinterpret_cast<HRESULT(WINAPI*)(REFCLSID,REFIID,void**)>(GetProcAddress(tested,"DllGetClassObject"));
        ComPtr<IClassFactory> factory;
        if(!get || FAILED(get(serviceId,IID_PPV_ARGS(&factory))) || FAILED(factory->CreateInstance(nullptr,IID_PPV_ARGS(&service))))return 2;
    }else service=new Service;
    ComPtr<ITfCompositionSink> compositionSink;
    service->QueryInterface(IID_PPV_ARGS(&compositionSink));
    HRESULT result=service->ActivateEx(&mgr,42,0);
    bool ok=SUCCEEDED(result)&&vendorAttempts==0&&mgr.advised==1&&mgr.sink;
    std::cout<<(ok?"PASS ":"FAIL ")<<(tested?"packaged DLL factory/sink activation":"included service activation without external services")<<" own sink="<<mgr.advised<<" hr="<<std::hex<<result<<"\n";
    if(!tested)std::cout<<"External service attempts="<<vendorAttempts<<"\n";
    if(!ok){compositionSink.Reset();service->Release();if(tested)FreeLibrary(tested);return 1;}
    auto check=[&](bool condition,const char* name){std::cout<<(condition?"PASS ":"FAIL ")<<name<<"\n";ok=ok&&condition;};
    TestContext first,second;
    auto key=[&](TestContext& c,WPARAM k){BOOL eaten=FALSE;mgr.sink->OnKeyDown(&c,k,0,&eaten);return eaten!=FALSE;};
    BYTE control[256]{};control[VK_CONTROL]=0x80;
    if(!SetKeyboardState(control))return 2;
    check(!key(first,'S')&&first.text.empty(),"thread-local Control modifier passes key through");
    if(!SetKeyboardState(keyboard.neutral))return 2;
    mgr.sink->OnSetFocus(FALSE);mgr.sink->OnSetFocus(TRUE);
    check(key(first,'S')&&first.text==L"ㄋ","first key through TSF key sink");
    key(first,'U');key(first,'3');key(first,VK_DOWN);key(first,'2');
    check(first.text==L"妳"&&first.ended==0,"second candidate remains preedit");
    key(first,VK_RETURN);check(first.text==L"妳"&&first.ended==1,"Enter commits once");
    check(!key(first,VK_RETURN)&&first.ended==1,"second Enter passes through");
    first.text.clear();key(first,'U');key(first,VK_SPACE);key(first,VK_ESCAPE);
    check(first.text.empty()&&first.ended==2,"Esc cancels preedit");
    key(first,'S');mgr.sink->OnSetFocus(FALSE);
    check(!key(first,VK_RETURN)&&first.text==L"ㄋ","losing focus rejects Enter before async cleanup");
    check(!key(second,'S')&&second.text.empty(),"new field cannot receive old preedit");
    first.flush();
    check(first.text.empty()&&key(second,'S')&&second.text==L"ㄋ","old field canceled before new field accepts key");
    mgr.sink->OnSetFocus(FALSE);
    compositionSink->OnCompositionTerminated(1,static_cast<ITfComposition*>(&second));
    second.text.clear();
    check(key(second,'U')&&second.text==L"ㄧ","same field can start after external termination");
    second.flush();
    check(second.text==L"ㄧ","stale focus cleanup cannot erase new composition");
    key(second,VK_ESCAPE);
    second.rejectAsync=true;key(second,'S');mgr.sink->OnSetFocus(FALSE);
    check(!key(second,VK_RETURN)&&second.text==L"ㄋ","failed cleanup cannot commit old field");
    second.rejectAsync=false;mgr.sink->OnSetFocus(TRUE);second.flush();
    check(second.text.empty()&&key(first,'U')&&first.text==L"ㄧ","focus return retries failed cleanup");
    key(first,VK_ESCAPE);
    service->Deactivate();
    first.flush();second.flush();
    ok=ok&&mgr.unadvised==1&&!mgr.sink;
    compositionSink.Reset();service->Release();
    if(tested)FreeLibrary(tested);
    std::cout<<(ok?"PASS ":"FAIL ")<<"standalone TSF contract deactivation\n";
    return ok?0:1;
}
