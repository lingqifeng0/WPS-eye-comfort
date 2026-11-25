#include "../vendor/minhook/include/MinHook.h"
#include "profile.h"
namespace blue {
using QueryColor=void* (__thiscall*)(void*,void*,const QString*,const QString*,const void*,bool*,bool);
using QueryGradient=void* (__thiscall*)(void*,void*,const QString*,const QString*,const void*,bool*);
using QueryIcons=void* (__thiscall*)(void*,QStringList*,const QString*,const void*);
// KThemeLoader::getIconThemeColor returns QList<QColor> by hidden sret on x86.
// Keep this separate from the older getIcons(QStringList) hook: the two list
// element layouts are different and must never share a callback.
using QueryIconThemeColor=void* (__thiscall*)(void*,void*,const QString*,const void*);
using ColorCtor=void (__thiscall*)(void*,int,int,int,int);
using ColorInt=int (__thiscall*)(const void*);
using GradientCtor=void (__thiscall*)(void*,double,double,double,double);
using GradientCopy=void (__thiscall*)(void*,const void*);
using SetStop=void (__thiscall*)(void*,double,const void*);
using SetInt=void (__thiscall*)(void*,int);
static ColorCtor colorCtor;static ColorInt red,green,blueValue,alpha;
static GradientCtor gradientCtor;static GradientCopy gradientCopy;static SetStop setStop;static SetInt coordinate;static NoArg gradientDtor;
static void* nativeColorOriginal=nullptr;static void* nativeGradientOriginal=nullptr;static void* nativeIconOriginal=nullptr;static void* nativeIconThemeOriginal=nullptr;
static bool nativeHooks(bool);static bool widgetPalettes(const QStringList&,bool);static bool bindNativeIcons(const QStringList&,bool);static bool tintNativeIcons(const QStringList&,bool);static bool scrollbars(const QStringList&,bool);
static bool has(const char* value,const char* part){return strstr(value,part)!=nullptr;}
static void lower(char* s){for(;*s;s++)if(*s>='A'&&*s<='Z')*s+=32;}
static void makeColor(void* out,Rgb value,int a=255){colorCtor(out,value.r,value.g,value.b,a);}
static void* __fastcall color(void* self,void*,void* out,const QString* cls,const QString* key,const void* fallback,bool* found,bool resolve){
    ((QueryColor)nativeColorOriginal)(self,out,cls,key,fallback,found,resolve);
    if(!enabled)return out;int a=alpha(out);if(!a)return out;
    char name[512],style[512];textOf(*key,name,512);textOf(*cls,style,512);lower(name);lower(style);
    if(has(style,"bluescreen")||has(style,"paper"))return out;
    int r=red(out),g=green(out),b=blueValue(out),hi=r>g?r:g,lo=r<g?r:g;hi=hi>b?hi:b;lo=lo<b?lo:b;
    bool bg=has(name,"background")||has(name,"bgcolor")||has(name,"fill")||has(name,"window")||has(name,"buttonface");
    bool text=has(name,"text")||has(name,"foreground")||has(name,"icon")||has(name,"font")||has(name,"title");
    bool line=has(name,"border")||has(name,"line")||has(name,"separator");
    bool disabled=has(name,"disable");Rgb value={r,g,b};
    bool control=has(style,"button")||has(style,"tool")||has(style,"status")||has(style,"menu")||has(style,"zoom")||has(style,"lite")||has(style,"window")||has(style,"title")||has(style,"toolbar")||has(style,"ribbon")||has(style,"rb")||has(style,"tab")||has(style,"gallery")||has(style,"label")||has(style,"combo")||has(style,"list")||has(style,"view")||has(style,"pane")||has(style,"category")||has(style,"quick");
    // The style gallery in this WPS build is backed by KListView rather than
    // a class name containing "gallery".  Give its item labels the dark card
    // foreground that matches the light gallery card; otherwise the global
    // dark palette leaves the labels in the same pale/low-contrast tone as
    // the surrounding chrome and they look as if they were swallowed.
    bool gallery=has(style,"kxwpsstylegallery")||has(style,"stylegallery")||
        has(style,"gallery")||has(style,"klistview")||
        has(style,"kstylesgallery")||has(style,"galleryview");
    // Font and font-size pickers are separate popup widgets.  Their list
    // delegates use keys such as list-background/list-text and otherwise
    // keep the light WPS card while the surrounding ribbon is dark.
    bool popup=has(style,"kpopupwidgetex")||has(style,"kpopupwidget")||
        has(style,"popupwidget")||has(style,"popupmenu")||has(style,"popup");
    bool field=!strcmp(style,"kcombobox")||!strcmp(style,"klitecombobox")||has(style,"kxfontsizepreviewcombobox");
    // WPS's document selection is painted by the native theme resolver, not
    // by a screen filter.  Use the same deep-blue hover tone as the ribbon so
    // a selected range stays legible on the black page instead of becoming a
    // white flash.
    // The document renderer uses the short KDocument key "Selection" for
    // the actual text-selection fill.  It is not a QWidget palette role, so
    // matching only the longer Qt names leaves the native selection white.
    bool selectionBg=!strcmp(name,"selection")||has(name,"background-highlight")||
        has(name,"highlight-background")||has(name,"selection-background")||
        has(name,"background-select")||has(name,"background-selected")||
        has(name,"text-content-highlight-background");
    bool selectionText=has(name,"text-highlight")||has(name,"highlight-text")||
        has(name,"highlighted-text")||has(name,"selection-text");
    if(selectionBg)value=hover;
    else if(selectionText)value=foreground;
    else if(popup&&(bg||has(name,"list-background")))value=workspace;
    else if(popup&&(text||has(name,"list-text")))value=disabled?muted:foreground;
    else if(has(name,"list-background"))value=workspace;
    else if(has(name,"list-text"))value=disabled?muted:foreground;
    else if(field&&bg)value=card;
    else if(field&&text)value=disabled?muted:cardText;
    else if(gallery&&bg)value=card;
    else if(gallery&&text)value=cardText;
    else if(has(style,"iconcolor")&&hi-lo<55)value=disabled?muted:foreground;
    else if(has(name,"handle")||has(name,"thumb"))value=thumb;
    else if(bg)value=has(name,"hover")||has(name,"down")||has(name,"selected")?hover:background;
    else if(text&&hi-lo<120)value=disabled?muted:foreground;
    else if(line&&hi-lo<55)value=border;
    // WPS custom buttons often use keys such as "topline", "cross",
    // "arrow" or "other" instead of the word "text".  Treat neutral colors
    // in those control styles as foreground; mapping them to the dark frame
    // background is what made the icons disappear in the previous build.
    else if(control&&hi-lo<90)value=disabled?muted:foreground;
    else if(hi-lo<40)value=bg?background:(r+g+b)/3>130?background:disabled?muted:foreground;
    makeColor(out,value,a);if(found)*found=true;return out;
}
static void* __fastcall gradient(void* self,void*,void* out,const QString* cls,const QString* key,const void* fallback,bool* found){
    ((QueryGradient)nativeGradientOriginal)(self,out,cls,key,fallback,found);if(!enabled)return out;
    char name[512],style[512];textOf(*key,name,512);textOf(*cls,style,512);lower(name);lower(style);
    if(has(style,"bluescreen")||has(style,"paper")||(!has(name,"background")&&!has(name,"bg")))return out;
    alignas(8) char grad[256]={},c[32]={};gradientCtor(grad,0.,0.,0.,1.);coordinate(grad,2);
    makeColor(c,has(name,"hover")||has(name,"down")||has(name,"selected")?hover:background);
    setStop(grad,0.,c);setStop(grad,1.,c);gradientDtor(out);gradientCopy(out,grad);gradientDtor(grad);if(found)*found=true;return out;
}
static void* __fastcall icons(void* self,void*,QStringList* out,const QString* key,const void* fallback){
    ((QueryIcons)nativeIconOriginal)(self,out,key,fallback);
    if(enabled&&out->d&&out->d->ref==1)for(int i=out->d->begin;i<out->d->end;i++){
        void* c=out->d->array[i];if(!c)continue;int r=red(c),g=green(c),b=blueValue(c),a=alpha(c),hi=r>g?r:g,lo=r<g?r:g;hi=hi>b?hi:b;lo=lo<b?lo:b;
        // This is a color list, not a raster image.  Replacing a neutral
        // entry here keeps WPS's vector/device-pixel rendering intact while
        // covering the darker navy/gray strokes used by the ribbon icons.
        if(a)makeColor(c,foreground,a);
    }
    return out;
}
// The WPS toolbar and status-bar controls request their neutral icon colors
// through KThemeLoader::getIconThemeColor.  This is a QList<QColor>, whose
// elements are inline 16-byte QColor values; it is not the pointer list used
// by getIcons above.  Recolor only neutral/dark entries and leave colored
// semantic icons (red, yellow, blue, etc.) untouched.
static void* __fastcall iconThemeColor(void* self,void*,void* out,const QString* key,const void* fallback){
    ((QueryIconThemeColor)nativeIconThemeOriginal)(self,out,key,fallback);

    if(!enabled||!out)return out;
    QListData* d=((QStringList*)out)->d;
    int count=d?d->end-d->begin:0;
    if(!d||d->ref!=1||count<=0||count>64)return out;
    char name[512]={};textOf(*key,name,512);lower(name);
    bool disabled=has(name,"disable")||has(name,"inactive");
    bool semantic=has(name,"colorred")||has(name,"colorgreen")||has(name,"colorblue")||
        has(name,"colororange")||has(name,"colorpurple")||has(name,"coloryellow");
    bool iconRole=has(name,"icon")||has(name,"stylebase")||has(name,"styletheme")||
        has(name,"styledelete")||has(name,"colorblack")||has(name,"colorgray");
    Rgb target=disabled?muted:foreground;
    // QColor is 16 bytes in the WPS Qt5 build.  QList stores movable value
    // types inline in its data array, indexed by begin/end.
    for(int i=d->begin;i<d->end;i++){
        void* c=(char*)d->array+i*16;
        int a=alpha(c);if(!a)continue;
        int r=red(c),g=green(c),b=blueValue(c),hi=r>g?r:g,lo=r<g?r:g;
        hi=hi>b?hi:b;lo=lo<b?lo:b;
        // WPS names the normal toolbar strokes with iconbase/stylebase keys;
        // those can be dark blue rather than neutral gray, so the old
        // saturation-only test left them nearly invisible on the new frame.
        // Preserve explicitly semantic color entries (red/green/blue/etc.).
        // The frame uses the same native color table for both neutral gray
        // and dark-blue strokes.  Recolor every non-semantic entry so the
        // toolbar icons stay visible without replacing their QIcon pixmaps.
        if(!semantic)makeColor(c,target,a);
    }
    return out;
}
static bool load(){
    HMODULE gui=GetModuleHandleW(L"Qt5GuiKso.dll"),widgets=GetModuleHandleW(L"Qt5WidgetsKso.dll");
#define G(field,type,symbol) field=(type)GetProcAddress(gui,symbol);if(!field)return false
    G(colorCtor,ColorCtor,"??0QColor@kso_qt@@QAE@HHHH@Z");
    G(red,ColorInt,"?red@QColor@kso_qt@@QBEHXZ");G(green,ColorInt,"?green@QColor@kso_qt@@QBEHXZ");
    G(blueValue,ColorInt,"?blue@QColor@kso_qt@@QBEHXZ");G(alpha,ColorInt,"?alpha@QColor@kso_qt@@QBEHXZ");
    G(gradientCtor,GradientCtor,"??0QLinearGradient@kso_qt@@QAE@NNNN@Z");G(gradientCopy,GradientCopy,"??0QGradient@kso_qt@@QAE@ABV01@@Z");
    G(gradientDtor,NoArg,"??1QGradient@kso_qt@@QAE@XZ");G(setStop,SetStop,"?setColorAt@QGradient@kso_qt@@QAEXNABVQColor@2@@Z");
    G(coordinate,SetInt,"?setCoordinateMode@QGradient@kso_qt@@QAEXW4CoordinateMode@12@@Z");
#undef G
    return true;
}
static void releaseWidgets(QStringList& list){if(list.d&&list.d->ref!=-1&&InterlockedDecrement(&list.d->ref)==0)api.dispose(list.d);list.d=nullptr;}
alignas(8) static char originalBase[32]={};static bool baseBacked=false;
alignas(8) static char originalAppPalette[64]={};static bool appPaletteBacked=false;
static QString originalCss={nullptr};static bool cssBacked=false;
static bool nativeBase(bool restore){
    HMODULE shell=GetModuleHandleW(L"kshell.dll");
    auto skin=(MetaGet)GetProcAddress(shell,"?skin@KApplication@@QAEPAVKAppSkin@@XZ");auto theme=(MetaGet)GetProcAddress(shell,"?getTheme@KAppSkin@@QAEPAVIKThememLoader@@XZ");
    auto update=(NoArg)GetProcAddress(shell,"?updateUiColor@KApplication@@QAEXXZ");if(!skin||!theme||!update)return false;
    void* app=api.instance();void* t=theme(skin(app));if(!t)return false;void** vt=*(void***)t;
    using Get=void* (__thiscall*)(void*,void*);using Set=void (__thiscall*)(void*,const void*);
    if(!baseBacked){((Get)vt[9])(t,originalBase);baseBacked=true;}
    // Do not replace KAppSkin's global base color.  The WPS gallery derives
    // its preview item text/background from this skin object and loses the
    // labels when the whole application base is forced to the frame color.
    // The parent paint hooks below provide the deep-blue chrome locally.
    if(false){alignas(8) char value[32]={};makeColor(value,workspace);((Set)vt[10])(t,restore?originalBase:value);}
    // The ribbon/status paint handlers read QApplication's palette directly
    // instead of the individual widget palettes.  Keep a copy and provide
    // the same dark palette at that level so WPS can draw its own labels and
    // gallery items without reintroducing the white default bands.
    HMODULE gui=GetModuleHandleW(L"Qt5GuiKso.dll"),widgets=GetModuleHandleW(L"Qt5WidgetsKso.dll");
    using AppPaletteGet=void (__cdecl*)(void*,const char*);
    using AppPaletteSet=void (__cdecl*)(const void*,const char*);
    using PaletteSet=void (__thiscall*)(void*,int,int,const void*);
    auto appGet=(AppPaletteGet)GetProcAddress(widgets,"?palette@QApplication@kso_qt@@SA?AVQPalette@2@PBD@Z");
    auto appSet=(AppPaletteSet)GetProcAddress(widgets,"?setPalette@QApplication@kso_qt@@SAXABVQPalette@2@PBD@Z");
    auto palCtor=(NoArg)GetProcAddress(gui,"??0QPalette@kso_qt@@QAE@XZ");
    auto palDtor=(NoArg)GetProcAddress(gui,"??1QPalette@kso_qt@@QAE@XZ");
    auto palSet=(PaletteSet)GetProcAddress(gui,"?setColor@QPalette@kso_qt@@QAEXW4ColorGroup@12@W4ColorRole@12@ABVQColor@2@@Z");
    // Keep QApplication's global palette untouched.  WPS's style gallery
    // delegates use that palette for their preview labels; replacing it with
    // the frame palette makes the whole gallery a blank light card.  The
    // scoped widget palettes and theme resolver below are sufficient for the
    // outer chrome and avoid touching document selection metrics.
    if(appGet&&appSet&&palCtor&&palDtor&&palSet){
        if(!appPaletteBacked){appGet(originalAppPalette,nullptr);appPaletteBacked=true;}
        if(!restore){
            alignas(8) char appPal[64]={},bg[32]={},fg[32]={},muted[32]={};palCtor(appPal);
            alignas(8) char selection[32]={};makeColor(selection,hover);
            makeColor(bg,background);makeColor(fg,foreground);makeColor(muted,blue::muted);
            for(int role=0;role<21;role++){if(role==17)continue;bool text=role==0||role==6||role==7||role==8||role==13||role==19||role==20;const void* normal=role==12?&selection:(role==14||role==15?&fg:(text?&fg:&bg));palSet(appPal,0,role,normal);palSet(appPal,2,role,normal);palSet(appPal,1,role,text?&muted:&bg);}
            appSet(appPal,nullptr);palDtor(appPal);
        }else{appSet(originalAppPalette,nullptr);palDtor(originalAppPalette);appPaletteBacked=false;}
    }
    // A few WPS controls paint their text and button chrome through Qt's
    // stylesheet path instead of the skin color resolver.  Keep a small,
    // class-scoped stylesheet so their labels, arrows and hover states retain
    // readable contrast without touching the document canvas.
    auto getCss=(StringGet)GetProcAddress(widgets,"?styleSheet@QApplication@kso_qt@@QBE?AVQString@2@XZ");
    auto setCss=(SetString)GetProcAddress(widgets,"?setStyleSheet@QApplication@kso_qt@@QAEXABVQString@2@@Z");
    if(getCss&&setCss){
        if(!cssBacked){getCss(app,&originalCss);cssBacked=true;}
        if(!restore){
            char bgHex[16]={},workspaceHex[16]={},fgHex[16]={},mutedHex[16]={},hoverHex[16]={},borderHex[16]={},thumbHex[16]={},cardHex[16]={},cardTextHex[16]={},accentHex[16]={};
            wsprintfA(bgHex,"#%02x%02x%02x",background.r,background.g,background.b);
            wsprintfA(workspaceHex,"#%02x%02x%02x",workspace.r,workspace.g,workspace.b);
            wsprintfA(fgHex,"#%02x%02x%02x",foreground.r,foreground.g,foreground.b);
            wsprintfA(mutedHex,"#%02x%02x%02x",muted.r,muted.g,muted.b);
            wsprintfA(hoverHex,"#%02x%02x%02x",hover.r,hover.g,hover.b);
            wsprintfA(borderHex,"#%02x%02x%02x",border.r,border.g,border.b);
            wsprintfA(thumbHex,"#%02x%02x%02x",thumb.r,thumb.g,thumb.b);
            wsprintfA(cardHex,"#%02x%02x%02x",card.r,card.g,card.b);
            wsprintfA(cardTextHex,"#%02x%02x%02x",cardText.r,cardText.g,cardText.b);
            wsprintfA(accentHex,"#%02x%02x%02x",accent.r,accent.g,accent.b);
            char cssText[12288]={};
            wsprintfA(cssText,
                "* {background:%s;color:%s;}"
                "KMainDockArea,KxWpsPromeMainWindow,KPromeHeaderBar,KPromeTabBar,KPromeTab,KPromeTabBarScrollWidget,KPromeSubTabBar,"
                "KxWpsRbTabWidget,KRbTabWidget,KRbCategoryContainer,KRbCategory,KRbQuickAccessToolbar,KRbQuickAccessContainer,KRbTabToolbar,"
                "KRbQuickToolbar,KRbLayoutWidget,KRbHScrollWidget,KRbLogicGroup,KxWpsStatusBar,KStatusBar,KToolBarItemWidget "
                "{color:%s;background-color:%s;border-color:%s;}"
                "KMainDockArea,KxWpsPromeMainWindow,KPromeHeaderBar,KPromeTabBar,KPromeTab,KPromeTabBarScrollWidget,KPromeSubTabBar,"
                "KxWpsRbTabWidget,KRbTabWidget,KRbCategoryContainer,KRbCategory,KRbQuickAccessToolbar,KRbQuickAccessContainer,KRbTabToolbar,"
                "KRbQuickToolbar,KRbLayoutWidget,KRbHScrollWidget,KRbLogicGroup,KxWpsStatusBar,KStatusBar {background:%s;}"
                "KPromeMinButton:hover,KPromeRestoreMaxButton:hover,KPromeCloseButton:hover {color:%s;background:%s;}"
                "KWindowMultiTitleButtonGroup,KWindowMultiTitleButton,KPromeMinimizeButton,KPromeMaximizeButton,KPromeMinButton,KPromeRestoreMaxButton,KPromeCloseButton {color:%s;background:transparent;border:none;}"
                "KPromeMinButton:disabled,KPromeRestoreMaxButton:disabled,KPromeCloseButton:disabled {color:%s;}"
                "KxWpsView,KxWpsDocPane,QTextEdit {selection-background-color:%s;selection-color:%s;}"
                "QComboBox,QLineEdit {color:%s;background-color:%s;selection-background-color:%s;selection-color:%s;}"
                "KxWpsStyleGallery,KStylesGallery,KGalleryView,KListView,QListView {background:%s;color:%s;}"
                "KPopupWidgetEx,KPopupWidget,KPopupMenu,QComboBox QAbstractItemView,QComboBox QListView,KPopupWidgetEx QAbstractItemView,KPopupWidget QAbstractItemView,QAbstractItemView {color:%s;background-color:%s;alternate-background-color:%s;border:1px solid %s;selection-background-color:%s;selection-color:%s;}"
                "KPopupWidgetEx::item,KPopupWidget::item,KPopupMenu::item,QComboBox QAbstractItemView::item,QComboBox QListView::item,QAbstractItemView::item {color:%s;background-color:%s;}"
                "KPopupWidgetEx::item:selected,KPopupWidget::item:selected,KPopupMenu::item:selected,QComboBox QAbstractItemView::item:selected,QComboBox QListView::item:selected,QAbstractItemView::item:selected {color:%s;background-color:%s;}"
                "QScrollBar {background:%s;} QScrollBar::handle {background:%s;}"
                ,bgHex,fgHex,fgHex,bgHex,borderHex,bgHex,fgHex,hoverHex,fgHex,mutedHex,hoverHex,fgHex,fgHex,workspaceHex,hoverHex,fgHex,cardHex,cardTextHex,fgHex,workspaceHex,workspaceHex,borderHex,hoverHex,fgHex,fgHex,workspaceHex,fgHex,hoverHex,workspaceHex,thumbHex);
            QString css=qstr(cssText);
            setCss(app,&css);api.destroy(&css);
        }else if(cssBacked){setCss(app,&originalCss);api.destroy(&originalCss);originalCss.d=nullptr;cssBacked=false;}
    }
    update(app);if(restore)baseBacked=false;return true;
}
static bool refreshWidgets(bool restore,bool repaint=true){
    HMODULE widgets=GetModuleHandleW(L"Qt5WidgetsKso.dll");
    auto all=(AllWidgets)GetProcAddress(widgets,"?allWidgets@QApplication@kso_qt@@SA?AV?$QList@PAVQWidget@kso_qt@@@2@XZ");
    auto update=(NoArg)GetProcAddress(widgets,"?update@QWidget@kso_qt@@QAEXXZ");if(!all||!update)return false;
    QStringList list={nullptr};all(&list);
    bool ok=widgetPalettes(list,restore);
    // Leave QIcon ownership and the ribbon's icon/text layout entirely to
    // WPS.  Replacing those QIcons changes the responsive toolbar from its
    // native icon layout to text fallback and also hides the title buttons.
    // The theme-color hooks and widget palettes below recolor the controls
    // without replacing their source icons.  The optional tint below keeps
    // multiple native-size pixmaps, so it never reduces an icon to a single
    // 24x24 intermediate.
    // Keep WPS's original QIcon pixmaps intact.  Some gallery previews store
    // their labels inside the icon image, so raster-tinting the widget icon
    // makes those labels disappear; the native theme/color hooks already
    // recolor the toolbar strokes without resampling them.
    if(false && ok&&(enabled||restore))ok=tintNativeIcons(list,restore);
    if(ok)ok=scrollbars(list,restore);
    if(repaint)for(int i=list.d->begin;i<list.d->end;i++)if(list.d->array[i])update(list.d->array[i]);releaseWidgets(list);return ok;
}
static bool everApplied=false;
static bool apply(bool restore){
    if(restore&&!enabled&&!everApplied)return true;
    if(!restore)everApplied=true;
    if(!load())return false;HMODULE shell=GetModuleHandleW(L"kshell.dll");
    auto skinGet=(MetaGet)GetProcAddress(shell,"?skin@KApplication@@QAEPAVKAppSkin@@XZ");auto changed=(NoArg)GetProcAddress(shell,"?emitSkinChanged@KAppSkin@@QAEXXZ");
    auto reset=(NoArg)GetProcAddress(shell,"?resetDeviceDependentResources@KAppSkin@@QAEXXZ");if(!skinGet||!changed||!reset)return false;
    // Capture the original widget palettes before native UI update notifications.
    if(!enabled&&!restore&&!refreshWidgets(false))return false;
    enabled=!restore;if(!nativeHooks(restore)){enabled=false;return false;}
    if(!nativeBase(restore))return false;
    void* skin=skinGet(api.instance());reset(skin);changed(skin);void* prome=api.skin(api.instance());if(prome){api.resetResources(prome);api.changed(prome);}
    auto all=(AllWidgets)GetProcAddress(GetModuleHandleW(L"Qt5WidgetsKso.dll"),"?allWidgets@QApplication@kso_qt@@SA?AV?$QList@PAVQWidget@kso_qt@@@2@XZ");
    auto modelGet=(MetaGet)GetProcAddress(shell,"?model@KGalleryView@@QBEPAVKGalleryAbstractModel@@XZ");
    auto modelUpdate=(NoArg)GetProcAddress(shell,"?updateItems@KGalleryAbstractModel@@UAEXXZ");
    // Do not force a second model rebuild after the skin notification.  The
    // style gallery owns its preview labels and this update path clears the
    // delegates on this WPS build; normal paint invalidation is enough.
    if(false && all&&modelGet&&modelUpdate){QStringList list={nullptr};all(&list);for(int i=list.d->begin;i<list.d->end;i++){
        void* w=list.d->array[i];if(!w)continue;void* g=((CastGet)(*(void***)w)[1])(w,"KGalleryView");if(!g)continue;
        void* model=modelGet(g);if(model&&((CastGet)(*(void***)model)[1])(model,"KStylesGalleryModel"))modelUpdate(model);
    }releaseWidgets(list);}
    return refreshWidgets(restore);
}
static bool postRefresh(){return !enabled||refreshWidgets(false,false);}
}
#include "widget_palettes.h"
#include "native_icons.h"
#include "native_icon_tint.h"
#include "scrollbars.h"
#include "theme_hooks.h"
