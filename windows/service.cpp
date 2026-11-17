#include "profile.h"
#include "engine.h"
#include <memory>
#include <initguid.h>
#include <inputscope.h>
#include "candidates.h"
#include "composition_line.h"
using namespace tingzi;
namespace {
HMODULE module;
long liveObjects;
class Service;
class KeyEdit final:public ITfEditSession {
    long refs=1;
    ComPtr<ITfContext> context;
    Service* owner;
    std::optional<WPARAM> key;
    std::optional<unsigned long long> generation;
public:
    KeyEdit(Service* owner,ITfContext* context,std::optional<WPARAM> key,std::optional<unsigned long long> generation={});
    ~KeyEdit();
    STDMETHODIMP QueryInterface(REFIID iid,void** ptr) override {
        if(!ptr)return E_POINTER;
        *ptr=nullptr;
        if(iid!=IID_IUnknown && iid!=IID_ITfEditSession)return E_NOINTERFACE;
        *ptr=static_cast<ITfEditSession*>(this);AddRef();return S_OK;
    }
    STDMETHODIMP_(ULONG) AddRef() override {return InterlockedIncrement(&refs);}
    STDMETHODIMP_(ULONG) Release() override {long n=InterlockedDecrement(&refs);if(!n)delete this;return n;}
    STDMETHODIMP DoEditSession(TfEditCookie ec) override;
};
class Service final:public ITfTextInputProcessorEx, public ITfKeyEventSink, public ITfCompositionSink, public ITfThreadMgrEventSink, public ITfTextLayoutSink {
    long refs=1;
    ComPtr<ITfThreadMgr> thread;
    TfClientId client=TF_CLIENTID_NULL;
    DWORD focusCookie=TF_INVALID_COOKIE;
    ComPtr<ITfComposition> composition;
    ComPtr<ITfContext> composingIn;
    CandidateWindow menu;
    CompositionLine line;
    DWORD layoutCookie=TF_INVALID_COOKIE;
    std::optional<RECT> lastAnchor;
    HWND anchorParent=nullptr;
    bool losingFocus=false;
    unsigned long long compositionGeneration=0;
    DWORD activationFlags=0;
    DWORD learningStatus=0;
    Dictionary& dictionary() {
        static Dictionary data;
        static const bool loaded=data.load(folder(module));
        static const bool extended=(data.loadUser(userPhraseFile()),data.loadTemplates(userTemplateFile()),true);
        static const bool learned=([]{for(const auto& word:Learning().learnedWords())data.addUser(word.reading,word.text);}(),true);
        (void)learned;
        (void)extended;
        (void)loaded;return data;
    }
    std::unique_ptr<Input> input;
    bool chinese=true, shiftPending=false;
    // Letters, digits and the space bar come out full-width while this is on; Shift+Space switches it.
    bool fullWidth=false;
    static bool otherModifier(){
        return ((GetKeyState(VK_CONTROL)|GetKeyState(VK_MENU)|GetKeyState(VK_LWIN)|GetKeyState(VK_RWIN))&0x8000)!=0;
    }
    bool widthSwitch(ITfContext* c,WPARAM k) const {
        return k==VK_SPACE && (GetKeyState(VK_SHIFT)&0x8000) && !otherModifier() && !losingFocus && input && c && !input->active();
    }
    // The key as one full-width character for the engine to commit, or 0 when it is not a printable key.
    WPARAM literal(ITfContext* c,WPARAM k,LPARAM flags) const {
        if(!fullWidth || losingFocus || !input || !c || input->active() || otherModifier())return 0;
        BYTE state[256];wchar_t typed[4];
        // Flag 4 asks Windows to translate without disturbing dead-key state.
        if(!GetKeyboardState(state) || ToUnicode(UINT(k),UINT((flags>>16)&0xff),state,typed,4,4)!=1)return 0;
        if(typed[0]==L' ')return literalKey|0x3000;
        return typed[0]>0x20 && typed[0]<0x7f?literalKey|WPARAM(typed[0]+0xfee0):0;
    }
    static WPARAM withShift(WPARAM k){return (GetKeyState(VK_SHIFT)&0x8000)?(k|shiftKey):k;}
    static bool shift(WPARAM k){return k==VK_SHIFT || k==VK_LSHIFT || k==VK_RSHIFT;}
    static bool shortcut(){
        return ((GetKeyState(VK_CONTROL)|GetKeyState(VK_MENU)|GetKeyState(VK_LWIN)|GetKeyState(VK_RWIN))&0x8000)!=0;
    }
    bool canSwitch() const {return input && !input->active() && !shortcut();}
    void indicateMode(){
#ifndef TINGZI_STANDALONE
        for(auto name:{chinese?L"Local\\TingziChinese":L"Local\\TingziEnglish",fullWidth?L"Local\\TingziFullWidth":L"Local\\TingziHalfWidth"}){
            auto event=OpenEventW(EVENT_MODIFY_STATE,FALSE,name);
            if(event){SetEvent(event);CloseHandle(event);}
        }
#endif
    }
    void startFloat(){
#ifndef TINGZI_STANDALONE
        if(activationFlags&(TF_TMAE_SECUREMODE|TF_TMAE_COMLESS|TF_TMAE_CONSOLE))return;
        static bool tried=false;
        if(tried)return;
        tried=true;
        HANDLE running=OpenMutexW(SYNCHRONIZE,FALSE,L"Local\\TingziFloat020");
        if(running){CloseHandle(running);return;}
        HANDLE token=nullptr;
        if(!OpenProcessToken(GetCurrentProcess(),TOKEN_QUERY,&token))return;
        TOKEN_ELEVATION elevation{};DWORD bytes=0,container=0;
        bool normal=GetTokenInformation(token,TokenElevation,&elevation,sizeof(elevation),&bytes) && !elevation.TokenIsElevated
            && GetTokenInformation(token,TokenIsAppContainer,&container,sizeof(container),&bytes) && !container;
        CloseHandle(token);if(!normal)return;
        Text dir=folder(module)+L"TingziFloat";Text app=dir+L"\\TingziFloat.exe";
        STARTUPINFOW info{sizeof(info)}; PROCESS_INFORMATION proc{};
        if(CreateProcessW(app.c_str(),nullptr,nullptr,nullptr,FALSE,0,nullptr,dir.c_str(),&info,&proc)){
            CloseHandle(proc.hThread);CloseHandle(proc.hProcess);
        }
#endif
    }
    void hideMenu(){menu.hide();}
    void hideCompositionUI(){hideMenu();line.hide();lastAnchor.reset();anchorParent=nullptr;}
    void detachLayout(){
        if(layoutCookie!=TF_INVALID_COOKIE && composingIn){
            ComPtr<ITfSource> source;
            if(SUCCEEDED(composingIn.As(&source)))source->UnadviseSink(layoutCookie);
        }
        layoutCookie=TF_INVALID_COOKIE;
    }
    bool allowsOverlay(ITfContext* c,TfEditCookie ec,ITfRange* range) const {
        if(losingFocus || (activationFlags&(TF_TMAE_SECUREMODE|TF_TMAE_CONSOLE|TF_TMAE_COMLESS)))return false;
        TF_STATUS status{};
        if(SUCCEEDED(c->GetStatus(&status)) && (status.dwDynamicFlags&TF_SD_READONLY))return false;
        HWND focus=GetFocus();wchar_t cls[64]{};
        GetClassNameW(focus,cls,64);
        if((!_wcsicmp(cls,L"Edit") || !_wcsnicmp(cls,L"RichEdit",8)) &&
            (GetWindowLongPtrW(focus,GWL_STYLE)&(ES_PASSWORD|ES_READONLY)))return false;
        ComPtr<ITfReadOnlyProperty> prop;
        if(FAILED(c->GetAppProperty(GUID_PROP_INPUTSCOPE,&prop)) || !prop)return true;
        VARIANT value;VariantInit(&value);
        HRESULT hr=prop->GetValue(ec,range,&value);
        ComPtr<ITfInputScope> scope;
        if(SUCCEEDED(hr) && value.vt==VT_UNKNOWN && value.punkVal)value.punkVal->QueryInterface(IID_PPV_ARGS(&scope));
        VariantClear(&value);
        if(!scope)return true;
        InputScope* types=nullptr;UINT n=0;
        if(FAILED(scope->GetInputScopes(&types,&n)))return false;
        bool ok=true;
        for(UINT i=0;i<n;++i){
            switch(types[i]){
            case IS_PASSWORD:case IS_PRIVATE:case IS_NUMERIC_PASSWORD:case IS_NUMERIC_PIN:
            case IS_ALPHANUMERIC_PIN:case IS_ALPHANUMERIC_PIN_SET: ok=false;break;
            default:break;
            }
        }
        CoTaskMemFree(types);return ok;
    }
    static bool learnable(ITfContext* c,TfEditCookie ec,ITfRange* range,DWORD flags) {
        if(flags & (TF_TMAE_SECUREMODE|TF_TMAE_COMLESS|TF_TMAE_CONSOLE))return false;
        wchar_t file[MAX_PATH]{};
        GetModuleFileNameW(nullptr,file,MAX_PATH);
        Text exe=std::filesystem::path(file).filename().wstring();
        if(!_wcsicmp(exe.c_str(),L"Code.exe") || !_wcsicmp(exe.c_str(),L"devenv.exe") || !_wcsicmp(exe.c_str(),L"WindowsTerminal.exe"))return false;
        HWND focus=GetFocus();wchar_t cls[128]{};GetClassNameW(focus,cls,128);
        if((!_wcsicmp(cls,L"Edit") || !_wcsnicmp(cls,L"RichEdit",8)) &&
            (GetWindowLongPtrW(focus,GWL_STYLE)&ES_PASSWORD) && SendMessageW(focus,EM_GETPASSWORDCHAR,0,0))return false;
        bool plain=!_wcsicmp(exe.c_str(),L"notepad.exe");
        ComPtr<ITfReadOnlyProperty> property;
        if(FAILED(c->GetAppProperty(GUID_PROP_INPUTSCOPE,&property)) || !property)return plain;
        VARIANT v;VariantInit(&v);
        HRESULT hr=property->GetValue(ec,range,&v);
        if(v.vt==VT_EMPTY){VariantClear(&v);return plain;}
        ComPtr<ITfInputScope> scope;
        if(SUCCEEDED(hr) && v.vt==VT_UNKNOWN && v.punkVal) v.punkVal->QueryInterface(IID_PPV_ARGS(&scope));
        VariantClear(&v);if(!scope)return false;
        InputScope* types=nullptr;UINT n=0;
        if(FAILED(scope->GetInputScopes(&types,&n)))return false;
        bool allow=n?true:plain;
        for(UINT i=0;i<n;++i) if(types[i]!=IS_DEFAULT && types[i]!=IS_TEXT && types[i]!=IS_CHAT && types[i]!=IS_CHAT_WITHOUT_EMOJI && types[i]!=IS_NATIVE_SCRIPT)allow=false;
        CoTaskMemFree(types);return allow;
    }
    std::optional<RECT> locate(TfEditCookie ec,ITfContext* c,ITfRange* range,HWND& parent){
        ComPtr<ITfContextView> view;
        if(FAILED(c->GetActiveView(&view)) || !view)return {};
        view->GetWnd(&parent);
        if(!parent)parent=GetFocus();
        if(!parent)return {};
        if(anchorParent!=parent){lastAnchor.reset();anchorParent=parent;}
        RECT rect{}; BOOL clipped=FALSE;ComPtr<ITfRange> end;
        if(SUCCEEDED(range->Clone(&end)) && SUCCEEDED(end->Collapse(ec,TF_ANCHOR_END)) &&
           SUCCEEDED(view->GetTextExt(ec,end.Get(),&rect,&clipped)) && !clipped && rect.bottom>rect.top)
            return lastAnchor=rect;
        if(lastAnchor)return lastAnchor;
        GUITHREADINFO info{sizeof(info)};
        if(GetGUIThreadInfo(GetCurrentThreadId(),&info) && info.hwndCaret &&
           GetAncestor(info.hwndCaret,GA_ROOT)==GetAncestor(parent,GA_ROOT) && info.rcCaret.bottom>info.rcCaret.top){
            rect=info.rcCaret;MapWindowPoints(info.hwndCaret,nullptr,reinterpret_cast<POINT*>(&rect),2);
            return lastAnchor=rect;
        }
        return {};
    }
    void present(TfEditCookie ec,ITfContext* c,ITfRange* range,bool retryMissing=false){
        if(!allowsOverlay(c,ec,range)){hideCompositionUI();return;}
        HWND parent=nullptr;auto rect=locate(ec,c,range,parent);
        if(!rect){
            line.hide();hideMenu();
            if(retryMissing){
                auto read=new KeyEdit(this,c,std::nullopt);HRESULT result;
                c->RequestEditSession(client,read,TF_ES_ASYNC|TF_ES_READ,&result);
                read->Release();
            }
            return;
        }
        RECT anchor=line.show(module,parent,*rect,*input,lineSize());
        if(!input->selecting()){hideMenu();return;}
        menu.show(module,parent,anchor,*input,learningStatus,this,[](void* object,WPARAM key){
            auto self=static_cast<Service*>(object);
            if(!self->composingIn)return;
            auto session=new KeyEdit(self,self->composingIn.Get(),key,self->compositionGeneration);HRESULT result;
            self->composingIn->RequestEditSession(self->client,session,TF_ES_ASYNCDONTCARE|TF_ES_READWRITE,&result);
            session->Release();
        });
    }
    bool wants(ITfContext* c,WPARAM key) const {
        if(losingFocus || !chinese || !input || !c || (composingIn && c!=composingIn.Get()))return false;
        if((GetKeyState(VK_CONTROL)|GetKeyState(VK_MENU)|GetKeyState(VK_LWIN)|GetKeyState(VK_RWIN)) & 0x8000)return false;
        return input->wants(withShift(key));
    }
    void endLater(){
        hideCompositionUI();
        if(!composition || !composingIn)return;
        losingFocus=true;
        auto c=composingIn; auto session=new KeyEdit(this,c.Get(),0,compositionGeneration);HRESULT hr=E_FAIL;
        c->RequestEditSession(client,session,TF_ES_ASYNCDONTCARE|TF_ES_READWRITE,&hr);session->Release();
    }
public:
    Service(){InterlockedIncrement(&liveObjects);}
    ~Service(){hideCompositionUI();InterlockedDecrement(&liveObjects);}
    STDMETHODIMP QueryInterface(REFIID iid,void** out) override {
        if(!out)return E_POINTER;
        *out=nullptr;
        if(iid==IID_IUnknown || iid==IID_ITfTextInputProcessor || iid==IID_ITfTextInputProcessorEx)*out=static_cast<ITfTextInputProcessorEx*>(this);
        else if(iid==IID_ITfKeyEventSink)*out=static_cast<ITfKeyEventSink*>(this);
        else if(iid==IID_ITfCompositionSink)*out=static_cast<ITfCompositionSink*>(this);
        else if(iid==IID_ITfThreadMgrEventSink)*out=static_cast<ITfThreadMgrEventSink*>(this);
        else if(iid==IID_ITfTextLayoutSink)*out=static_cast<ITfTextLayoutSink*>(this);
        else return E_NOINTERFACE;
        AddRef();return S_OK;
    }
    STDMETHODIMP_(ULONG) AddRef() override {return InterlockedIncrement(&refs);}
    STDMETHODIMP_(ULONG) Release() override {long n=InterlockedDecrement(&refs);if(!n)delete this;return n;}
    STDMETHODIMP Activate(ITfThreadMgr* mgr,TfClientId id) override {return ActivateEx(mgr,id,0);}
    STDMETHODIMP ActivateEx(ITfThreadMgr* mgr,TfClientId id,DWORD flags) override {
        activationFlags=flags;
        if(!mgr)return E_INVALIDARG;
        if(thread)return E_UNEXPECTED;
        auto& data=dictionary();
#ifdef TINGZI_STANDALONE
        // The standalone keyboard has no voice window to fall back to.
        if(data.byCode.empty())return HRESULT_FROM_WIN32(ERROR_FILE_NOT_FOUND);
#else
        if(data.byCode.empty()){startFloat();return S_OK;}
#endif
        ComPtr<ITfKeystrokeMgr> keys;
        HRESULT hr=mgr->QueryInterface(IID_PPV_ARGS(&keys));if(FAILED(hr))return hr;
        hr=keys->AdviseKeyEventSink(id,this,TRUE);
        if(SUCCEEDED(hr)){
            ComPtr<ITfSource> source;
            hr=mgr->QueryInterface(IID_PPV_ARGS(&source));
            if(SUCCEEDED(hr))hr=source->AdviseSink(IID_ITfThreadMgrEventSink,static_cast<ITfThreadMgrEventSink*>(this),&focusCookie);
            if(FAILED(hr)){keys->UnadviseKeyEventSink(id);return hr;}
            thread=mgr;client=id;input=std::make_unique<Input>(data);
        }
        return hr;
    }
    STDMETHODIMP Deactivate() override {
        endLater();detachLayout(); ComPtr<ITfKeystrokeMgr> keys;
        ComPtr<ITfSource> source;
        if(thread && focusCookie!=TF_INVALID_COOKIE && SUCCEEDED(thread.As(&source)))source->UnadviseSink(focusCookie);
        focusCookie=TF_INVALID_COOKIE;
        if(thread && SUCCEEDED(thread.As(&keys)))keys->UnadviseKeyEventSink(client);
        thread.Reset();return S_OK;
    }
    STDMETHODIMP OnSetFocus(BOOL foreground) override {shiftPending=false;if(!foreground || losingFocus)endLater();else {startFloat();indicateMode();}return S_OK;}
    STDMETHODIMP OnSetFocus(ITfDocumentMgr* doc,ITfDocumentMgr*) override {
        if(!composingIn)return S_OK;
        ComPtr<ITfContext> next;if(doc)doc->GetTop(&next);
        if(next.Get()!=composingIn.Get())endLater();
        return S_OK;
    }
    STDMETHODIMP OnInitDocumentMgr(ITfDocumentMgr*) override {return S_OK;}
    STDMETHODIMP OnUninitDocumentMgr(ITfDocumentMgr*) override {return S_OK;}
    STDMETHODIMP OnPushContext(ITfContext* c) override {if(composingIn && c!=composingIn.Get())endLater();return S_OK;}
    STDMETHODIMP OnPopContext(ITfContext* c) override {if(c==composingIn.Get())endLater();return S_OK;}
    STDMETHODIMP OnTestKeyDown(ITfContext* c,WPARAM k,LPARAM flags,BOOL* eat) override {
        if(!eat)return E_POINTER;
        if(!shift(k))shiftPending=false;
        *eat=shift(k)?canSwitch():(widthSwitch(c,k) || wants(c,k) || literal(c,k,flags));return S_OK;
    }
    STDMETHODIMP OnKeyDown(ITfContext* c,WPARAM k,LPARAM flags,BOOL* eat) override {
        if(!eat)return E_POINTER;
        if(shift(k)){
            if(!(flags&(1L<<30)))shiftPending=canSwitch();
            *eat=canSwitch();return S_OK;
        }
        shiftPending=false;
        if(widthSwitch(c,k)){fullWidth=!fullWidth;indicateMode();*eat=TRUE;return S_OK;}
        const WPARAM typed=wants(c,k)?withShift(k):literal(c,k,flags);
        *eat=FALSE;if(!typed)return S_OK;
        auto session=new KeyEdit(this,c,typed);HRESULT result=E_FAIL;
        HRESULT hr=c->RequestEditSession(client,session,TF_ES_SYNC|TF_ES_READWRITE,&result);
        session->Release();*eat=SUCCEEDED(hr)&&result==S_OK;return S_OK;
    }
    STDMETHODIMP OnTestKeyUp(ITfContext*,WPARAM k,LPARAM,BOOL* e) override {
        if(!e)return E_POINTER;
        *e=shift(k)&&shiftPending&&canSwitch();return S_OK;
    }
    STDMETHODIMP OnKeyUp(ITfContext*,WPARAM k,LPARAM,BOOL* e) override {
        if(!e)return E_POINTER;
        *e=shift(k)&&shiftPending&&canSwitch();
        if(*e){chinese=!chinese;indicateMode();}
        shiftPending=false;return S_OK;
    }
    STDMETHODIMP OnPreservedKey(ITfContext*,REFGUID,BOOL* e) override {*e=FALSE;return S_OK;}
    STDMETHODIMP OnCompositionTerminated(TfEditCookie,ITfComposition* which) override {
        if(which==composition.Get()){hideCompositionUI();detachLayout();composition.Reset();composingIn.Reset();++compositionGeneration;losingFocus=false;if(input)input->clear();}return S_OK;
    }
    STDMETHODIMP OnLayoutChange(ITfContext* c,TfLayoutCode code,ITfContextView*) override {
        if(c!=composingIn.Get() || !composition || losingFocus)return S_OK;
        if(code==TF_LC_DESTROY){hideCompositionUI();return S_OK;}
        auto session=new KeyEdit(this,c,std::nullopt);HRESULT hr;
        c->RequestEditSession(client,session,TF_ES_ASYNCDONTCARE|TF_ES_READ,&hr);
        session->Release();return S_OK;
    }
    HRESULT refresh(ITfContext* c,TfEditCookie ec){
        ComPtr<ITfRange> range;
        if(c==composingIn.Get() && !losingFocus && composition && input && input->active() &&
           SUCCEEDED(composition->GetRange(&range)))present(ec,c,range.Get());
        return S_OK;
    }
    HRESULT edit(ITfContext* c,WPARAM key,TfEditCookie ec,std::optional<unsigned long long> expected={}){
        if(expected && *expected!=compositionGeneration)return S_FALSE;
        if(key && losingFocus)return S_FALSE;
        if(key)startFloat();
        if(!input || (composingIn && composingIn.Get()!=c))return S_FALSE;
        if(!key && !composition)return S_FALSE;
        Input next=*input;
        if(!composition && !key)return S_FALSE;
        ComPtr<ITfRange> range;
        HRESULT hr;
        if(!composition) {
            ComPtr<ITfInsertAtSelection> insert;
            hr=c->QueryInterface(IID_PPV_ARGS(&insert));if(FAILED(hr))return hr;
            hr=insert->InsertTextAtSelection(ec,TF_IAS_QUERYONLY,nullptr,0,&range);if(FAILED(hr))return hr;
            ComPtr<ITfContextComposition> cc;
            hr=c->QueryInterface(IID_PPV_ARGS(&cc));if(FAILED(hr))return hr;
            HWND parent=nullptr;locate(ec,c,range.Get(),parent);
            hr=cc->StartComposition(ec,range.Get(),this,&composition);
            if(FAILED(hr) || !composition){hideCompositionUI();return FAILED(hr)?hr:E_FAIL;}
            composingIn=c;
            ++compositionGeneration;
            losingFocus=false;
            ComPtr<ITfSource> source;
            if(SUCCEEDED(c->QueryInterface(IID_PPV_ARGS(&source))) &&
               FAILED(source->AdviseSink(IID_ITfTextLayoutSink,static_cast<ITfTextLayoutSink*>(this),&layoutCookie)))
                layoutCookie=TF_INVALID_COOKIE;
        } else {hr=composition->GetRange(&range);if(FAILED(hr))return hr;}
        bool allowed=learnable(c,ec,range.Get(),activationFlags);
        if(!allowed){next.setPreferences(nullptr);learningStatus=1;}
        else if(!input->active()){
            Learning store;
            bool enabled=store.enabled();learningStatus=enabled?3:2;
            next.setPreferences(enabled?std::make_shared<const Preferences>(store.ranks()):nullptr);
        }
        auto edits=next.corrections();
        auto fresh=next.newWords();
        Text commit;
        if(key)commit=next.press(key);else next.clear();
        auto added=next.addedWord();
        Text text=next.active()?next.preview():commit;
        hr=range->SetText(ec,0,text.c_str(),(LONG)text.size());if(FAILED(hr))return hr;
        ComPtr<ITfRange> caret;
        if(SUCCEEDED(range->Clone(&caret))){
            // The application's own caret follows the one inside the sentence.
            LONG moved=0;
            if(next.active() && next.caretInside() && SUCCEEDED(caret->Collapse(ec,TF_ANCHOR_START)))caret->ShiftEnd(ec,LONG(next.caretOffset()),&moved,nullptr);
            if(SUCCEEDED(caret->Collapse(ec,TF_ANCHOR_END))){TF_SELECTION s{caret.Get(),{TF_AE_NONE,FALSE}};c->SetSelection(ec,1,&s);}
        }
        input=std::make_unique<Input>(std::move(next));
        if(input->active()){present(ec,c,range.Get(),true);return S_OK;}
        hideCompositionUI();auto done=composition;hr=done->EndComposition(ec);
        if(SUCCEEDED(hr)){detachLayout();composition.Reset();composingIn.Reset();++compositionGeneration;losingFocus=false;if(allowed && !commit.empty()){
                if(!edits.empty() || !fresh.empty()){Learning store;store.save(edits);store.saveWords(fresh);}
                if(added)appendUserPhrase(added->reading,added->text);
            }}
        return hr;
    }
};
KeyEdit::KeyEdit(Service* s,ITfContext* c,std::optional<WPARAM> k,std::optional<unsigned long long> g):context(c),owner(s),key(k),generation(g){owner->AddRef();}
KeyEdit::~KeyEdit(){owner->Release();}
HRESULT KeyEdit::DoEditSession(TfEditCookie ec){return key?owner->edit(context.Get(),*key,ec,generation):owner->refresh(context.Get(),ec);}
class Factory final:public IClassFactory {
    long refs=1;
public:
    Factory(){InterlockedIncrement(&liveObjects);}
    ~Factory(){InterlockedDecrement(&liveObjects);}
    STDMETHODIMP QueryInterface(REFIID iid,void** p) override {if(!p)return E_POINTER;*p=nullptr;if(iid!=IID_IUnknown&&iid!=IID_IClassFactory)return E_NOINTERFACE;*p=this;AddRef();return S_OK;}
    STDMETHODIMP_(ULONG) AddRef() override {return InterlockedIncrement(&refs);}
    STDMETHODIMP_(ULONG) Release() override {auto n=InterlockedDecrement(&refs);if(!n)delete this;return n;}
    STDMETHODIMP CreateInstance(IUnknown* outer,REFIID iid,void** out) override {if(outer)return CLASS_E_NOAGGREGATION;auto s=new Service;auto hr=s->QueryInterface(iid,out);s->Release();return hr;}
    STDMETHODIMP LockServer(BOOL lock) override {if(lock)InterlockedIncrement(&liveObjects);else InterlockedDecrement(&liveObjects);return S_OK;}
};
}
BOOL WINAPI DllMain(HINSTANCE instance,DWORD reason,LPVOID){if(reason==DLL_PROCESS_ATTACH) module=instance;return TRUE;}
extern "C" HRESULT WINAPI DllCanUnloadNow(){return liveObjects?S_FALSE:S_OK;}
extern "C" HRESULT WINAPI DllGetClassObject(REFCLSID cls,REFIID iid,void** out){if(cls!=serviceId)return CLASS_E_CLASSNOTAVAILABLE;auto f=new Factory;auto hr=f->QueryInterface(iid,out);f->Release();return hr;}
