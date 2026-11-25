// WPS 12.8.2.19823 x86: reversible native view and UI colors.
// Uses the loaded WPS/Qt skin APIs and queues work on the Qt GUI thread.
// It does not draw an overlay, change display gamma, or edit documents.
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#ifdef _MSC_VER
#include <intrin.h>
#endif
#include <stddef.h>
#include <stdint.h>
#include <string.h>

struct QString { void* d; };
struct QListData { LONG ref; int alloc, begin, end; void* array[1]; };
struct QStringList { QListData* d; };
struct ColorRecord { char kind[16]; char style[96]; char name[96]; char value[512]; };
struct Request {
    DWORD magic, version, mode, count;
    volatile LONG done, status;
    DWORD guiThread;
    DWORD window;
    ULONGLONG expireTick;
    ColorRecord records[1];
};
static_assert(offsetof(Request, records) == 40, "Request header must match Python HEADER (40 bytes)");
using FromAscii = void* (__cdecl*)(const char*,int);
using StringCopy = void (__thiscall*)(QString*,const QString*);
using StringDtor = void (__thiscall*)(QString*);
using ListAppend = void** (__thiscall*)(QStringList*);
using ListDetach = QListData* (__thiscall*)(QStringList*,int);
using ListDispose = void (__cdecl*)(QListData*);
using Instance = void* (__cdecl*)();
using PromeSkin = void* (__thiscall*)(void*);
using SetData = void (__thiscall*)(void*,const QString*,const QString*,const QString*,const QStringList*);
using NoArg = void (__thiscall*)(void*);
using Reset = void (__thiscall*)(void*,bool);
using SwitchSkin = void (__thiscall*)(void*,const QString*,bool,bool,bool);
using AllWidgets = void (__cdecl*)(QStringList*);
using SizeGet = int (__thiscall*)(void*);
using Visible = bool (__thiscall*)(void*);
using MetaGet = void* (__thiscall*)(void*);
using CastGet = void* (__thiscall*)(void*,const char*);
using ClassGet = const char* (__thiscall*)(void*);
using StringGet = void (__thiscall*)(void*,QString*);
using PropertyGet = void (__thiscall*)(void*,void*,const char*);
using VariantDtor = void (__thiscall*)(void*);
using SetString = void (__thiscall*)(void*,const QString*);
struct Slot;
using SlotImpl = void (__cdecl*)(int,Slot*,void*,void**,bool*);
struct Slot { LONG ref; SlotImpl impl; Request* request; };
using SingleShot = void (__cdecl*)(int,int,const void*,Slot*);

struct Api {
    FromAscii from; StringCopy copy; StringDtor destroy;
    ListAppend append; ListDetach detach; ListDispose dispose; QListData* empty;
    Instance instance; PromeSkin skin; SetData setData; SetData emitData;
    Reset reset; NoArg changed, resetResources; SingleShot queue;
    SwitchSkin switchSkin;
};
static Api api={};
static HINSTANCE agentModule=nullptr;
static HHOOK volatile pendingHook=nullptr;
static Request* volatile pendingRequest=nullptr;
static volatile LONG requestBusy=0;

static bool loadApi() {
    HMODULE qt=GetModuleHandleW(L"Qt5CoreKso.dll");
    HMODULE prome=GetModuleHandleW(L"kprometheus.dll");
    if(!qt || !prome) return false;
#define QT(field,type,symbol) api.field=(type)GetProcAddress(qt,symbol); if(!api.field)return false
#define PR(field,type,symbol) api.field=(type)GetProcAddress(prome,symbol); if(!api.field)return false
    QT(from,FromAscii,"?fromAscii_helper@QString@kso_qt@@CAPAU?$QTypedArrayData@G@2@PBDH@Z");
    QT(copy,StringCopy,"??0QString@kso_qt@@QAE@ABV01@@Z");
    QT(destroy,StringDtor,"??1QString@kso_qt@@QAE@XZ");
    QT(append,ListAppend,"?append@QListData@kso_qt@@QAEPAPAXXZ");
    QT(detach,ListDetach,"?detach@QListData@kso_qt@@QAEPAUData@12@H@Z");
    QT(dispose,ListDispose,"?dispose@QListData@kso_qt@@SAXPAUData@12@@Z");
    QT(empty,QListData*,"?shared_null@QListData@kso_qt@@2UData@12@B");
    QT(instance,Instance,"?instance@QCoreApplication@kso_qt@@SAPAV12@XZ");
    QT(queue,SingleShot,"?singleShotImpl@QTimer@kso_qt@@CAXHW4TimerType@Qt@2@PBVQObject@2@PAVQSlotObjectBase@QtPrivate@2@@Z");
    PR(skin,PromeSkin,"?promeSkin@KPromeApplication@@QAEPAVKPromeSkin@@XZ");
    PR(setData,SetData,"?setUserThemeData@KPromeSkin@@QAEXABVQString@kso_qt@@00ABVQStringList@3@@Z");
    PR(emitData,SetData,"?emitSetWpsThemeData@KPromeSkin@@QAEXABVQString@kso_qt@@00ABVQStringList@3@@Z");
    PR(reset,Reset,"?reset@KPromeSkin@@QAEX_N@Z");
    PR(changed,NoArg,"?informChanged@KPromeSkin@@QAEXXZ");
    PR(resetResources,NoArg,"?resetDeviceDependentResources@KPromeSkin@@QAEXXZ");
    PR(switchSkin,SwitchSkin,"?switchSkin@KPromeSkin@@QAEXABVQString@kso_qt@@_N11@Z");
    return true;
}

