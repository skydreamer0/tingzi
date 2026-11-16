#include "engine.h"
#include "candidates.h"
#include <imm.h>
#include <commctrl.h>
#include <richedit.h>
#include <inputscope.h>
#include "profile.h"
#include <shellapi.h>
using namespace tingzi;
namespace {
Dictionary dictionary;
Input input(dictionary);
HWND edit, status;
CandidateWindow candidates;
CompositionLine compositionLine;
constexpr UINT lineFirst=410, candidateFirst=420;
constexpr DWORD lineSizes[]={0,36,44,52},candidateSizes[]={20,24,32};
void refreshAppearanceMenu(HWND frame) {
    HMENU menu=GetMenu(frame);
    for(UINT i=0;i<4;++i)CheckMenuItem(menu,lineFirst+i,MF_BYCOMMAND|(lineSizes[i]==DWORD(lineSize())?MF_CHECKED:MF_UNCHECKED));
    for(UINT i=0;i<3;++i)CheckMenuItem(menu,candidateFirst+i,MF_BYCOMMAND|(candidateSizes[i]==DWORD(candidateSize())?MF_CHECKED:MF_UNCHECKED));
}
void addAppearanceMenu(HWND frame){
    HMENU bar=CreateMenu(),line=CreatePopupMenu(),list=CreatePopupMenu();
    for(UINT i=0;i<4;++i)AppendMenuW(line,MF_STRING,lineFirst+i,i?std::to_wstring(lineSizes[i]).c_str():L"關閉");
    for(UINT i=0;i<3;++i)AppendMenuW(list,MF_STRING,candidateFirst+i,std::to_wstring(candidateSizes[i]).c_str());
    AppendMenuW(bar,MF_POPUP,(UINT_PTR)line,L"輸入行字級");
    AppendMenuW(bar,MF_POPUP,(UINT_PTR)list,L"候選字級");SetMenu(frame,bar);refreshAppearanceMenu(frame);
}
void press(WPARAM key);
HFONT uiFont, editFont;
bool chinese = true;
void paintState() {
    Text line = input.active() ? L"組字　" + input.preview() :
        (chinese ? L"中文　輸入注音，按 ↓ 選字，Enter 送字，Esc 取消" : L"英數　按 F6 回到注音");
    SetWindowTextW(status,line.c_str());
    if(!input.active() || GetFocus()!=edit){compositionLine.hide();candidates.hide();return;}
    POINT point{};if(!GetCaretPos(&point) || !ClientToScreen(edit,&point)){compositionLine.hide();candidates.hide();return;}
    RECT caret{point.x,point.y,point.x+1,point.y+MulDiv(28,GetDpiForWindow(edit),96)};
    auto anchor=compositionLine.show(GetModuleHandleW(nullptr),GetParent(edit),caret,input,lineSize());
    if(!input.selecting()){candidates.hide();return;}
    candidates.show(GetModuleHandleW(nullptr),GetParent(edit),anchor,input,0,nullptr,[](void*,WPARAM k){press(k);});
}
void press(WPARAM key){
    if(!input.active()){
        Learning store;
        input.setPreferences(store.enabled()?std::make_shared<const Preferences>(store.ranks()):nullptr);
    }
    auto edits=input.corrections();
    auto fresh=input.newWords();
    Text commit=input.press(key);
    if(!commit.empty()){
        SendMessageW(edit,EM_REPLACESEL,TRUE,(LPARAM)commit.c_str());
        if(!edits.empty() || !fresh.empty()){Learning store;store.save(edits);store.saveWords(fresh);}
        if(auto added=input.addedWord())appendUserPhrase(added->reading,added->text);
    }
    paintState();
}
void switchLanguage() {
    input.clear(); chinese=!chinese; paintState(); SetFocus(edit);
}
LRESULT CALLBACK frame(HWND w,UINT message,WPARAM key,LPARAM arg) {
    switch(message) {
    case WM_CREATE: {
        addAppearanceMenu(w);
        auto instance=reinterpret_cast<LPCREATESTRUCT>(arg)->hInstance;
        DWORD editStyle=WS_CHILD|WS_VISIBLE|WS_TABSTOP|ES_MULTILINE|ES_AUTOVSCROLL|WS_VSCROLL;
        editStyle|=ES_WANTRETURN;
        edit=CreateWindowExW(WS_EX_CLIENTEDGE,L"EDIT",nullptr,editStyle,0,0,0,0,w,nullptr,instance,nullptr);
        if(!edit)return -1;
        ImmAssociateContextEx(edit,nullptr,0);
        status=CreateWindowW(L"STATIC",L"",WS_CHILD|WS_VISIBLE|SS_NOPREFIX,0,0,0,0,w,nullptr,instance,nullptr);
        CreateWindowW(L"BUTTON",L"中／英 (F6)",WS_CHILD|WS_VISIBLE,12,10,110,32,w,(HMENU)1,instance,nullptr);
        CreateWindowW(L"BUTTON",L"複製全文",WS_CHILD|WS_VISIBLE,132,10,108,32,w,(HMENU)2,instance,nullptr);
        uiFont=CreateFontW(-20,0,0,0,FW_NORMAL,0,0,0,DEFAULT_CHARSET,0,0,CLEARTYPE_QUALITY,0,L"Microsoft JhengHei UI");
        editFont=CreateFontW(-26,0,0,0,FW_NORMAL,0,0,0,DEFAULT_CHARSET,0,0,CLEARTYPE_QUALITY,0,L"Microsoft JhengHei UI");
        SendMessageW(edit,WM_SETFONT,(WPARAM)editFont,TRUE);
        SendMessageW(edit,EM_SETLIMITTEXT,0x7ffffffe,0);
        for(HWND child=GetWindow(w,GW_CHILD);child;child=GetWindow(child,GW_HWNDNEXT)) if(child!=edit) SendMessageW(child,WM_SETFONT,(WPARAM)uiFont,TRUE);
        paintState(); SetFocus(edit); return 0;
    }
    case WM_SIZE: {
        int width=LOWORD(arg), height=HIWORD(arg);
        MoveWindow(edit,12,52,width-24,(std::max)(28,height-104),TRUE);
        MoveWindow(status,14,height-46,width-28,40,TRUE);
        return 0;
    }
    case WM_COMMAND:
        if(LOWORD(key)>=lineFirst && LOWORD(key)<lineFirst+4 || LOWORD(key)>=candidateFirst && LOWORD(key)<candidateFirst+3){
            auto id=LOWORD(key);bool isLine=id<candidateFirst;
            DWORD value=isLine?lineSizes[id-lineFirst]:candidateSizes[id-candidateFirst];
            if(RegSetKeyValueW(HKEY_CURRENT_USER,appearanceKey,isLine?L"CompositionDIP":L"CandidateDIP",REG_DWORD,&value,sizeof(value))!=ERROR_SUCCESS)
                MessageBoxW(w,L"無法儲存字級設定。",L"聽字・注音與語音輸入法",MB_ICONERROR);
            refreshAppearanceMenu(w);paintState();return 0;
        }
        if((HWND)arg==edit && (HIWORD(key)==EN_KILLFOCUS || HIWORD(key)==EN_SETFOCUS))paintState();
        if(LOWORD(key)==1 && HIWORD(key)==BN_CLICKED) switchLanguage();
        if(LOWORD(key)==2 && HIWORD(key)==BN_CLICKED) {
            DWORD from,to;SendMessageW(edit,EM_GETSEL,(WPARAM)&from,(LPARAM)&to);
            SendMessageW(edit,EM_SETSEL,0,-1);SendMessageW(edit,WM_COPY,0,0);
            SendMessageW(edit,EM_SETSEL,from,to); SetFocus(edit);
        }
        return 0;
    case WM_SETFOCUS: SetFocus(edit); return 0;
    case WM_ACTIVATE: if(LOWORD(key)==WA_INACTIVE){compositionLine.hide();candidates.hide();}else if(edit && input.active())paintState();break;
    case WM_MOVE: if(edit && input.active())paintState();break;
    case WM_DPICHANGED: {
        const RECT& r=*reinterpret_cast<RECT*>(arg);
        SetWindowPos(w,nullptr,r.left,r.top,r.right-r.left,r.bottom-r.top,SWP_NOZORDER|SWP_NOACTIVATE);
        if(edit && input.active())paintState();return 0;
    }
    case WM_CLOSE:
        if((GetWindowTextLengthW(edit)>0 || input.active()) && MessageBoxW(w,L"關閉後會清除本視窗的文字。確定關閉？",L"聽字・注音與語音輸入法",MB_OKCANCEL|MB_DEFBUTTON2)!=IDOK) return 0;
        DestroyWindow(w);return 0;
    case WM_DESTROY: compositionLine.hide();candidates.hide();DeleteObject(uiFont); DeleteObject(editFont); PostQuitMessage(0); return 0;
    }
    return DefWindowProcW(w,message,key,arg);
}
}
int WINAPI wWinMain(HINSTANCE instance,HINSTANCE,PWSTR arguments,int show){
    SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    if(!dictionary.load(folder(instance))) {MessageBoxW(nullptr,L"找不到可用的 Tingzi.cin，請從完整資料夾開啟。",L"聽字・注音與語音輸入法",MB_ICONERROR);return 1;}
    dictionary.loadUser(userPhraseFile());dictionary.loadTemplates(userTemplateFile());
    for(const auto& word:Learning().learnedWords())dictionary.addUser(word.reading,word.text);
    WNDCLASSW cls{};cls.hInstance=instance;cls.lpfnWndProc=frame;cls.lpszClassName=L"TingziPad";cls.hCursor=LoadCursorW(nullptr,IDC_ARROW);cls.hbrBackground=(HBRUSH)(COLOR_WINDOW+1);
    cls.hIcon=LoadIconW(instance,MAKEINTRESOURCEW(1));
    RegisterClassW(&cls);
    HWND w=CreateWindowW(cls.lpszClassName,L"聽字・注音與語音輸入法　｜　打字板",WS_OVERLAPPEDWINDOW,CW_USEDEFAULT,CW_USEDEFAULT,850,470,nullptr,nullptr,instance,nullptr);
    if(!w)return 2;
    ShowWindow(w,show); SetFocus(edit);
    MSG msg{};
    while(GetMessageW(&msg,nullptr,0,0)>0){
        if(msg.hwnd==edit && msg.message==WM_KEYDOWN && !(GetKeyState(VK_CONTROL)&0x8000) && !(GetKeyState(VK_MENU)&0x8000)) {
            if(msg.wParam==VK_F6){switchLanguage(); continue;}
            const WPARAM pressed=(GetKeyState(VK_SHIFT)&0x8000)?(msg.wParam|shiftKey):msg.wParam;
            if(chinese && input.wants(pressed)){
                press(pressed);continue;
            }
        }
        TranslateMessage(&msg); DispatchMessageW(&msg);
    }
    return (int)msg.wParam;
}
