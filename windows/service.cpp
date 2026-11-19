#include "profile.h"
#include "engine.h"
#include <memory>
#include <initguid.h>
#include <inputscope.h>
#include "candidates.h"
#include "composition_line.h"
#include "journal.h"
using namespace tingzi;
namespace {
HMODULE module;
long liveObjects;
// How often each repeating problem has happened in this program, for the record; see noteOften.
long keysRefused,keysWhileEnding,staleEdits,endedByProgram,editFailures,activations;
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
        static const bool extended=(data.loadPairs(folder(module)+L"pairs.txt"),true);
        static const bool user=(refreshUserWords(data,Learning().learnedWords()),true);
        (void)user;(void)extended;(void)loaded;return data;
    }
    // Words added in the settings page, kept with Shift+Enter or Ctrl and a digit, or learned from
    // picks reach programs that are already open: each new composition checks whether they changed.
    static void refreshUserWords(Dictionary& data,const std::vector<NewWord>& learned){
        struct Seen {
            std::filesystem::file_time_type phrases,templates; std::vector<std::pair<Text,Text>> learned;
            bool operator==(const Seen& o) const {return phrases==o.phrases && templates==o.templates && learned==o.learned;}
        };
        static std::optional<Seen> last;
        auto stamp=[](const Text& file){std::error_code error;auto at=std::filesystem::last_write_time(std::filesystem::path(file),error);return error?std::filesystem::file_time_type{}:at;};
        Seen now{stamp(userPhraseFile()),stamp(userTemplateFile()),{}};
        for(const auto& word:learned)now.learned.push_back({word.reading,word.text});
        if(last && *last==now)return;
        last=std::move(now);
        auto words=Dictionary::readUser(userPhraseFile());
        for(auto& word:Dictionary::readTemplates(userTemplateFile()))words.push_back(std::move(word));
        for(const auto& word:learned)words.push_back({word.reading,word.text,userPhraseScore});
        data.setUserWords(words);
    }
    std::unique_ptr<Input> input;
    // A standalone symbol list is selectable without any text in the composition.
    static bool menuOnly(const Input& state){return state.selecting() && !state.active();}
    static bool atSelection(ITfContext* context,TfEditCookie ec,ITfRange* range){
        TF_SELECTION selection{};ULONG fetched=0;
        const HRESULT hr=context->GetSelection(ec,TF_DEFAULT_SELECTION,1,&selection,&fetched);
        ComPtr<ITfRange> current;current.Attach(selection.range);
        BOOL sameStart=FALSE,sameEnd=FALSE;
        return SUCCEEDED(hr) && fetched==1 && current &&
            SUCCEEDED(range->IsEqualStart(ec,current.Get(),TF_ANCHOR_START,&sameStart)) && sameStart &&
            SUCCEEDED(range->IsEqualEnd(ec,current.Get(),TF_ANCHOR_END,&sameEnd)) && sameEnd;
    }
    bool chinese=true, shiftPending=false;
    // Letters, digits and the space bar come out full-width while this is on; Shift+Space switches it.
    bool fullWidth=false;
    static bool otherModifier(){
        return ((GetKeyState(VK_CONTROL)|GetKeyState(VK_MENU)|GetKeyState(VK_LWIN)|GetKeyState(VK_RWIN))&0x8000)!=0;
    }
    bool widthSwitch(ITfContext* c,WPARAM k) const {
        return k==VK_SPACE && (GetKeyState(VK_SHIFT)&0x8000) && !otherModifier() && !losingFocus && input && c && !input->active() && !input->selecting();
    }
    // The key as one full-width character for the engine to commit, or 0 when it is not a printable key.
    WPARAM literal(ITfContext* c,WPARAM k,LPARAM flags) const {
        if(!fullWidth || losingFocus || !input || !c || input->active() || input->selecting() || otherModifier())return 0;
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
    bool canSwitch() const {return input && !input->active() && !input->selecting() && !shortcut();}
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
        RECT anchor=*rect;
        if(menuOnly(*input))line.hide();
        else anchor=line.show(module,parent,anchor,*input,lineSize());
        if(!input->selecting()){hideMenu();return;}
        menu.show(module,parent,anchor,*input,learningStatus,this,[](void* object,WPARAM key){
            auto self=static_cast<Service*>(object);
            if(!self->composingIn)return;
            auto session=new KeyEdit(self,self->composingIn.Get(),key,self->compositionGeneration);HRESULT result;
            self->composingIn->RequestEditSession(self->client,session,TF_ES_ASYNCDONTCARE|TF_ES_READWRITE,&result);
            session->Release();
        });
    }
    // The key as the engine takes it, or 0 when it is not for the engine.
    WPARAM engineKey(ITfContext* c,WPARAM key) const {
        if(losingFocus || !chinese || !input || !c || (composingIn && c!=composingIn.Get()))return 0;
        if((GetKeyState(VK_MENU)|GetKeyState(VK_LWIN)|GetKeyState(VK_RWIN)) & 0x8000)return 0;
        if(GetKeyState(VK_CONTROL)&0x8000){
            // Ctrl alone with a digit keeps characters as a word; every other Ctrl shortcut belongs to the application.
            const WPARAM kept=controlKey|key;
            return !(GetKeyState(VK_SHIFT)&0x8000) && input->wants(kept)?kept:0;
        }
        const WPARAM typed=withShift(key);
        return input->wants(typed)?typed:0;
    }
    bool wants(ITfContext* c,WPARAM key) const {return engineKey(c,key)!=0;}
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
        if(data.byCode.empty()){note(L"找不到注音詞庫，只開啟語音浮窗");startFloat();return S_OK;}
