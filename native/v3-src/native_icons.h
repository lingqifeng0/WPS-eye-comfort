// Use WPS's own icon-color binding engine, retaining its native states.
namespace blue {
static bool bindNativeIcons(const QStringList& list,bool restore){
    HMODULE core=GetModuleHandleW(L"Qt5CoreKso.dll"),gui=GetModuleHandleW(L"Qt5GuiKso.dll"),widgets=GetModuleHandleW(L"Qt5WidgetsKso.dll"),shell=GetModuleHandleW(L"kshell.dll");
    using ObjectSet=bool (__thiscall*)(void*,const char*,const void*);
    using ConstGet=const void* (__thiscall*)(const void*);
    using CacheKey=LONGLONG (__thiscall*)(const void*);
    using VariantKey=LONGLONG (__thiscall*)(const void*,bool*);
    using KeyCtor=void (__thiscall*)(void*,LONGLONG);
    using Bind=void (__thiscall*)(void*,void*,const void*,const void*,bool);
    using TryBind=void* (__thiscall*)(void*,void*,const void*,const void*);
    using ClassGet=const char* (__thiscall*)(const void*);
    auto property=(PropertyGet)GetProcAddress(core,"?property@QObject@kso_qt@@QBE?AVQVariant@2@PBD@Z");
    auto setProperty=(ObjectSet)GetProcAddress(core,"?setProperty@QObject@kso_qt@@QAE_NPBDABVQVariant@2@@Z");
    auto valid=(Visible)GetProcAddress(core,"?isValid@QVariant@kso_qt@@QBE_NXZ");
    auto data=(ConstGet)GetProcAddress(core,"?constData@QVariant@kso_qt@@QBEPBXXZ");
    auto variantDtor=(NoArg)GetProcAddress(core,"??1QVariant@kso_qt@@QAE@XZ");
    auto variantCtor=(NoArg)GetProcAddress(core,"??0QVariant@kso_qt@@QAE@XZ");
    auto keyCtor=(KeyCtor)GetProcAddress(core,"??0QVariant@kso_qt@@QAE@_J@Z");
    auto variantKey=(VariantKey)GetProcAddress(core,"?toLongLong@QVariant@kso_qt@@QBE_JPA_N@Z");
    auto iconVariant=(StringGet)GetProcAddress(gui,"??BQIcon@kso_qt@@QBE?AVQVariant@1@XZ");
    auto key=(CacheKey)GetProcAddress(gui,"?cacheKey@QIcon@kso_qt@@QBE_JXZ");
    auto iconDtor=(NoArg)GetProcAddress(gui,"??1QIcon@kso_qt@@QAE@XZ");
    auto skinGet=(MetaGet)GetProcAddress(shell,"?skin@KApplication@@QAEPAVKAppSkin@@XZ");
    auto bind=(Bind)GetProcAddress(shell,"?bindIconColor@KAppSkin@@QAE?AVQIcon@kso_qt@@PBVQObject@3@ABV23@_N@Z");
    auto tryBind=(TryBind)GetProcAddress(shell,"?tryBindAppIconColor@KAppSkin@@AAE?AVQIcon@kso_qt@@PBVQObject@3@ABV23@@Z");
    auto abstractGet=(StringGet)GetProcAddress(widgets,"?icon@QAbstractButton@kso_qt@@QBE?AVQIcon@2@XZ");
    auto abstractSet=(SetString)GetProcAddress(widgets,"?setIcon@QAbstractButton@kso_qt@@QAEXABVQIcon@2@@Z");
    auto toolGet=(StringGet)GetProcAddress(shell,"?icon@KToolButton@@QBE?AVQIcon@kso_qt@@XZ");
    auto toolSet=(SetString)GetProcAddress(shell,"?setIcon@KToolButton@@QAEXABVQIcon@kso_qt@@@Z");
    auto statusGet=(StringGet)GetProcAddress(shell,"?icon@KStatusBarLabel@@QBE?AVQIcon@kso_qt@@XZ");
    auto statusSet=(SetString)GetProcAddress(shell,"?setIcon@KStatusBarLabel@@QAEXABVQIcon@kso_qt@@@Z");
    HMODULE lite=GetModuleHandleW(L"ksolite.dll");
    auto liteGet=(StringGet)GetProcAddress(lite,"?icon@KLiteIconWidget@@QBE?AVQIcon@kso_qt@@XZ");
    auto liteSet=(SetString)GetProcAddress(lite,"?setIcon@KLiteIconWidget@@QAEXABVQIcon@kso_qt@@@Z");
    auto labelGet=(StringGet)GetProcAddress(lite,"?icon@KLiteContentLabel@@QBE?AVQIcon@kso_qt@@XZ");
    auto labelSet=(SetString)GetProcAddress(lite,"?setIcon@KLiteContentLabel@@QAEXABVQIcon@kso_qt@@@Z");
    auto className=(ClassGet)GetProcAddress(core,"?className@QMetaObject@kso_qt@@QBEPBDXZ");
    auto typeName=(const char* (__thiscall*)(const void*))GetProcAddress(core,"?typeName@QVariant@kso_qt@@QBEPBDXZ");
    auto iconCopy=(void (__thiscall*)(void*,const void*))GetProcAddress(gui,"??0QIcon@kso_qt@@QAE@ABV01@@Z");
    if(!property||!setProperty||!valid||!data||!variantDtor||!variantCtor||!keyCtor||!variantKey||!iconVariant||!key||!iconDtor||!skinGet||(!bind&&!tryBind)||!abstractGet||!abstractSet||!toolGet||!toolSet||!className)return false;
    void* skin=skinGet(api.instance());if(!skin)return false;
    const char* originalKey="_wps_native_eye_source_icon";const char* appliedKey="_wps_native_eye_bound_icon_key";
    for(int i=list.d->begin;i<list.d->end;i++){
        void* w=list.d->array[i];if(!w)continue;void** vt=*(void***)w;
        void* meta=((MetaGet)vt[0])(w);const char* cls=meta?className(meta):"";
        bool title=strcmp(cls,"KPromeMinButton")==0||strcmp(cls,"KPromeRestoreMaxButton")==0||strcmp(cls,"KPromeCloseButton")==0;
        if(!title)continue;
        void* button=((CastGet)vt[1])(w,"KToolButton");StringGet get=toolGet;SetString set=toolSet;
        if(!button){button=((CastGet)vt[1])(w,"QAbstractButton");get=abstractGet;set=abstractSet;}
        if(!button&&statusGet&&statusSet){button=((CastGet)vt[1])(w,"KStatusBarLabel");get=statusGet;set=statusSet;}
        if(!button&&liteGet&&liteSet){button=((CastGet)vt[1])(w,"KLiteIconWidget");get=liteGet;set=liteSet;}
        if(!button&&labelGet&&labelSet){button=((CastGet)vt[1])(w,"KLiteContentLabel");get=labelGet;set=labelSet;}
        bool generic=!button;
        alignas(8) char current[64]={},source[64]={},applied[64]={};
        if(generic){
            if(!typeName||!iconCopy)continue;alignas(8) char value[64]={};property(w,value,"icon");
            const char* type=typeName(value);bool isIcon=valid(value)&&type&&strcmp(type,"QIcon")==0;
            if(isIcon)iconCopy(current,data(value));variantDtor(value);if(!isIcon)continue;
        }else get(button,(QString*)current);
        auto setIcon=[&](const void* value){
            if(!generic)set(button,(const QString*)value);
            else{alignas(8) char v[64]={};iconVariant((void*)value,(QString*)v);setProperty(w,"icon",v);variantDtor(v);}
        };
        LONGLONG currentKey=key(current);property(w,source,originalKey);property(w,applied,appliedKey);
        bool has=valid(source),ours=has&&valid(applied)&&currentKey==variantKey(applied,nullptr);
        if(restore){
            if(ours)setIcon(data(source));
            alignas(8) char empty[64]={};variantCtor(empty);setProperty(w,originalKey,empty);setProperty(w,appliedKey,empty);variantDtor(empty);
        }else if(currentKey&&!ours){
            variantDtor(source);iconVariant(current,(QString*)source);setProperty(w,originalKey,source);
            // Prefer WPS's app-level binding helper.  It selects the active
            // icon palette without forcing a disabled/muted raster, and it
            // keeps the source QIcon intact for all device-pixel sizes.
            alignas(8) char revised[64]={};
            if(tryBind)tryBind(skin,revised,w,current);
            else bind(skin,revised,w,current,false);
            setIcon(revised);
            variantDtor(applied);keyCtor(applied,key(revised));setProperty(w,appliedKey,applied);iconDtor(revised);
        }
        variantDtor(applied);variantDtor(source);iconDtor(current);
    }
    return true;
}
}
