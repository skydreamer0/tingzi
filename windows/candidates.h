#pragma once
#include "engine.h"
#include "composition_line.h"
namespace tingzi {
class CandidateWindow {
    HWND window=nullptr;
    WNDPROC original=nullptr;
    HFONT font=nullptr;
    int cellW=100,cellH=34,columns=3,rows=1,header=26;
    bool rounded=false;
    size_t page=0, selected=0,total=0;
    bool learning=false;
    Text caption;
    std::vector<Text> labels;
    void* owner=nullptr;
    using Choose=void (*)(void*,WPARAM);
    Choose choose=nullptr;
    Text title() const {
        return caption+L"   "+std::to_wstring(page+1)+L"/"+std::to_wstring((total+8)/9)
            +(learning?L"   學習中":L"   學習關");
    }
    int cellAt(int x,int y) const {
        if(y<header || x<0 || x>=cellW*columns)return -1;
        int i=(y-header)/cellH*columns+x/cellW;
        return i>=0 && i<int(labels.size())?i:-1;
    }
    static LRESULT CALLBACK proc(HWND w,UINT m,WPARAM k,LPARAM arg){
        auto self=reinterpret_cast<CandidateWindow*>(GetWindowLongPtrW(w,GWLP_USERDATA));
        if(!self)return DefWindowProcW(w,m,k,arg);
        if(m==WM_MOUSEACTIVATE)return MA_NOACTIVATE;
        if(m==WM_ERASEBKGND)return 1;
        if(m==WM_PAINT){self->paint();return 0;}
        if(m==WM_LBUTTONUP){
            int i=self->cellAt(LOWORD(arg),HIWORD(arg));if(i>=0)self->choose(self->owner,'1'+i);
            return 0;
        }
        if(m==WM_MOUSEWHEEL){self->choose(self->owner,GET_WHEEL_DELTA_WPARAM(k)>0?VK_PRIOR:VK_NEXT);return 0;}
        return CallWindowProcW(self->original,w,m,k,arg);
    }
    void paint(){
        PAINTSTRUCT ps;HDC dc=BeginPaint(window,&ps);
        RECT bounds;GetClientRect(window,&bounds);
        const Palette colors=palette(); fill(dc,bounds,colors.back); SetBkMode(dc,TRANSPARENT);
        if(!rounded){SetDCBrushColor(dc,colors.frame);FrameRect(dc,&bounds,(HBRUSH)GetStockObject(DC_BRUSH));}
        auto previous=SelectObject(dc,font);
        RECT top={10,0,bounds.right-8,header};SetTextColor(dc,colors.muted);
        DrawTextW(dc,title().c_str(),-1,&top,DT_SINGLELINE|DT_VCENTER|DT_NOPREFIX|DT_END_ELLIPSIS);
        for(int i=0;i<int(labels.size());++i){
            bool active=page*9+i==selected;
            RECT r={i%columns*cellW,header+i/columns*cellH,(i%columns+1)*cellW,header+(i/columns+1)*cellH};
            if(active){RECT soft{r.left+4,r.top+2,r.right-4,r.bottom-2};chip(dc,soft,colors.active,chipRadius);}
            SetTextColor(dc,active?colors.activeText:colors.text);r.left+=12;
            Text text=std::to_wstring(i+1)+L"  "+labels[i];
            DrawTextW(dc,text.c_str(),-1,&r,DT_NOPREFIX|DT_SINGLELINE|DT_VCENTER|DT_END_ELLIPSIS);
        }
        SelectObject(dc,previous);EndPaint(window,&ps);
    }
public:
    ~CandidateWindow(){hide();}
    void hide(){if(window){DestroyWindow(window);window=nullptr;}if(font){DeleteObject(font);font=nullptr;}}
    void show(HMODULE module,HWND parent,RECT caret,const Input& input,DWORD learningStatus,void* target,Choose callback){
        auto list=input.choices();if(list.empty()){hide();return;}
        owner=target;choose=callback;page=input.currentPage();selected=input.currentIndex();total=list.size();learning=input.learningActive();caption=input.listTitle();
        labels.clear();
        for(size_t i=page*9;i<list.size() && i<page*9+9;++i)labels.push_back(list[i].text);
        if(!window){
            window=CreateWindowExW(WS_EX_NOACTIVATE|WS_EX_TOOLWINDOW|WS_EX_TOPMOST,L"STATIC",L"聽字 候選",WS_POPUP,
                0,0,1,1,parent,nullptr,module,nullptr);
            if(!window)return;
            SetWindowLongPtrW(window,GWLP_USERDATA,(LONG_PTR)this);
            original=reinterpret_cast<WNDPROC>(SetWindowLongPtrW(window,GWLP_WNDPROC,(LONG_PTR)proc));
        }
        rounded=roundWindow(window,palette().frame);
        SetPropW(window,L"TingziLearningStatus",(HANDLE)(ULONG_PTR)learningStatus);
        UINT dpi=GetDpiForWindow(parent?parent:window);if(!dpi)dpi=96;
        if(font)DeleteObject(font);
        int size=candidateSize();
        font=CreateFontW(-MulDiv(size,dpi,96),0,0,0,FW_NORMAL,0,0,0,DEFAULT_CHARSET,0,0,CLEARTYPE_QUALITY,0,L"Microsoft JhengHei UI");
        HDC dc=GetDC(window);auto old=SelectObject(dc,font);
        auto caption=title(); SIZE titleSize{};
        GetTextExtentPoint32W(dc,caption.c_str(),(int)caption.size(),&titleSize);
        cellW=int(titleSize.cx)+MulDiv(24,dpi,96);
        for(auto& label:labels){SIZE size{};GetTextExtentPoint32W(dc,label.c_str(),(int)label.size(),&size);cellW=(std::max)(cellW,int(size.cx)+MulDiv(44,dpi,96));}
        SelectObject(dc,old);ReleaseDC(window,dc);
        columns=1;rows=int(labels.size());header=MulDiv(size+12,dpi,96);cellH=MulDiv(size+14,dpi,96);
        MONITORINFO mon{sizeof(mon)};GetMonitorInfoW(MonitorFromRect(&caret,MONITOR_DEFAULTTONEAREST),&mon);
        int room=std::max<int>(mon.rcWork.bottom-caret.bottom,caret.top-mon.rcWork.top);
        int fit=std::max(1,(room-header-2)/cellH);
        if(rows>fit){
            columns=(rows+fit-1)/fit;
            rows=(int(labels.size())+columns-1)/columns;
        }
        cellW=std::min(std::max(cellW,MulDiv(250,dpi,96)),int(mon.rcWork.right-mon.rcWork.left-4)/columns);
        int width=columns*cellW+2,height=header+rows*cellH+2;
        int x=std::clamp<int>(caret.left,mon.rcWork.left,(std::max)(mon.rcWork.left,mon.rcWork.right-width));
        int y=caret.bottom+height<=mon.rcWork.bottom?caret.bottom:(std::max)(mon.rcWork.top,caret.top-height);
        SetWindowPos(window,HWND_TOPMOST,x,y,width,height,SWP_NOACTIVATE|SWP_SHOWWINDOW);
        InvalidateRect(window,nullptr,FALSE);
    }
};
}
