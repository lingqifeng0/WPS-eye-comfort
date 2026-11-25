// A conservative fallback for WPS buttons whose custom painter ignores the
// skin palette.  It works on the existing QIcon owned by the widget, stores
// the original icon on that same widget, and only replaces neutral dark
// pixels.  Colored semantic icons remain unchanged.  Every source size is
// kept as a separate pixmap; the old single 24x24 conversion made toolbar
// icons look soft on high-DPI displays.
namespace blue {
static bool tintNativeIcons(const QStringList& list,bool restore){
    HMODULE core=GetModuleHandleW(L"Qt5CoreKso.dll"),gui=GetModuleHandleW(L"Qt5GuiKso.dll"),widgets=GetModuleHandleW(L"Qt5WidgetsKso.dll"),shell=GetModuleHandleW(L"kshell.dll");
    using ObjectSet=bool (__thiscall*)(void*,const char*,const void*);
    using ConstGet=const void* (__thiscall*)(const void*);
    using PixmapGet=void (__thiscall*)(void*,void*,int,int,int,int);
    using FromImage=void (__cdecl*)(void*,const void*,int);
    using PixelGet=unsigned (__thiscall*)(const void*,int,int);
    using PixelSet=void (__thiscall*)(void*,int,int,const void*);
    using AddPixmap=void (__thiscall*)(void*,const void*,int,int);
    using ClassGet=const char* (__thiscall*)(const void*);
    using ParentGet=void* (__thiscall*)(const void*);
    auto property=(PropertyGet)GetProcAddress(core,"?property@QObject@kso_qt@@QBE?AVQVariant@2@PBD@Z");
    auto setProperty=(ObjectSet)GetProcAddress(core,"?setProperty@QObject@kso_qt@@QAE_NPBDABVQVariant@2@@Z");
    auto valid=(Visible)GetProcAddress(core,"?isValid@QVariant@kso_qt@@QBE_NXZ");
    auto data=(ConstGet)GetProcAddress(core,"?constData@QVariant@kso_qt@@QBEPBXXZ");
    auto variantDtor=(NoArg)GetProcAddress(core,"??1QVariant@kso_qt@@QAE@XZ");
    auto variantCtor=(NoArg)GetProcAddress(core,"??0QVariant@kso_qt@@QAE@XZ");
    auto iconVariant=(StringGet)GetProcAddress(gui,"??BQIcon@kso_qt@@QBE?AVQVariant@1@XZ");
    auto iconCtor=(NoArg)GetProcAddress(gui,"??0QIcon@kso_qt@@QAE@XZ");
    auto iconDtor=(NoArg)GetProcAddress(gui,"??1QIcon@kso_qt@@QAE@XZ");
    auto pixmapGet=(PixmapGet)GetProcAddress(gui,"?pixmap@QIcon@kso_qt@@QBE?AVQPixmap@2@HHW4Mode@12@W4State@12@@Z");
    auto toImage=(StringGet)GetProcAddress(gui,"?toImage@QPixmap@kso_qt@@QBE?AVQImage@2@XZ");
    auto fromImage=(FromImage)GetProcAddress(gui,"?fromImage@QPixmap@kso_qt@@SA?AV12@ABVQImage@2@V?$QFlags@W4ImageConversionFlag@Qt@kso_qt@@@2@@Z");
    auto pixmapDtor=(NoArg)GetProcAddress(gui,"??1QPixmap@kso_qt@@UAE@XZ");
    auto imageDtor=(NoArg)GetProcAddress(gui,"??1QImage@kso_qt@@UAE@XZ");
    auto width=(SizeGet)GetProcAddress(gui,"?width@QImage@kso_qt@@QBEHXZ");
    auto height=(SizeGet)GetProcAddress(gui,"?height@QImage@kso_qt@@QBEHXZ");
    auto pixel=(PixelGet)GetProcAddress(gui,"?pixel@QImage@kso_qt@@QBEIHH@Z");
    auto setPixel=(PixelSet)GetProcAddress(gui,"?setPixelColor@QImage@kso_qt@@QAEXHHABVQColor@2@@Z");
    auto add=(AddPixmap)GetProcAddress(gui,"?addPixmap@QIcon@kso_qt@@QAEXABVQPixmap@2@W4Mode@12@W4State@12@@Z");
    auto className=(ClassGet)GetProcAddress(core,"?className@QMetaObject@kso_qt@@QBEPBDXZ");
    auto parent=(ParentGet)GetProcAddress(core,"?parent@QObject@kso_qt@@QBEPAV12@XZ");
    auto abstractGet=(StringGet)GetProcAddress(widgets,"?icon@QAbstractButton@kso_qt@@QBE?AVQIcon@2@XZ");
    auto abstractSet=(SetString)GetProcAddress(widgets,"?setIcon@QAbstractButton@kso_qt@@QAEXABVQIcon@2@@Z");
    auto toolGet=(StringGet)GetProcAddress(shell,"?icon@KToolButton@@QBE?AVQIcon@kso_qt@@XZ");
    auto toolSet=(SetString)GetProcAddress(shell,"?setIcon@KToolButton@@QAEXABVQIcon@kso_qt@@@Z");
    if(!property||!setProperty||!valid||!data||!variantDtor||!variantCtor||!iconVariant||!iconCtor||!iconDtor||!pixmapGet||!toImage||!fromImage||!pixmapDtor||!imageDtor||!width||!height||!pixel||!setPixel||!add||!abstractGet||!abstractSet||!toolGet||!toolSet)return false;
    const char* originalKey="_wps_native_eye_tint_source_icon";
    const char* appliedKey="_wps_native_eye_tint_applied";
    for(int i=list.d->begin;i<list.d->end;i++){
        void* w=list.d->array[i];if(!w)continue;void** vt=*(void***)w;
        const char* cls="";
        if(className){void* meta=((MetaGet)vt[0])(w);if(meta)cls=className(meta);}
        // Style-gallery entries are complete preview cards.  Their labels and
        // sample text live inside the same pixmap as the card background;
        // tinting those pixels turns the labels into white-on-white blanks.
        // Leave the gallery subtree's source icons untouched and let the
        // native theme resolver paint its own readable card.
        bool gallery=false;
        for(void* node=w;node&&parent;node=parent(node)){
            const char* name="";void* meta=((MetaGet)(*(void***)node)[0])(node);if(className&&meta)name=className(meta);
            if(strstr(name,"Gallery")||strstr(name,"gallery")||strstr(name,"ListView")||strstr(name,"KListView")){gallery=true;break;}
        }
        if(gallery)continue;
        // WPS paints its native title controls through the same button path,
        // but their dark source glyphs are often brighter navy than ribbon
        // glyphs.  Mark those classes so they get the stronger high-DPI tint.
        bool titleButton=strcmp(cls,"KPromeMinButton")==0||
            strcmp(cls,"KPromeRestoreMaxButton")==0||
            strcmp(cls,"KPromeCloseButton")==0||
            strcmp(cls,"KWindowMultiTitleButton")==0;
        void* button=((CastGet)vt[1])(w,"QAbstractButton");StringGet get=abstractGet;SetString set=abstractSet;
        if(!button){button=((CastGet)vt[1])(w,"KToolButton");get=toolGet;set=toolSet;}
        if(!button)continue;
        alignas(8) char source[64]={},applied[64]={};property(w,source,originalKey);property(w,applied,appliedKey);
        bool hasSource=valid(source),hasApplied=valid(applied);
        if(restore){
            if(hasSource&&hasApplied)set(button,(const QString*)data(source));
            alignas(8) char empty[64]={};variantCtor(empty);setProperty(w,originalKey,empty);setProperty(w,appliedKey,empty);variantDtor(empty);
            variantDtor(applied);variantDtor(source);continue;
        }
        alignas(8) char current[64]={};get(button,(QString*)current);
        if(!hasSource){iconVariant(current,(QString*)source);setProperty(w,originalKey,source);hasSource=true;}
        if(hasApplied){variantDtor(current);variantDtor(applied);variantDtor(source);continue;}
        const void* original=data(source);
        alignas(8) char result[64]={};iconCtor(result);bool changed=false;
        // Normal, disabled, active and selected; both off/on states.  Keep a
        // real source-sized copy for each common WPS toolbar size instead of
        // stretching one low-resolution 24px raster across every state.
        const int sizes[]={16,20,24,32,48,64};
        for(int si=0;si<(int)(sizeof(sizes)/sizeof(sizes[0]));si++){
            int size=sizes[si];
            for(int mode=0;mode<4;mode++)for(int state=0;state<2;state++){
                alignas(8) char pix[128]={},img[128]={};pixmapGet((void*)original,pix,size,size,mode,state);toImage(pix,(QString*)img);
                int ww=width(img),hh=height(img);bool local=false;
                if(ww>0&&hh>0&&ww<=256&&hh<=256){
                    for(int y=0;y<hh;y++)for(int x=0;x<ww;x++){
                        unsigned p=pixel(img,x,y);int a=(int)(p>>24),r=(p>>16)&255,g=(p>>8)&255,b=p&255;
                        int hi=r>g?r:g;hi=hi>b?hi:b;int lo=r<g?r:g;lo=lo<b?lo:b;
                        // A neutral dark source pixel is the black/gray icon
                        // stroke.  Keep colored marks and transparent pixels.
                        // WPS's dark skin uses navy/blue strokes as well as
                        // neutral gray, so a hue-only test misses those
                        // strokes.  Bright semantic marks remain untouched.
                        if(a>24&&((titleButton&&hi<250)||(hi<175&&hi-lo<160))){alignas(8) char c[32]={};makeColor(c,foreground,a);setPixel(img,x,y,c);local=true;}
                    }
                    if(local){alignas(8) char revised[128]={};fromImage(revised,img,0);add(result,revised,mode,state);pixmapDtor(revised);changed=true;}
                    else add(result,pix,mode,state);
                }
                imageDtor(img);pixmapDtor(pix);
            }
        }
        if(changed){set(button,(const QString*)result);alignas(8) char marker[64]={};variantCtor(marker);setProperty(w,appliedKey,marker);variantDtor(marker);}
        iconDtor(result);iconDtor(current);variantDtor(applied);variantDtor(source);
    }
    return true;
}
}
