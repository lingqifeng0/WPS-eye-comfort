namespace blue {
static bool hookCreated=false;
static void* statusPaintOriginal=nullptr;static void* ribbonPaintOriginal=nullptr;
static void* coreDarkOriginal=nullptr;static void* promeDarkOriginal=nullptr;
static void* needBackgroundOriginal=nullptr;
static void* panePaintOriginal=nullptr;
static void* drawControlOriginal=nullptr;
// Popup controls in this WPS build do not ask KThemeLoader directly.  Their
// paint routines call these two free helpers, which return a QColor by hidden
// sret pointer.  Hooking the helpers is what reaches the font/size lists after
// they are created, without repainting over the list text ourselves.
static void* drawHelpColorOriginal=nullptr;static void* drawHelperColorOriginal=nullptr;
using DrawHelpColorFn=void* (__cdecl*)(void*,const void*,const QString*,const QString*,const void*,bool);
using DrawHelperColorFn=void* (__cdecl*)(void*,const QString*,const QString*,const void*,bool);
static bool popupName(const char* name){
    return has(name,"popup")||has(name,"list-background")||has(name,"list-text")||
        has(name,"menu-background")||has(name,"menu-text");
}
static void* __cdecl drawHelpColor(void* out,const void* widget,const QString* key,const QString* style,const void* fallback,bool resolve){
    void* result=((DrawHelpColorFn)drawHelpColorOriginal)(out,widget,key,style,fallback,resolve);
    if(!enabled||!out||!key||!style)return result;
    char name[256]={},klass[256]={};textOf(*key,name,256);textOf(*style,klass,256);lower(name);lower(klass);
    if(has(klass,"kpopupwidget")||has(klass,"popupwidget")||popupName(name)){
        bool text=has(name,"text")||has(name,"foreground")||has(name,"font");
        bool selected=has(name,"selected")||has(name,"highlight")||has(name,"hover");
        if(text)makeColor(out,selected?foreground:foreground);
        else if(has(name,"border")||has(name,"line"))makeColor(out,border);
        else makeColor(out,selected?hover:workspace);
    }
    return result;
}
static void* __cdecl drawHelperColor(void* out,const QString* key,const QString* style,const void* fallback,bool resolve){
    void* result=((DrawHelperColorFn)drawHelperColorOriginal)(out,key,style,fallback,resolve);
    if(!enabled||!out||!key||!style)return result;
    char name[256]={},klass[256]={};textOf(*key,name,256);textOf(*style,klass,256);lower(name);lower(klass);
    if(has(klass,"klitecombobox")||has(klass,"popup")||popupName(name)){
        bool text=has(name,"text")||has(name,"foreground")||has(name,"font");
        bool selected=has(name,"selected")||has(name,"highlight")||has(name,"hover");
        if(text)makeColor(out,selected?foreground:foreground);
        else if(has(name,"border")||has(name,"line"))makeColor(out,border);
        else makeColor(out,selected?hover:workspace);
    }
    return result;
}
// WPS's document view falls back to the process system colors for an active
// text selection.  A Qt palette/stylesheet only changes ordinary widgets, so
// the default Windows highlight (white on this machine) still leaked through
// on the black page.  Hook these two user32 calls inside the WPS process so
// the selection is deep blue without changing Windows' system colors for any
// other application.
static void* sysColorOriginal=nullptr;static void* sysColorBrushOriginal=nullptr;
static HBRUSH selectionBrush=nullptr;static HBRUSH selectionTextBrush=nullptr;
using SysColorFn=DWORD (WINAPI*)(int);
using SysColorBrushFn=HBRUSH (WINAPI*)(int);
static DWORD __stdcall sysColor(int index){
    DWORD value=((SysColorFn)sysColorOriginal)(index);
    if(enabled&&index==COLOR_HIGHLIGHT)return RGB(hover.r,hover.g,hover.b);
    if(enabled&&index==COLOR_HIGHLIGHTTEXT)return RGB(foreground.r,foreground.g,foreground.b);
    return value;
}
static HBRUSH __stdcall sysColorBrush(int index){
    if(enabled&&index==COLOR_HIGHLIGHT){
        if(!selectionBrush)selectionBrush=CreateSolidBrush(RGB(hover.r,hover.g,hover.b));
        return selectionBrush;
    }
    if(enabled&&index==COLOR_HIGHLIGHTTEXT){
        if(!selectionTextBrush)selectionTextBrush=CreateSolidBrush(RGB(foreground.r,foreground.g,foreground.b));
        return selectionTextBrush;
    }
    return ((SysColorBrushFn)sysColorBrushOriginal)(index);
}
static void releaseSelectionBrushes(){
    if(selectionBrush){DeleteObject(selectionBrush);selectionBrush=nullptr;}
    if(selectionTextBrush){DeleteObject(selectionTextBrush);selectionTextBrush=nullptr;}
}
static void paintBackground(void*,bool=false,int top=0);
static bool __fastcall needBackground(void* self,void*){
    // This three-byte WPS function is folded with unrelated true-returning
    // functions. Suppress only its exact KxView background call site.
#ifdef _MSC_VER
    void* caller=_ReturnAddress();
#else
    void* caller=__builtin_return_address(0);
#endif
    if(enabled&&caller==(char*)GetModuleHandleW(L"kshell.dll")+0x62ea1c)return false;
    return ((bool (__thiscall*)(void*))needBackgroundOriginal)(self);
}
static void __fastcall panePaint(void* self,void*,void* event){if(enabled)paintBackground(self,true);else ((void (__thiscall*)(void*,void*))panePaintOriginal)(self,event);}
static void* styleBackgroundOriginal=nullptr;
static void* __fastcall styleBackground(void* self,void*,void* out,bool first,bool second){
    return ((void* (__thiscall*)(void*,void*,bool,bool))styleBackgroundOriginal)(self,out,first,second);
}
// Keep WPS's own dark-skin branch selection.  The frame is recolored through
// the scoped palettes and native paint hooks; forcing the global dark branch
// makes the style-gallery model skip its item labels in this WPS build.
static bool __fastcall coreDark(void* self,void*){return ((bool (__thiscall*)(void*))coreDarkOriginal)(self);}
// The outer frame is recolored explicitly below.  Do not force the Prome
// title-bar skin into WPS's dark-mode branch: that branch suppresses the
// native min/max/close glyphs on this WPS build.  Keeping the original Prome
// state preserves its title-button layout while the palette hooks still make
// the frame deep blue.
static bool __fastcall promeDark(void* self,void*){return ((bool (__thiscall*)(void*))promeDarkOriginal)(self);}
static void paintBackground(void* widget,bool pane,int top){
    HMODULE gui=GetModuleHandleW(L"Qt5GuiKso.dll"),widgets=GetModuleHandleW(L"Qt5WidgetsKso.dll");
    using Ctor=void (__thiscall*)(void*,void*);
    using RectGet=void (__thiscall*)(const void*,void*);
    using SetTop=void (__thiscall*)(void*,int);
    using Fill=void (__thiscall*)(void*,const void*,const void*);
    auto ctor=(Ctor)GetProcAddress(gui,"??0QPainter@kso_qt@@QAE@PAVQPaintDevice@1@@Z");
    auto dtor=(NoArg)GetProcAddress(gui,"??1QPainter@kso_qt@@QAE@XZ");
    auto rect=(RectGet)GetProcAddress(widgets,"?rect@QWidget@kso_qt@@QBE?AVQRect@2@XZ");
    auto setTop=(SetTop)GetProcAddress(gui,"?setTop@QRect@kso_qt@@QAEXH@Z");
    auto fill=(Fill)GetProcAddress(gui,"?fillRect@QPainter@kso_qt@@QAEXABVQRect@2@ABVQColor@2@@Z");
    if(!ctor||!dtor||!rect||!fill)return;
    alignas(8) char painter[128]={},bounds[32]={},color[32]={};
    makeColor(color,pane?workspace:background);rect(widget,bounds);
    if(top>0&&setTop)setTop(bounds,top);
    ctor(painter,(char*)widget+8);fill(painter,bounds,color);dtor(painter);
}
// WPS paints the parent ribbon/status backgrounds as a separate light layer;
// drawing that original pass would cover the dark skin and hide navigation
// labels.  Keep the parent fill suppressed while child controls are refreshed
// with the palette and icon bindings below.
static void __fastcall statusPaint(void* self,void*,void* event){if(enabled)paintBackground(self);else ((void (__thiscall*)(void*,void*))statusPaintOriginal)(self,event);}
static void __fastcall ribbonPaint(void* self,void*,void* event){
    if(!enabled){((void (__thiscall*)(void*,void*))ribbonPaintOriginal)(self,event);return;}
    // KRbTabWidget paints both the tab strip and the command ribbon from one
    // parent event.  Keep the native pass for the tab captions, then darken
    // only the lower ribbon body; the tab buttons are painted afterwards and
    // retain their original high-resolution text/icons.
    ((void (__thiscall*)(void*,void*))ribbonPaintOriginal)(self,event);
    paintBackground(self,true,48);
}
// WPS's Prome title buttons use a custom style painter.  Their hit targets
// and geometry are native, but the glyph color is read from a separate theme
// table and remains navy after the frame is switched to deep blue.  Repaint
// only the three native glyphs after WPS's own pass; this keeps the buttons as
// icons and leaves their mouse behavior and layout untouched.
static const char* widgetClassName(const void* widget){
    if(!widget)return "";
    HMODULE core=GetModuleHandleW(L"Qt5CoreKso.dll");
    using ClassGet=const char* (__thiscall*)(const void*);
    auto className=(ClassGet)GetProcAddress(core,"?className@QMetaObject@kso_qt@@QBEPBDXZ");
    if(!className)return "";
    void** vt=*(void***)widget;void* meta=((MetaGet)vt[0])((void*)widget);
    return meta?className(meta):"";
}
static void drawTitleIcon(void* painter,const void* widget,int kind){
    if(!painter||!widget)return;
    HMODULE core=GetModuleHandleW(L"Qt5CoreKso.dll"),gui=GetModuleHandleW(L"Qt5GuiKso.dll"),widgets=GetModuleHandleW(L"Qt5WidgetsKso.dll");
    using RectGet=void (__thiscall*)(const void*,void*);
    using RectCoords=void (__thiscall*)(const void*,int*,int*,int*,int*);
    using SetHint=void (__thiscall*)(void*,int,bool);
    using SetPen=void (__thiscall*)(void*,const void*);
    using DrawLine=void (__thiscall*)(void*,int,int,int,int);
    using Save=void (__thiscall*)(void*);
    auto rect=(RectGet)GetProcAddress(widgets,"?rect@QWidget@kso_qt@@QBE?AVQRect@2@XZ");
    auto coords=(RectCoords)GetProcAddress(core,"?getRect@QRect@kso_qt@@QBEXPAH000@Z");
    auto hint=(SetHint)GetProcAddress(gui,"?setRenderHint@QPainter@kso_qt@@QAEXW4RenderHint@12@_N@Z");
    auto pen=(SetPen)GetProcAddress(gui,"?setPen@QPainter@kso_qt@@QAEXABVQColor@2@@Z");
    auto line=(DrawLine)GetProcAddress(gui,"?drawLine@QPainter@kso_qt@@QAEXHHHH@Z");
    auto save=(Save)GetProcAddress(gui,"?save@QPainter@kso_qt@@QAEXXZ");
    auto restore=(Save)GetProcAddress(gui,"?restore@QPainter@kso_qt@@QAEXXZ");
    if(!rect||!coords||!hint||!pen||!line)return;
    alignas(8) char bounds[32]={};rect(widget,bounds);
    int x=0,y=0,w=0,h=0;coords(bounds,&x,&y,&w,&h);if(w<12||h<12)return;
    if(save)save(painter);
    alignas(8) char white[32]={};makeColor(white,foreground);hint(painter,1,true);pen(painter,white);
    int cx=x+w/2,cy=y+h/2;int half=(w<h?w:h)>=40?10:7;
    auto hline=[&](int x1,int y1,int x2){line(painter,x1,y1,x2,y1);line(painter,x1,y1+1,x2,y1+1);};
    auto vline=[&](int x1,int y1,int y2){line(painter,x1,y1,x1,y2);line(painter,x1+1,y1,x1+1,y2);};
    if(kind==0){
        // WPS's native minimize bar sits just above the widget midpoint.
        // Two adjacent strokes keep it solid at both 100% and high-DPI scale.
        hline(cx-half,cy-1,cx+half);
    }else if(kind==1){
        hline(cx-half+2,cy-half,cx+half);
        vline(cx-half,cy-half+2,cy+half);
        hline(cx-half,cy+half,cx+half-2);
        vline(cx+half,cy-half,cy+half-2);
        int ox=half/2,oy=half/2;
        hline(cx-half+ox,cy-half+oy,cx+half-2);
        vline(cx-half+ox,cy-half+oy,cy+half-2);
    }else{
        line(painter,cx-half,cy-half,cx+half,cy+half);
        line(painter,cx-half+1,cy-half,cx+half+1,cy+half);
        line(painter,cx+half,cy-half,cx-half,cy+half);
        line(painter,cx+half-1,cy-half,cx-half-1,cy+half);
    }
    if(restore)restore(painter);
}
static void __fastcall drawControl(void* self,void*,int element,const void* option,void* painter,const void* widget){
    ((void (__thiscall*)(void*,int,const void*,void*,const void*))drawControlOriginal)(self,element,option,painter,widget);
    if(!enabled||!widget)return;
    const char* cls=widgetClassName(widget);int kind=-1;
    if(strcmp(cls,"KPromeMinButton")==0)kind=0;
    else if(strcmp(cls,"KPromeRestoreMaxButton")==0)kind=1;
    else if(strcmp(cls,"KPromeCloseButton")==0)kind=2;
    if(kind>=0)drawTitleIcon(painter,widget,kind);
}
static bool nativeHooks(bool restore){
    if(restore){releaseSelectionBrushes();return !hookCreated||MH_DisableHook(MH_ALL_HOOKS)==MH_OK;}
    if(!hookCreated){
        MH_STATUS init=MH_Initialize();if(init!=MH_OK&&init!=MH_ERROR_ALREADY_INITIALIZED)return false;
        HMODULE lite=GetModuleHandleW(L"ksolite.dll");
        void* c=(void*)GetProcAddress(lite,"?getColor@KThemeLoader@@UAE?AVQColor@kso_qt@@ABVQString@3@0ABV23@PA_N_N@Z");
        void* g=(void*)GetProcAddress(lite,"?getGradient@KThemeLoader@@UAE?BVQGradient@kso_qt@@ABVQString@3@0ABV23@PA_N@Z");
        if(!c||!g)return false;
        if(MH_CreateHook(c,(void*)&color,&nativeColorOriginal)!=MH_OK)return false;
        if(MH_CreateHook(g,(void*)&gradient,&nativeGradientOriginal)!=MH_OK){MH_RemoveHook(c);return false;}
        HMODULE shell=GetModuleHandleW(L"kshell.dll"),prome=GetModuleHandleW(L"kprometheus.dll");
        // Recolor through WPS/Qt's native theme and palette paths.  Do not
        // hook parent paint events or QStyle::drawControl: those hooks paint
        // a second rectangle over the ribbon and make the tabs appear to
        // change only after a click.  Native controls keep their geometry,
        // icons and hit targets while their colors come from the profile.
        void* iconTheme=(void*)GetProcAddress(lite,"?getIconThemeColor@KThemeLoader@@UAE?AV?$QList@VQColor@kso_qt@@@kso_qt@@ABVQString@3@ABVQColor@3@@Z");
        void* core=(void*)GetProcAddress(shell,"?isDarkSkin@KAppSkin@@QAE_NXZ");
        void* promeSkin=(void*)GetProcAddress(prome,"?isDarkSkin@KPromeSkin@@QBE_NXZ");
        void* style=(void*)GetProcAddress(shell,"?getBackColor@KStylesGalleryModel@@UAE?AVQColor@kso_qt@@_N0@Z");
        if(!iconTheme||!core||!promeSkin||!style||
           MH_CreateHook(iconTheme,(void*)&iconThemeColor,&nativeIconThemeOriginal)!=MH_OK||
           MH_CreateHook(core,(void*)&coreDark,&coreDarkOriginal)!=MH_OK||
           MH_CreateHook(promeSkin,(void*)&promeDark,&promeDarkOriginal)!=MH_OK||
           MH_CreateHook(style,(void*)&styleBackground,&styleBackgroundOriginal)!=MH_OK)return false;
        HMODULE user32=GetModuleHandleW(L"user32.dll");
        void* sys=(void*)GetProcAddress(user32,"GetSysColor");
        void* brush=(void*)GetProcAddress(user32,"GetSysColorBrush");
        if(!sys||!brush||MH_CreateHook(sys,(void*)&sysColor,&sysColorOriginal)!=MH_OK||
           MH_CreateHook(brush,(void*)&sysColorBrush,&sysColorBrushOriginal)!=MH_OK)return false;
        void* drawHelp=(void*)GetProcAddress(shell,"?getColorFromTheme@KDrawHelpFunc@@YA?AVQColor@kso_qt@@PBVQWidget@3@ABVQString@3@1ABV23@_N@Z");
        void* drawHelper=(void*)GetProcAddress(lite,"?getColorFromTheme@KDrawHelper@@YA?AVQColor@kso_qt@@ABVQString@3@0ABV23@_N@Z");
        if(!drawHelp||!drawHelper||MH_CreateHook(drawHelp,(void*)&drawHelpColor,&drawHelpColorOriginal)!=MH_OK||
           MH_CreateHook(drawHelper,(void*)&drawHelperColor,&drawHelperColorOriginal)!=MH_OK)return false;
        hookCreated=true;
    }
    MH_STATUS status=MH_EnableHook(MH_ALL_HOOKS);return status==MH_OK||status==MH_ERROR_ENABLED;
}
}