#endif
        ComPtr<ITfKeystrokeMgr> keys;
        HRESULT hr=mgr->QueryInterface(IID_PPV_ARGS(&keys));if(FAILED(hr))return hr;
        hr=keys->AdviseKeyEventSink(id,this,TRUE);
        if(SUCCEEDED(hr)){
            ComPtr<ITfSource> source;
            hr=mgr->QueryInterface(IID_PPV_ARGS(&source));
            if(SUCCEEDED(hr))hr=source->AdviseSink(IID_ITfThreadMgrEventSink,static_cast<ITfThreadMgrEventSink*>(this),&focusCookie);
            if(FAILED(hr)){keys->UnadviseKeyEventSink(id);note(L"無法接收焦點變化 "+hex(hr));return hr;}
            thread=mgr;client=id;input=std::make_unique<Input>(data);
            Text mode;
            if(flags&TF_TMAE_SECUREMODE)mode+=L"，安全模式";
            if(flags&TF_TMAE_CONSOLE)mode+=L"，主控台";
            if(flags&TF_TMAE_COMLESS)mode+=L"，無 COM";
            noteOften(activations,L"啟用"+mode);
        } else note(L"無法接收按鍵 "+hex(hr));
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
        const WPARAM wanted=engineKey(c,k);
        const WPARAM typed=wanted?wanted:literal(c,k,flags);
        *eat=FALSE;if(!typed)return S_OK;
        auto session=new KeyEdit(this,c,typed);HRESULT result=E_FAIL;
        HRESULT hr=c->RequestEditSession(client,session,TF_ES_SYNC|TF_ES_READWRITE,&result);
        session->Release();*eat=SUCCEEDED(hr)&&result==S_OK;
        // A key the input method wanted but could not take goes to the program as a plain letter.
        if(FAILED(hr))noteOften(keysRefused,L"程式沒有讓輸入法處理按鍵 "+hex(hr)+(input && input->active()?L"，正在組字":L"")+L"，這個鍵直接交給程式");
        else if(result!=S_OK && result!=S_FALSE)noteOften(editFailures,L"處理按鍵失敗 "+hex(result)+L"，這個鍵直接交給程式");
        return S_OK;
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
        if(which==composition.Get()){
            if(input && input->active())noteOften(endedByProgram,L"程式在組字途中結束了組字，未送出的注音被清掉");
            hideCompositionUI();detachLayout();composition.Reset();composingIn.Reset();++compositionGeneration;losingFocus=false;if(input)input->clear();}return S_OK;
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
        if(c==composingIn.Get() && !losingFocus && composition && input && (input->active() || input->selecting()) &&
           SUCCEEDED(composition->GetRange(&range)))present(ec,c,range.Get());
        return S_OK;
    }
    // Up to `most` characters on one side of `range`.
    static Text beside(TfEditCookie ec,ITfRange* range,bool ahead,LONG most){
        ComPtr<ITfRange> part;
        if(FAILED(range->Clone(&part)) || FAILED(part->Collapse(ec,ahead?TF_ANCHOR_END:TF_ANCHOR_START)))return {};
        LONG moved=0;
        if(FAILED(ahead?part->ShiftEnd(ec,most,&moved,nullptr):part->ShiftStart(ec,-most,&moved,nullptr)))return {};
        std::vector<wchar_t> buffer(size_t(most)+1);ULONG got=0;
        if(FAILED(part->GetText(ec,0,buffer.data(),ULONG(most),&got)))return {};
        return Text(buffer.data(),std::min<ULONG>(got,ULONG(most)));
    }
    // When the committed word replaced part of a recent dictation, notes what it replaced for the
    // voice window to learn from. `range` holds the committed text as it appears in the document.
    void noteFix(TfEditCookie ec,ITfRange* range,const Text& shown,const NewWord& word){
        Learning store;
        if(!store.enabled())return;
        auto recent=store.recentDictations();
        if(recent.empty())return;
        size_t longest=0;for(const auto& said:recent)longest=std::max(longest,said.size());
        const LONG most=LONG(std::min<size_t>(longest+8,4096));
        const Text before=beside(ec,range,false,most),after=beside(ec,range,true,most);
        for(const auto& said:recent)
            if(auto part=replaced(said,before,shown,after)){store.saveFix({*part,word.text,word.reading});return;}
        // Some programs do not let an input method read their text; the voice window then compares with its dictations itself.
        if(before.empty() && after.empty())store.saveFix({L"",word.text,word.reading});
    }
    HRESULT edit(ITfContext* c,WPARAM key,TfEditCookie ec,std::optional<unsigned long long> expected={}){
        if(expected && *expected!=compositionGeneration){if(key)noteOften(staleEdits,L"點選的候選字晚到，那一段組字已經結束，略過");return S_FALSE;}
        if(key && losingFocus){noteOften(keysWhileEnding,L"還在結束上一段組字時收到按鍵，這個鍵直接交給程式");return S_FALSE;}
        if(key)startFloat();
        if(!input || (composingIn && composingIn.Get()!=c))return S_FALSE;
        if(!key && !composition)return S_FALSE;
        Input next=*input;
        const bool wasMenuOnly=menuOnly(*input);
        const bool opensMenuOnly=!composition && !input->active() && !input->selecting() && key==VK_OEM_3;
        if(!composition && !key)return S_FALSE;
        ComPtr<ITfRange> range;
        HRESULT hr;
        if(!composition) {
            ComPtr<ITfInsertAtSelection> insert;
            hr=c->QueryInterface(IID_PPV_ARGS(&insert));if(FAILED(hr))return hr;
            hr=insert->InsertTextAtSelection(ec,TF_IAS_QUERYONLY,nullptr,0,&range);if(FAILED(hr)){noteOften(editFailures,L"讀不到游標位置 "+hex(hr));return hr;}
            ComPtr<ITfContextComposition> cc;
            hr=c->QueryInterface(IID_PPV_ARGS(&cc));if(FAILED(hr))return hr;
            if(opensMenuOnly && !allowsOverlay(c,ec,range.Get()))return S_FALSE;
            HWND parent=nullptr;const auto anchor=locate(ec,c,range.Get(),parent);
            if(opensMenuOnly && !anchor)return S_FALSE; // Do not consume a key for an invisible list.
            hr=cc->StartComposition(ec,range.Get(),this,&composition);
            if(FAILED(hr) || !composition){noteOften(editFailures,L"無法開始組字 "+hex(hr));hideCompositionUI();return FAILED(hr)?hr:E_FAIL;}
            composingIn=c;
            ++compositionGeneration;
            losingFocus=false;
            ComPtr<ITfSource> source;
            if(SUCCEEDED(c->QueryInterface(IID_PPV_ARGS(&source))) &&
               FAILED(source->AdviseSink(IID_ITfTextLayoutSink,static_cast<ITfTextLayoutSink*>(this),&layoutCookie)))
                layoutCookie=TF_INVALID_COOKIE;
        } else {hr=composition->GetRange(&range);if(FAILED(hr))return hr;}
        // A click within the same client may move its caret without a focus notification.
        // Dismiss without a write, and give the key back, rather than insert at the old range.
        const bool staleMenu=key && wasMenuOnly && (!allowsOverlay(c,ec,range.Get()) || !atSelection(c,ec,range.Get()));
        if(staleMenu)key=0;
        bool allowed=learnable(c,ec,range.Get(),activationFlags);
        if(!allowed){next.setPreferences(nullptr);next.setFavorites(nullptr);next.setSentences(nullptr);learningStatus=1;}
        else if(!input->active() && !wasMenuOnly && !opensMenuOnly){
            Learning store;
            bool enabled=store.enabled();learningStatus=enabled?3:2;
            refreshUserWords(dictionary(),store.learnedWords());
            next.setPreferences(enabled?std::make_shared<const Preferences>(store.ranks()):nullptr);
            next.setFavorites(enabled?std::make_shared<const Favorites>(store.favorites()):nullptr);
            next.setSentences(enabled?std::make_shared<const std::vector<Sentence>>(store.sentences()):nullptr);
        }
        auto edits=next.corrections();
        auto fresh=next.newWords();
        Text commit;
        if(key)commit=next.press(key);else next.clear();
        auto added=next.addedWord();
        auto said=next.committedSentence();
        auto typed=next.typedWord();
        // A word kept with Ctrl and a digit is saved at once; the sentence is still being typed.
        if(added && allowed)appendUserPhrase(added->reading,added->text);
        // Opening, navigating or dismissing an empty symbol list must not replace
        // the application's active selection with an empty string or move its caret.
        if(!((wasMenuOnly || menuOnly(next)) && !next.active() && commit.empty())){
            Text text=next.active()?next.preview():commit;
            hr=range->SetText(ec,0,text.c_str(),(LONG)text.size());if(FAILED(hr)){noteOften(editFailures,L"無法寫入組字區 "+hex(hr));return hr;}
            ComPtr<ITfRange> caret;
            if(SUCCEEDED(range->Clone(&caret))){
                // The application's own caret follows the one inside the sentence.
                LONG moved=0;
                if(next.active() && next.caretInside() && SUCCEEDED(caret->Collapse(ec,TF_ANCHOR_START)))caret->ShiftEnd(ec,LONG(next.caretOffset()),&moved,nullptr);
                if(SUCCEEDED(caret->Collapse(ec,TF_ANCHOR_END))){TF_SELECTION s{caret.Get(),{TF_AE_NONE,FALSE}};c->SetSelection(ec,1,&s);}
            }
        }
        input=std::make_unique<Input>(std::move(next));
        if(input->active() || input->selecting()){present(ec,c,range.Get(),true);return S_OK;}
        hideCompositionUI();auto done=composition;hr=done->EndComposition(ec);
        if(FAILED(hr))noteOften(editFailures,L"無法結束組字 "+hex(hr));
        if(SUCCEEDED(hr)){detachLayout();composition.Reset();composingIn.Reset();++compositionGeneration;losingFocus=false;if(allowed && !commit.empty()){
                if(!edits.empty() || !fresh.empty() || said){Learning store;store.save(edits);store.saveWords(fresh);if(said)store.saveSentence(*said);}
                if(typed)noteFix(ec,range.Get(),commit,*typed);
            }}
        return staleMenu && SUCCEEDED(hr) ? S_FALSE : hr;
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