static QString qstr(const char* text) {
    QString q={api.from(text,(int)strlen(text))}; return q;
}
static void textOf(const QString& q,char* out,int capacity) {
    int length=((int*)q.d)[1];int offset=((int*)q.d)[3];
    if(length<0 || length>10000){out[0]=0;return;}
    int n=WideCharToMultiByte(CP_UTF8,0,(const wchar_t*)((char*)q.d+offset),length,out,capacity-1,nullptr,nullptr);
    out[n>0?n:0]=0;
}
#include "theme.h"
#include "display_options.h"
#include "skin_switch.h"
static void processRequest(Request* req) {
    req->guiThread=GetCurrentThreadId();bool ok=true;
    if(req->expireTick && GetTickCount64()>req->expireTick){
        req->status=-5;
        InterlockedExchange(&requestBusy,0);InterlockedExchange(&req->done,1);
        return;
    }
    if(req->mode==9){ok=blue::setProfile(req)&&blue::apply(false);if(!ok)blue::apply(true);}
    else if(req->mode==10)ok=blue::apply(true);
    else if(req->mode==12)ok=blue::load()&&display::apply(req);
    else if(req->mode==13)ok=blue::postRefresh();
    else if(req->mode==14)ok=skin_switch::apply(req);
    if(!req->status)req->status=ok?1:-6;
    InterlockedExchange(&requestBusy,0);InterlockedExchange(&req->done,1);
}
static LRESULT CALLBACK oneMessageHook(int code,WPARAM parameter,LPARAM data) {
    CWPSTRUCT* message=(CWPSTRUCT*)data;
    if(code>=0 && message && message->message==WM_NULL && message->wParam==0x57454633 && message->lParam){
        Request* candidate=(Request*)message->lParam;
        if(InterlockedCompareExchangePointer((PVOID*)&pendingRequest,nullptr,candidate)==candidate){
            HHOOK hook=(HHOOK)InterlockedExchangePointer((PVOID*)&pendingHook,nullptr);
            if(hook)UnhookWindowsHookEx(hook);
            processRequest(candidate);
        }
    }
    return CallNextHookEx(nullptr,code,parameter,data);
}

extern "C" __declspec(dllexport) DWORD WINAPI RunFrameRequest(void* data) {
    Request* req=(Request*)data;
    if(!req || req->magic!=0x57454633 || req->version!=4 || (req->mode!=0&&req->mode!=9&&req->mode!=10&&req->mode!=12&&req->mode!=13&&req->mode!=14) || req->count>20)return 10;
    for(DWORD i=0;i<req->count;i++){
        if(!memchr(req->records[i].kind,0,16) || !memchr(req->records[i].style,0,96) || !memchr(req->records[i].name,0,96) || !memchr(req->records[i].value,0,512))return 11;
    }
    DWORD owner=0;DWORD actualThread=GetWindowThreadProcessId((HWND)(uintptr_t)req->window,&owner);
    if(owner!=GetCurrentProcessId()||actualThread!=req->guiThread)return 17;
    if(!loadApi()){req->status=-1;InterlockedExchange(&req->done,1);return 12;}
    void* app=api.instance();
    if(!app){req->status=-2;InterlockedExchange(&req->done,1);return 13;}
    if(InterlockedCompareExchange(&requestBusy,1,0)!=0)return 15;
    InterlockedExchangePointer((PVOID*)&pendingRequest,req);
    GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS|GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,(LPCWSTR)&oneMessageHook,(HMODULE*)&agentModule);
    HHOOK hook=SetWindowsHookExW(WH_CALLWNDPROC,oneMessageHook,agentModule,req->guiThread);
    InterlockedExchangePointer((PVOID*)&pendingHook,hook);
    DWORD timeoutMs=(req->mode==14)?10000:4000;
    DWORD_PTR result=0;
    bool sent=hook && SendMessageTimeoutW((HWND)(uintptr_t)req->window,WM_NULL,0x57454633,(LPARAM)req,SMTO_ABORTIFHUNG,timeoutMs,&result);
    // Atomically reclaim req if GUI hook did not execute (timeout, closed window, or hook failure).
    if(InterlockedCompareExchangePointer((PVOID volatile*)&pendingRequest,nullptr,req)==req){
        HHOOK oldHook=(HHOOK)InterlockedExchangePointer((PVOID volatile*)&pendingHook,nullptr);
        if(oldHook)UnhookWindowsHookEx(oldHook);
        InterlockedExchange(&requestBusy,0);
        req->status=-4;InterlockedExchange(&req->done,1);return 14;
    }
    return sent?0:16; // 16 = SendMessageTimeout timed out after GUI hook already claimed req.
}
extern "C" BOOL WINAPI DllMain(HINSTANCE dll,DWORD reason,void*) {
    if(reason==DLL_PROCESS_ATTACH){agentModule=dll;DisableThreadLibraryCalls(dll);}
    return TRUE;
}
