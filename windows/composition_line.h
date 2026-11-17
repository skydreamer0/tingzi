#pragma once
#include "engine.h"
#include "palette.h"

namespace tingzi {
#ifdef TINGZI_STANDALONE
inline constexpr wchar_t appearanceKey[]=L"Software\\TingziStandaloneRC\\Appearance";
#else
inline constexpr wchar_t appearanceKey[]=L"Software\\Tingzi\\Appearance";
#endif
// Follows the choice saved by the settings window, then Windows; high contrast always wins.
inline Palette palette() {
    HIGHCONTRASTW hc{sizeof(hc)};
    if(SystemParametersInfoW(SPI_GETHIGHCONTRAST,sizeof(hc),&hc,0) && (hc.dwFlags&HCF_HIGHCONTRASTON))
        return {GetSysColor(COLOR_WINDOW),GetSysColor(COLOR_WINDOWTEXT),GetSysColor(COLOR_WINDOWTEXT),GetSysColor(COLOR_WINDOWFRAME),GetSysColor(COLOR_HIGHLIGHT),GetSysColor(COLOR_HIGHLIGHTTEXT)};
    DWORD choice=0, light=1, bytes=sizeof(DWORD);
    RegGetValueW(HKEY_CURRENT_USER,appearanceKey,L"Theme",RRF_RT_REG_DWORD,nullptr,&choice,&bytes);
    bytes=sizeof(DWORD);
    RegGetValueW(HKEY_CURRENT_USER,L"Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize",L"AppsUseLightTheme",RRF_RT_REG_DWORD,nullptr,&light,&bytes);
    const bool dark=choice==2 || (choice!=1 && light==0);
    return dark ? darkPalette : lightPalette;
}
inline void fill(HDC dc,const RECT& r,COLORREF color){SetDCBrushColor(dc,color);FillRect(dc,&r,(HBRUSH)GetStockObject(DC_BRUSH));}
// A soft rounded block, for the chosen candidate and the syllables being typed.
inline void chip(HDC dc,const RECT& r,COLORREF color,int radius){
    auto brush=SelectObject(dc,GetStockObject(DC_BRUSH));auto pen=SelectObject(dc,GetStockObject(NULL_PEN));
    SetDCBrushColor(dc,color);RoundRect(dc,r.left,r.top,r.right+1,r.bottom+1,radius*2,radius*2);
    SelectObject(dc,pen);SelectObject(dc,brush);
}
// Asks Windows 11 to round the window and draw its edge in `edge`. Returns false on Windows 10,
// where the window keeps square corners and draws its own edge.
inline bool roundWindow(HWND window,COLORREF edge){
    using Set=HRESULT(WINAPI*)(HWND,DWORD,LPCVOID,DWORD);
    static const auto set=reinterpret_cast<Set>(reinterpret_cast<void*>(GetProcAddress(LoadLibraryW(L"dwmapi.dll"),"DwmSetWindowAttribute")));
    constexpr DWORD cornerPreference=33, borderColor=34, round=2;
    if(!set || FAILED(set(window,cornerPreference,&round,sizeof(round))))return false;
    set(window,borderColor,&edge,sizeof(edge));
    return true;
}
inline int displaySize(const wchar_t* name, int fallback, std::initializer_list<DWORD> allowed) {
    DWORD size=0, bytes=sizeof(size);
    if(RegGetValueW(HKEY_CURRENT_USER,appearanceKey,name,RRF_RT_REG_DWORD,nullptr,&size,&bytes)==ERROR_SUCCESS &&
       std::find(allowed.begin(),allowed.end(),size)!=allowed.end()) return int(size);
    return fallback;
}
inline int lineSize(){return displaySize(L"CompositionDIP",44,{0,36,44,52});}
inline int candidateSize(){return displaySize(L"CandidateDIP",24,{20,24,32});}

class CompositionLine {
    static constexpr wchar_t className[]=L"TingziCompositionLine";
    HWND window=nullptr;
    HMODULE module=nullptr;
    HFONT font=nullptr;
    Input::DisplayRuns runs;
    int fontHeight=0,padding=0;
    bool rounded=false;

