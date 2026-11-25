// WPS display preferences, not document formatting. No registry-only cache claims.
namespace display {
static const DWORD ids[]={0xe0010031,0xe0010032,0xe00101e0};
static DWORD originals[3]={};static bool backed[3]={};
static bool apply(Request* req){
    HMODULE shell=GetModuleHandleW(L"kshell.dll"),main=GetModuleHandleW(L"wpsmain.dll");
    auto core=(MetaGet)GetProcAddress(shell,"?coreApplication@KxApplication@@QBEPAUIKApplication@@XZ");if(!main||!core)return false;
    void* native=core(api.instance());if(!native)return false;
    using Query=LONG (__stdcall*)(void*,const void*,void**);using Get=DWORD (__stdcall*)(void*,DWORD,DWORD);using Set=void (__stdcall*)(void*,DWORD,DWORD);using Release=DWORD (__stdcall*)(void*);
    void* options=nullptr;LONG hr=((Query)(*(void***)native)[23])(native,(char*)main+0x8a1adc,&options);if(hr<0||!options){req->status=-71;return false;}
    auto context=(void (__cdecl*)())((char*)main+0x890a0);void** vt=*(void***)options;bool ok=true;bool changed=false;
    DWORD modifiedIds[3]={};DWORD modifiedPriors[3]={};int modifiedCount=0;
    for(DWORD i=0;i<req->count;i++){
        auto& r=req->records[i];DWORD id,value;int index=-1;
        if(!blue::parseHex(r.name,id)){ok=false;break;}for(int j=0;j<3;j++)if(ids[j]==id)index=j;if(index<0){ok=false;break;}
        context();DWORD prior=((Get)vt[3])(options,id,0);
        if(!strcmp(r.kind,"query")){wsprintfA(r.value,"%08x",prior);continue;}
        if(strcmp(r.kind,"restore")&&strcmp(r.kind,"setting")){ok=false;break;}
        if(!backed[index]){originals[index]=prior;backed[index]=true;}
        if(!strcmp(r.kind,"restore"))value=originals[index];else if(!blue::parseHex(r.value,value)){ok=false;break;}
        if(prior!=value){
            context();((Set)vt[4])(options,id,value);changed=true;
            if(modifiedCount<3){modifiedIds[modifiedCount]=id;modifiedPriors[modifiedCount]=prior;modifiedCount++;}
        }
        context();DWORD actual=((Get)vt[3])(options,id,0);
        wsprintfA(r.value,"%08x",actual);
        if(actual!=value){req->status=-73;ok=false;break;}
    }
    if(!ok&&modifiedCount>0){
        for(int k=modifiedCount-1;k>=0;k--){
            context();((Set)vt[4])(options,modifiedIds[k],modifiedPriors[k]);
        }
        changed=false;
    }
    ((Release)vt[2])(options);
    if(changed){
        auto all=(AllWidgets)GetProcAddress(GetModuleHandleW(L"Qt5WidgetsKso.dll"),"?allWidgets@QApplication@kso_qt@@SA?AV?$QList@PAVQWidget@kso_qt@@@2@XZ");
        auto update=(NoArg)GetProcAddress(GetModuleHandleW(L"Qt5WidgetsKso.dll"),"?update@QWidget@kso_qt@@QAEXXZ");
        auto invalidate=(NoArg)GetProcAddress(shell,"?invalidateViewDirtyRegion@KxView@@UAEXXZ");auto updateView=(NoArg)GetProcAddress(shell,"?updateView@KxView@@IAEXXZ");
        if(all&&update){QStringList list={nullptr};all(&list);for(int i=list.d->begin;i<list.d->end;i++){
            void* w=list.d->array[i];if(!w)continue;void* v=((CastGet)(*(void***)w)[1])(w,"KxView");if(v&&invalidate){invalidate(v);if(updateView)updateView(v);}update(w);
        }blue::releaseWidgets(list);}
    }
    return ok;
}
}
