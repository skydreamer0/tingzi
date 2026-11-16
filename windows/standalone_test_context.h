#pragma once
#include "profile.h"
#include <vector>
class TestContext final:public ITfContext,public ITfInsertAtSelection,public ITfContextComposition,public ITfComposition,public ITfRange,public ITfSource {
public:
    std::wstring text;
    int ended=0;
    bool rejectAsync=false;
    std::vector<ComPtr<ITfEditSession>> queued;
    STDMETHODIMP QueryInterface(REFIID iid,void** out) override {
        if(!out)return E_POINTER;
        *out=nullptr;
        if(iid==IID_ITfContext)*out=static_cast<ITfContext*>(this);
        if(iid==IID_ITfInsertAtSelection)*out=static_cast<ITfInsertAtSelection*>(this);
        if(iid==IID_ITfContextComposition)*out=static_cast<ITfContextComposition*>(this);
        if(iid==IID_ITfComposition)*out=static_cast<ITfComposition*>(this);
        if(iid==IID_ITfRange)*out=static_cast<ITfRange*>(this);
        if(iid==IID_ITfSource)*out=static_cast<ITfSource*>(this);
        if(iid==IID_IUnknown)*out=static_cast<ITfContext*>(this);
        if(!*out)return E_NOINTERFACE;
        AddRef();return S_OK;
    }
    STDMETHODIMP_(ULONG) AddRef() override{return 2;}
    STDMETHODIMP_(ULONG) Release() override{return 1;}
    void flush(){auto pending=std::move(queued);queued.clear();for(auto& session:pending)session->DoEditSession(1);}
    STDMETHODIMP RequestEditSession(TfClientId tid, ITfEditSession *pes, DWORD dwFlags, HRESULT *phrSession) override {if(dwFlags&TF_ES_SYNC){*phrSession=pes->DoEditSession(1);}else{if(rejectAsync){*phrSession=E_FAIL;return E_FAIL;}queued.emplace_back(pes);*phrSession=TF_S_ASYNC;}return S_OK;}
    STDMETHODIMP InWriteSession(TfClientId tid, BOOL *pfWriteSession) override {return E_NOTIMPL;}
    STDMETHODIMP GetSelection(TfEditCookie ec, ULONG ulIndex, ULONG ulCount, TF_SELECTION *pSelection, ULONG *pcFetched) override {return E_NOTIMPL;}
    STDMETHODIMP SetSelection(TfEditCookie ec, ULONG ulCount, const TF_SELECTION *pSelection) override {return S_OK;}
    STDMETHODIMP GetStart(TfEditCookie ec, ITfRange **ppStart) override {return E_NOTIMPL;}
    STDMETHODIMP GetEnd(TfEditCookie ec, ITfRange **ppEnd) override {return E_NOTIMPL;}
    STDMETHODIMP GetActiveView(ITfContextView **ppView) override {return E_NOTIMPL;}
    STDMETHODIMP EnumViews(IEnumTfContextViews **ppEnum) override {return E_NOTIMPL;}
    STDMETHODIMP GetStatus(TF_STATUS *pdcs) override {return E_NOTIMPL;}
    STDMETHODIMP GetProperty(REFGUID guidProp, ITfProperty **ppProp) override {return E_NOTIMPL;}
    STDMETHODIMP GetAppProperty(REFGUID guidProp, ITfReadOnlyProperty **ppProp) override {return E_NOTIMPL;}
    STDMETHODIMP TrackProperties(const GUID **prgProp, ULONG cProp, const GUID **prgAppProp, ULONG cAppProp, ITfReadOnlyProperty **ppProperty) override {return E_NOTIMPL;}
    STDMETHODIMP EnumProperties(IEnumTfProperties **ppEnum) override {return E_NOTIMPL;}
    STDMETHODIMP GetDocumentMgr(ITfDocumentMgr **ppDm) override {return E_NOTIMPL;}
    STDMETHODIMP CreateRangeBackup(TfEditCookie ec, ITfRange *pRange, ITfRangeBackup **ppBackup) override {return E_NOTIMPL;}
    STDMETHODIMP InsertTextAtSelection(TfEditCookie ec, DWORD dwFlags, const WCHAR *pchText, LONG cch, ITfRange **ppRange) override {*ppRange=static_cast<ITfRange*>(this);AddRef();return S_OK;}
    STDMETHODIMP InsertEmbeddedAtSelection(TfEditCookie ec, DWORD dwFlags, IDataObject *pDataObject, ITfRange **ppRange) override {return E_NOTIMPL;}
    STDMETHODIMP StartComposition(TfEditCookie ecWrite, ITfRange *pCompositionRange, ITfCompositionSink *pSink, ITfComposition **ppComposition) override {*ppComposition=static_cast<ITfComposition*>(this);AddRef();return S_OK;}
    STDMETHODIMP EnumCompositions(IEnumITfCompositionView **ppEnum) override {return E_NOTIMPL;}
    STDMETHODIMP FindComposition(TfEditCookie ecRead, ITfRange *pTestRange, IEnumITfCompositionView **ppEnum) override {return E_NOTIMPL;}
    STDMETHODIMP TakeOwnership(TfEditCookie ecWrite, ITfCompositionView *pComposition, ITfCompositionSink *pSink, ITfComposition **ppComposition) override {return E_NOTIMPL;}
    STDMETHODIMP GetRange(ITfRange **ppRange) override {*ppRange=static_cast<ITfRange*>(this);AddRef();return S_OK;}
    STDMETHODIMP ShiftStart(TfEditCookie ecWrite, ITfRange *pNewStart) override {return E_NOTIMPL;}
    STDMETHODIMP ShiftEnd(TfEditCookie ecWrite, ITfRange *pNewEnd) override {return E_NOTIMPL;}
    STDMETHODIMP EndComposition(TfEditCookie ecWrite) override {++ended;return S_OK;}
    STDMETHODIMP GetText(TfEditCookie ec, DWORD dwFlags, WCHAR *pchText, ULONG cchMax, ULONG *pcch) override {return E_NOTIMPL;}
    STDMETHODIMP SetText(TfEditCookie ec, DWORD dwFlags, const WCHAR *pchText, LONG cch) override {text.assign(pchText,cch);return S_OK;}
    STDMETHODIMP GetFormattedText(TfEditCookie ec, IDataObject **ppDataObject) override {return E_NOTIMPL;}
    STDMETHODIMP GetEmbedded(TfEditCookie ec, REFGUID rguidService, REFIID riid, IUnknown **ppunk) override {return E_NOTIMPL;}
    STDMETHODIMP InsertEmbedded(TfEditCookie ec, DWORD dwFlags, IDataObject *pDataObject) override {return E_NOTIMPL;}
    STDMETHODIMP ShiftStart(TfEditCookie ec, LONG cchReq, LONG *pcch, const TF_HALTCOND *pHalt) override {return E_NOTIMPL;}
    STDMETHODIMP ShiftEnd(TfEditCookie ec, LONG cchReq, LONG *pcch, const TF_HALTCOND *pHalt) override {return E_NOTIMPL;}
    STDMETHODIMP ShiftStartToRange(TfEditCookie ec, ITfRange *pRange, TfAnchor aPos) override {return E_NOTIMPL;}
    STDMETHODIMP ShiftEndToRange(TfEditCookie ec, ITfRange *pRange, TfAnchor aPos) override {return E_NOTIMPL;}
    STDMETHODIMP ShiftStartRegion(TfEditCookie ec, TfShiftDir dir, BOOL *pfNoRegion) override {return E_NOTIMPL;}
    STDMETHODIMP ShiftEndRegion(TfEditCookie ec, TfShiftDir dir, BOOL *pfNoRegion) override {return E_NOTIMPL;}
    STDMETHODIMP IsEmpty(TfEditCookie ec, BOOL *pfEmpty) override {return E_NOTIMPL;}
    STDMETHODIMP Collapse(TfEditCookie ec, TfAnchor aPos) override {return S_OK;}
    STDMETHODIMP IsEqualStart(TfEditCookie ec, ITfRange *pWith, TfAnchor aPos, BOOL *pfEqual) override {return E_NOTIMPL;}
    STDMETHODIMP IsEqualEnd(TfEditCookie ec, ITfRange *pWith, TfAnchor aPos, BOOL *pfEqual) override {return E_NOTIMPL;}
    STDMETHODIMP CompareStart(TfEditCookie ec, ITfRange *pWith, TfAnchor aPos, LONG *plResult) override {return E_NOTIMPL;}
    STDMETHODIMP CompareEnd(TfEditCookie ec, ITfRange *pWith, TfAnchor aPos, LONG *plResult) override {return E_NOTIMPL;}
    STDMETHODIMP AdjustForInsert(TfEditCookie ec, ULONG cchInsert, BOOL *pfInsertOk) override {return E_NOTIMPL;}
    STDMETHODIMP GetGravity(TfGravity *pgStart, TfGravity *pgEnd) override {return E_NOTIMPL;}
    STDMETHODIMP SetGravity(TfEditCookie ec, TfGravity gStart, TfGravity gEnd) override {return E_NOTIMPL;}
    STDMETHODIMP Clone(ITfRange **ppClone) override {*ppClone=static_cast<ITfRange*>(this);AddRef();return S_OK;}
    STDMETHODIMP GetContext(ITfContext **ppContext) override {return E_NOTIMPL;}
    STDMETHODIMP AdviseSink(REFIID riid, IUnknown *punk, DWORD *pdwCookie) override {*pdwCookie=1;return S_OK;}
    STDMETHODIMP UnadviseSink(DWORD dwCookie) override {return S_OK;}
};