    static LRESULT CALLBACK proc(HWND hwnd,UINT msg,WPARAM wp,LPARAM lp){
        if(msg==WM_NCCREATE){
            auto create=reinterpret_cast<CREATESTRUCTW*>(lp);
            SetWindowLongPtrW(hwnd,GWLP_USERDATA,reinterpret_cast<LONG_PTR>(create->lpCreateParams));
        }
        auto self=reinterpret_cast<CompositionLine*>(GetWindowLongPtrW(hwnd,GWLP_USERDATA));
        switch(msg){
        case WM_NCHITTEST: return HTTRANSPARENT;
        case WM_MOUSEACTIVATE: return MA_NOACTIVATE;
        case WM_ERASEBKGND: return 1;
        case WM_PAINT: if(self){self->paint(hwnd);return 0;} break;
        }
        return DefWindowProcW(hwnd,msg,wp,lp);
    }
    static int width(HDC dc,const Text& text){
        SIZE size{};
        if(!text.empty()) GetTextExtentPoint32W(dc,text.data(),int(text.size()),&size);
        return size.cx;
    }
    void paint(HWND hwnd){
        PAINTSTRUCT ps{}; HDC dc=BeginPaint(hwnd,&ps);
        RECT r{};GetClientRect(hwnd,&r);
        const Palette colors=palette();
        fill(dc,r,colors.back);
        if(!rounded){SetDCBrushColor(dc,colors.frame);FrameRect(dc,&r,(HBRUSH)GetStockObject(DC_BRUSH));}
        auto old=SelectObject(dc,font);
        int before=width(dc,runs.before), active=width(dc,runs.active);
        RECT clip{padding,2,(std::max)(LONG(padding),r.right-padding),r.bottom-2};
        IntersectClipRect(dc,clip.left,clip.top,clip.right,clip.bottom);
        int x=padding-(std::max)(0,before+active-int(clip.right-clip.left));
        TEXTMETRICW tm{}; GetTextMetricsW(dc,&tm);
        int y=(r.bottom-tm.tmHeight)/2;
        SetBkMode(dc,TRANSPARENT);SetTextColor(dc,colors.text);
        TextOutW(dc,x,y,runs.before.data(),int(runs.before.size()));
        x+=before;
        if(active){
            const int inset=(std::max)(2,padding/3);
            RECT highlight{x-inset,inset,x+active+inset,r.bottom-inset};
            chip(dc,highlight,colors.active,chipRadius);
            SetTextColor(dc,colors.activeText);
            TextOutW(dc,x,y,runs.active.data(),int(runs.active.size()));
        }
        SetTextColor(dc,colors.text);
        // With nothing being typed, a bar marks where the caret sits inside the sentence.
        int gap=0;
        if(!active && !runs.after.empty()){
            RECT bar{x,y,x+2,y+tm.tmHeight};
            SetDCBrushColor(dc,colors.text);FillRect(dc,&bar,(HBRUSH)GetStockObject(DC_BRUSH));
            gap=4;
        }
        TextOutW(dc,x+active+gap,y,runs.after.data(),int(runs.after.size()));
        SelectObject(dc,old);EndPaint(hwnd,&ps);
    }
public:
    CompositionLine()=default;
    CompositionLine(const CompositionLine&)=delete;
    CompositionLine& operator=(const CompositionLine&)=delete;
    ~CompositionLine(){hide();}
    void hide(){
        if(window){DestroyWindow(window);window=nullptr;UnregisterClassW(className,module);}
        if(font){DeleteObject(font);font=nullptr;}
        fontHeight=0;runs={};
    }
    RECT show(HMODULE instance,HWND parent,RECT caret,const Input& input,int dip){
        if(!dip || !input.active() || !parent){hide();return caret;}
        if(window && GetWindow(window,GW_OWNER)!=GetAncestor(parent,GA_ROOT))hide();
        if(!window){
            module=instance;
            WNDCLASSW wc{};wc.lpfnWndProc=proc;wc.hInstance=module;wc.lpszClassName=className;
            if(!RegisterClassW(&wc) && GetLastError()!=ERROR_CLASS_ALREADY_EXISTS)return caret;
            window=CreateWindowExW(WS_EX_NOACTIVATE|WS_EX_TOOLWINDOW|WS_EX_TOPMOST|WS_EX_TRANSPARENT,
                className,L"聽字 輸入行",WS_POPUP,0,0,0,0,GetAncestor(parent,GA_ROOT),nullptr,module,this);
            if(!window){UnregisterClassW(className,module);return caret;}
        }
        // Again on every showing, so the edge follows a change between light and dark.
        rounded=roundWindow(window,palette().frame);
        UINT dpi=GetDpiForWindow(parent);if(!dpi)dpi=96;
        const int height=MulDiv(dip,dpi,96);
        bool changed=height!=fontHeight;
        if(changed){
            if(font)DeleteObject(font);
            font=CreateFontW(-height,0,0,0,FW_NORMAL,FALSE,FALSE,FALSE,DEFAULT_CHARSET,OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,CLEARTYPE_QUALITY,DEFAULT_PITCH,L"Microsoft JhengHei UI");
            fontHeight=height;
        }
        auto next=input.displayRuns();
        changed=changed || next.before!=runs.before || next.active!=runs.active || next.after!=runs.after;
        runs=std::move(next);padding=MulDiv(8,dpi,96);
        HDC dc=GetDC(window);auto old=SelectObject(dc,font);
        int textWidth=width(dc,runs.before)+width(dc,runs.active)+width(dc,runs.after);
        TEXTMETRICW tm{};GetTextMetricsW(dc,&tm);SelectObject(dc,old);ReleaseDC(window,dc);
        MONITORINFO mon{sizeof(mon)};
        if(!GetMonitorInfoW(MonitorFromRect(&caret,MONITOR_DEFAULTTONEAREST),&mon)){hide();return caret;}
        const RECT work=mon.rcWork;
        int w=std::min<int>(work.right-work.left,std::clamp(textWidth+2*padding,MulDiv(220,dpi,96),MulDiv(720,dpi,96)));
        int h=std::min<int>(work.bottom-work.top,tm.tmHeight+2*padding);
        int x=std::clamp<int>(caret.left,work.left,work.right-w);
        int y=caret.bottom+h<=work.bottom?std::max<int>(work.top,caret.bottom):
            std::clamp<int>(caret.top-h,work.top,work.bottom-h);
        RECT placed{x,y,x+w,y+h},current{};
        GetWindowRect(window,&current);
        if(!IsWindowVisible(window) || !EqualRect(&current,&placed)){
            SetWindowPos(window,HWND_TOPMOST,x,y,w,h,SWP_SHOWWINDOW|SWP_NOACTIVATE);changed=true;
        }
        if(changed)InvalidateRect(window,nullptr,FALSE);
        return placed;
    }
};
}
