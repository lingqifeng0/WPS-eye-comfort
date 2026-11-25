// Native scrollbar colors. The x86 class layout is guarded by the WPS DLL hashes.
namespace blue {
struct QColorValue { DWORD data[4]; };
static bool scrollbars(const QStringList& list,bool restore){
    HMODULE core=GetModuleHandleW(L"Qt5CoreKso.dll"),gui=GetModuleHandleW(L"Qt5GuiKso.dll"),shell=GetModuleHandleW(L"kshell.dll"),widgets=GetModuleHandleW(L"Qt5WidgetsKso.dll");
    using ObjectSet=bool (__thiscall*)(void*,const char*,const void*);using ConstGet=const void* (__thiscall*)(const void*);
    using ColorSet=void (__thiscall*)(void*,QColorValue);using Assign=void* (__thiscall*)(void*,const void*);
    auto get=(PropertyGet)GetProcAddress(core,"?property@QObject@kso_qt@@QBE?AVQVariant@2@PBD@Z");
    auto put=(ObjectSet)GetProcAddress(core,"?setProperty@QObject@kso_qt@@QAE_NPBDABVQVariant@2@@Z");
    auto valid=(Visible)GetProcAddress(core,"?isValid@QVariant@kso_qt@@QBE_NXZ");auto data=(ConstGet)GetProcAddress(core,"?constData@QVariant@kso_qt@@QBEPBXXZ");
    auto empty=(NoArg)GetProcAddress(core,"??0QVariant@kso_qt@@QAE@XZ");auto destroy=(NoArg)GetProcAddress(core,"??1QVariant@kso_qt@@QAE@XZ");
    auto variant=(StringGet)GetProcAddress(gui,"??BQColor@kso_qt@@QBE?AVQVariant@1@XZ");
    auto assign=(Assign)GetProcAddress(gui,"??4QColor@kso_qt@@QAEAAV01@ABV01@@Z");
    auto set=(ColorSet)GetProcAddress(shell,"?setBackgroundColor@KxScrollBar@@QAEXVQColor@kso_qt@@@Z");
    if(!get||!put||!valid||!data||!empty||!destroy||!variant||!assign||!set)return false;
    alignas(8) char color[32]={};makeColor(color,workspace);QColorValue value;memcpy(&value,color,16);
    const char* key="_wps_eye_original_scroll_background";
    for(int i=list.d->begin;i<list.d->end;i++){
        void* w=list.d->array[i];if(!w)continue;void* scroll=((CastGet)(*(void***)w)[1])(w,"KxScrollBar");if(!scroll)continue;
        alignas(8) char saved[64]={};get(w,saved,key);bool has=valid(saved);
        if(restore){if(has)assign((char*)scroll+0x44,data(saved));alignas(8) char e[64]={};empty(e);put(w,key,e);destroy(e);}
        else{if(!has){destroy(saved);variant((char*)scroll+0x44,(QString*)saved);put(w,key,saved);}set(scroll,value);}
        destroy(saved);
    }
    return true;
}
}
