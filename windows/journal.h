#pragma once
// A short record of what the text service did, for the voice window's problem report: which program it ran in,
// and which step failed or dropped a key. It never holds what was typed: no keys, readings, candidates or text.
// Every program the input method runs in appends to the same file; a write that fails is skipped.
#include "engine.h"
#include <cstdio>
namespace tingzi {
inline Text journalFile(){
    const Text dir=localLearningDir();
    return dir.empty()?Text{}:dir+L"\\tingzi-ime-log.txt";
}
// The file name of the program the input method is loaded in, such as notepad.exe.
inline const Text& hostProgram(){
    static const Text name=[]{
        wchar_t file[MAX_PATH]{};
        GetModuleFileNameW(nullptr,file,MAX_PATH);
        return std::filesystem::path(file).filename().wstring();
    }();
    return name;
}
inline Text hex(HRESULT hr){
    wchar_t text[16]{};
    swprintf_s(text,L"0x%08X",unsigned(hr));
    return text;
}
// Adds one line. Past 256 KB the record is moved to tingzi-ime-log.old.txt and a new one starts.
inline void note(const Text& event){
    const Text file=journalFile();
    if(file.empty())return;
    WIN32_FILE_ATTRIBUTE_DATA facts{};
    if(GetFileAttributesExW(file.c_str(),GetFileExInfoStandard,&facts) &&
       ((ULONGLONG(facts.nFileSizeHigh)<<32)|facts.nFileSizeLow)>256*1024)
        MoveFileExW(file.c_str(),(file.substr(0,file.size()-4)+L".old.txt").c_str(),MOVEFILE_REPLACE_EXISTING);
    SYSTEMTIME now{};
    GetSystemTime(&now);
    wchar_t stamp[40]{};
    swprintf_s(stamp,L"%04u-%02u-%02u %02u:%02u:%02u UTC ",now.wYear,now.wMonth,now.wDay,now.wHour,now.wMinute,now.wSecond);
    const std::string line=narrow(Text(stamp)+L"注音（"+hostProgram()+L"）："+event+L"\n");
    HANDLE out=CreateFileW(file.c_str(),FILE_APPEND_DATA,FILE_SHARE_READ|FILE_SHARE_WRITE|FILE_SHARE_DELETE,nullptr,OPEN_ALWAYS,FILE_ATTRIBUTE_NORMAL,nullptr);
    if(out==INVALID_HANDLE_VALUE)return;
    DWORD written=0;
    WriteFile(out,line.data(),DWORD(line.size()),&written,nullptr);
    CloseHandle(out);
}
// An event that may repeat with every key: noted the first ten times in this program, then once every hundred,
// with how many times it has happened, so a busy program cannot fill the record.
inline void noteOften(long& count,const Text& event){
    const long n=InterlockedIncrement(&count);
    if(n<=10)note(event);
    else if(n%100==0)note(event+L"（這個程式裡第 "+std::to_wstring(n)+L" 次）");
}
}
