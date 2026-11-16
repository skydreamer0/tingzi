#pragma once
#include "profile.h"
class TestThread final:public ITfThreadMgr,public ITfKeystrokeMgr,public ITfSource {
public:
    ComPtr<ITfKeyEventSink> sink;
    int advised=0,unadvised=0;
    STDMETHODIMP QueryInterface(REFIID iid,void** out) override {
        if(!out)return E_POINTER;
        *out=nullptr;
        if(iid==IID_IUnknown || iid==IID_ITfThreadMgr)*out=static_cast<ITfThreadMgr*>(this);
        else if(iid==IID_ITfKeystrokeMgr)*out=static_cast<ITfKeystrokeMgr*>(this);
        else if(iid==IID_ITfSource)*out=static_cast<ITfSource*>(this);
        else return E_NOINTERFACE;
        AddRef();return S_OK;
    }
    STDMETHODIMP_(ULONG) AddRef() override{return 2;}
    STDMETHODIMP_(ULONG) Release() override{return 1;}
    STDMETHODIMP Activate(TfClientId *ptid) override {return E_NOTIMPL;}
    STDMETHODIMP Deactivate(void) override {return E_NOTIMPL;}
    STDMETHODIMP CreateDocumentMgr(ITfDocumentMgr **ppdim) override {return E_NOTIMPL;}
    STDMETHODIMP EnumDocumentMgrs(IEnumTfDocumentMgrs **ppEnum) override {return E_NOTIMPL;}
    STDMETHODIMP GetFocus(ITfDocumentMgr **ppdimFocus) override {return E_NOTIMPL;}
    STDMETHODIMP SetFocus(ITfDocumentMgr *pdimFocus) override {return E_NOTIMPL;}
    STDMETHODIMP AssociateFocus(HWND hwnd, ITfDocumentMgr *pdimNew, ITfDocumentMgr **ppdimPrev) override {return E_NOTIMPL;}
    STDMETHODIMP IsThreadFocus(BOOL *pfThreadFocus) override {return E_NOTIMPL;}
    STDMETHODIMP GetFunctionProvider(REFCLSID clsid, ITfFunctionProvider **ppFuncProv) override {return E_NOTIMPL;}
    STDMETHODIMP EnumFunctionProviders(IEnumTfFunctionProviders **ppEnum) override {return E_NOTIMPL;}
    STDMETHODIMP GetGlobalCompartment(ITfCompartmentMgr **ppCompMgr) override {return E_NOTIMPL;}
    STDMETHODIMP AdviseKeyEventSink(TfClientId tid, ITfKeyEventSink *pSink, BOOL fForeground) override {sink=pSink;++advised;return S_OK;}
    STDMETHODIMP UnadviseKeyEventSink(TfClientId tid) override {sink.Reset();++unadvised;return S_OK;}
    STDMETHODIMP GetForeground(CLSID *pclsid) override {return E_NOTIMPL;}
    STDMETHODIMP TestKeyDown(WPARAM wParam, LPARAM lParam, BOOL *pfEaten) override {return E_NOTIMPL;}
    STDMETHODIMP TestKeyUp(WPARAM wParam, LPARAM lParam, BOOL *pfEaten) override {return E_NOTIMPL;}
    STDMETHODIMP KeyDown(WPARAM wParam, LPARAM lParam, BOOL *pfEaten) override {return E_NOTIMPL;}
    STDMETHODIMP KeyUp(WPARAM wParam, LPARAM lParam, BOOL *pfEaten) override {return E_NOTIMPL;}
    STDMETHODIMP GetPreservedKey(ITfContext *pic, const TF_PRESERVEDKEY *pprekey, GUID *pguid) override {return E_NOTIMPL;}
    STDMETHODIMP IsPreservedKey(REFGUID rguid, const TF_PRESERVEDKEY *pprekey, BOOL *pfRegistered) override {return E_NOTIMPL;}
    STDMETHODIMP PreserveKey(TfClientId tid, REFGUID rguid, const TF_PRESERVEDKEY *prekey, const WCHAR *pchDesc, ULONG cchDesc) override {return E_NOTIMPL;}
    STDMETHODIMP UnpreserveKey(REFGUID rguid, const TF_PRESERVEDKEY *pprekey) override {return E_NOTIMPL;}
    STDMETHODIMP SetPreservedKeyDescription(REFGUID rguid, const WCHAR *pchDesc, ULONG cchDesc) override {return E_NOTIMPL;}
    STDMETHODIMP GetPreservedKeyDescription(REFGUID rguid, BSTR *pbstrDesc) override {return E_NOTIMPL;}
    STDMETHODIMP SimulatePreservedKey(ITfContext *pic, REFGUID rguid, BOOL *pfEaten) override {return E_NOTIMPL;}
    STDMETHODIMP AdviseSink(REFIID riid, IUnknown *punk, DWORD *pdwCookie) override {*pdwCookie=1;return S_OK;}
    STDMETHODIMP UnadviseSink(DWORD dwCookie) override {return S_OK;}
};
