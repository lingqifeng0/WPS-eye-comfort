namespace blue {
static bool widgetPalettes(const QStringList& list,bool restore){
    HMODULE core=GetModuleHandleW(L"Qt5CoreKso.dll"),gui=GetModuleHandleW(L"Qt5GuiKso.dll"),widgets=GetModuleHandleW(L"Qt5WidgetsKso.dll");
    using PaletteSet=void (__thiscall*)(void*,int,int,const void*);
    using SetValue=void (__thiscall*)(void*,const void*);
    using ObjectSet=bool (__thiscall*)(void*,const char*,const void*);
    using ConstGet=const void* (__thiscall*)(const void*);
    using BrushCtor=void (__thiscall*)(void*,const void*,int);
    auto property=(PropertyGet)GetProcAddress(core,"?property@QObject@kso_qt@@QBE?AVQVariant@2@PBD@Z");
    auto setProperty=(ObjectSet)GetProcAddress(core,"?setProperty@QObject@kso_qt@@QAE_NPBDABVQVariant@2@@Z");
    auto valid=(Visible)GetProcAddress(core,"?isValid@QVariant@kso_qt@@QBE_NXZ");
    auto data=(ConstGet)GetProcAddress(core,"?constData@QVariant@kso_qt@@QBEPBXXZ");
    auto variantCtor=(NoArg)GetProcAddress(core,"??0QVariant@kso_qt@@QAE@XZ");
    auto variantDtor=(NoArg)GetProcAddress(core,"??1QVariant@kso_qt@@QAE@XZ");
    auto paletteGet=(ConstGet)GetProcAddress(widgets,"?palette@QWidget@kso_qt@@QBEABVQPalette@2@XZ");
    auto paletteCtor=(NoArg)GetProcAddress(gui,"??0QPalette@kso_qt@@QAE@XZ");
    auto paletteDtor=(NoArg)GetProcAddress(gui,"??1QPalette@kso_qt@@QAE@XZ");
    auto paletteVariant=(StringGet)GetProcAddress(gui,"??BQPalette@kso_qt@@QBE?AVQVariant@1@XZ");
    auto paletteSet=(PaletteSet)GetProcAddress(gui,"?setColor@QPalette@kso_qt@@QAEXW4ColorGroup@12@W4ColorRole@12@ABVQColor@2@@Z");
    auto widgetPalette=(SetValue)GetProcAddress(widgets,"?setPalette@QWidget@kso_qt@@QAEXABVQPalette@2@@Z");
    auto setMdi=(SetValue)GetProcAddress(widgets,"?setBackground@QMdiArea@kso_qt@@QAEXABVQBrush@2@@Z");
    auto brushCtor=(BrushCtor)GetProcAddress(gui,"??0QBrush@kso_qt@@QAE@ABVQColor@1@W4BrushStyle@Qt@1@@Z");
    auto brushDtor=(NoArg)GetProcAddress(gui,"??1QBrush@kso_qt@@QAE@XZ");
    auto className=(ClassGet)GetProcAddress(core,"?className@QMetaObject@kso_qt@@QBEPBDXZ");
    auto setFixedWidth=(void (__thiscall*)(void*,int))GetProcAddress(widgets,"?setFixedWidth@QWidget@kso_qt@@QAEXH@Z");
    auto setMinimumWidth=(void (__thiscall*)(void*,int))GetProcAddress(widgets,"?setMinimumWidth@QWidget@kso_qt@@QAEXH@Z");
    auto setMaximumWidth=(void (__thiscall*)(void*,int))GetProcAddress(widgets,"?setMaximumWidth@QWidget@kso_qt@@QAEXH@Z");
    auto setFill=(void (__thiscall*)(void*,bool))GetProcAddress(widgets,"?setAutoFillBackground@QWidget@kso_qt@@QAEX_N@Z");
    if(!property||!setProperty||!valid||!data||!variantDtor||!paletteGet||!paletteCtor||!paletteDtor||!paletteVariant||!paletteSet||!widgetPalette||!setMdi||!brushCtor||!brushDtor)return false;
    alignas(8) char pal[64]={},bg[32]={},fg[32]={},accent[32]={},brush[64]={},muted[32]={};
    paletteCtor(pal);makeColor(bg,background);makeColor(fg,foreground);makeColor(accent,blue::accent);brushCtor(brush,bg,1);
    makeColor(muted,blue::muted);
    alignas(8) char selection[32]={};makeColor(selection,hover);
    for(int role=0;role<21;role++){
        if(role==17)continue;
        bool text=role==0||role==6||role==7||role==8||role==13||role==19||role==20;
        const void* normal=role==12?selection:(role==14||role==15?accent:(text?fg:bg));
        // QPalette::ColorGroup is Active=0, Disabled=1, Inactive=2;
        // group 3 is NColorGroups and is ignored by Qt.  The old build only
        // wrote that invalid group, leaving many WPS controls on dark gray
        // defaults.  Populate both normal groups explicitly.
        paletteSet(pal,0,role,normal);paletteSet(pal,2,role,normal);
        paletteSet(pal,1,role,text?muted:bg);
    }
    alignas(8) char areaPal[64]={},fieldPal[64]={},areaBg[32]={},fieldBg[32]={},fieldFg[32]={};
    paletteCtor(areaPal);paletteCtor(fieldPal);makeColor(areaBg,workspace);makeColor(fieldBg,card);makeColor(fieldFg,cardText);
    for(int role=0;role<21;role++){if(role==17)continue;bool text=role==0||role==6||role==7||role==8||role==13||role==19||role==20;
        paletteSet(areaPal,0,role,role==12?selection:(text?fg:areaBg));paletteSet(areaPal,2,role,role==12?selection:(text?fg:areaBg));
        paletteSet(fieldPal,0,role,role==12?selection:(text?fieldFg:fieldBg));paletteSet(fieldPal,2,role,role==12?selection:(text?fieldFg:fieldBg));
        paletteSet(areaPal,1,role,text?muted:areaBg);paletteSet(fieldPal,1,role,text?muted:fieldBg);}
    const char* key="_wps_eye_original_palette";
    for(int i=list.d->begin;i<list.d->end;i++){
        void* w=list.d->array[i];if(!w)continue;
        alignas(8) char saved[64]={};property(w,saved,key);bool has=valid(saved);
        if(restore){if(has)widgetPalette(w,data(saved));alignas(8) char empty[64]={};variantCtor(empty);setProperty(w,key,empty);variantDtor(empty);}
        else{
            if(!has){variantDtor(saved);paletteVariant((void*)paletteGet(w),(QString*)saved);setProperty(w,key,saved);}
            const char* cls=className?className(((MetaGet)(*(void***)w)[0])(w)):"";
            bool area=strcmp(cls,"KxWpsView")==0||strcmp(cls,"KxWpsDocPane")==0||strcmp(cls,"KxWpsMdiArea")==0||strcmp(cls,"KPromeCentralArea")==0||strcmp(cls,"KxWpsMainWindow")==0||strcmp(cls,"KPromeDockMainWindow")==0||strcmp(cls,"KPromeWpsPage")==0||strcmp(cls,"KxWpsSubWindow")==0||strcmp(cls,"KxWpsSplitter")==0;
            // The ribbon style picker is a custom gallery/list view.  Its
            // item names are painted from the widget palette rather than the
            // theme-color query, so the global dark palette otherwise makes
            // white text sit on the light preview card and the names vanish.
            bool field=strcmp(cls,"KGalleryComboBoxLineEdit")==0||
                strcmp(cls,"KxWpsGalleryFontComboBox")==0||
                strcmp(cls,"KxFontSizePreviewComboBox")==0;
            bool galleryWidget=strstr(cls,"Gallery")!=nullptr||strstr(cls,"gallery")!=nullptr||
                strstr(cls,"ListView")!=nullptr||strstr(cls,"KListView")!=nullptr;
            // Gallery preview delegates use their own item palette.  Keep the
            // saved WPS palette on that subtree; forcing the frame palette
            // here paints the preview card but drops the item labels.
            if(!galleryWidget)widgetPalette(w,area?areaPal:field?fieldPal:pal);
            if(strstr(cls,"KPromeMinButton")||strstr(cls,"KPromeRestoreMaxButton")||strstr(cls,"KPromeCloseButton")){
                if(!restore&&setFixedWidth)setFixedWidth(w,70);
                if(restore&&setMinimumWidth&&setMaximumWidth){setMinimumWidth(w,0);setMaximumWidth(w,16777215);}
            }
        }
        variantDtor(saved);
        if(className&&setFill){
            const char* cls=className(((MetaGet)(*(void***)w)[0])(w));
            if(strcmp(cls,"QWidget")==0||strcmp(cls,"QMainWindow")==0||strcmp(cls,"KPromeHeaderBar")==0||strcmp(cls,"KPromeTabBar")==0||strcmp(cls,"KPromeTabBarScrollWidget")==0||strcmp(cls,"KPromeTab")==0||strcmp(cls,"KPromeSubTabBar")==0||strcmp(cls,"KxWpsStatusBar")==0||strcmp(cls,"KxWpsPromeMainWindow")==0||strcmp(cls,"KxWpsRbTabWidget")==0||strcmp(cls,"KRbTabWidget")==0||strcmp(cls,"KRbCategoryContainer")==0||strcmp(cls,"KRbCategory")==0||strcmp(cls,"KRbToolbarContainer")==0||strcmp(cls,"KRbQuickAccessToolbar")==0||strcmp(cls,"KRbQuickAccessContainer")==0||strcmp(cls,"KRbTabToolbar")==0||strcmp(cls,"KRbQuickToolbar")==0||strcmp(cls,"KxWpsDocPane")==0||strcmp(cls,"KxWpsView")==0||strcmp(cls,"KxWpsMdiArea")==0||strcmp(cls,"KPromeCentralArea")==0||strcmp(cls,"KxWpsMainWindow")==0||strcmp(cls,"KPromeDockMainWindow")==0||strcmp(cls,"KPromeWpsPage")==0||strcmp(cls,"KxWpsSubWindow")==0||strcmp(cls,"KxWpsSplitter")==0){
                const char* fillKey="_wps_eye_original_autofill";
                property(w,saved,fillKey);has=valid(saved);
                auto toBool=(Visible)GetProcAddress(core,"?toBool@QVariant@kso_qt@@QBE_NXZ");
                if(restore){if(has&&toBool)setFill(w,toBool(saved));alignas(8) char empty[64]={};variantCtor(empty);setProperty(w,fillKey,empty);variantDtor(empty);}
                else{if(!has){variantDtor(saved);property(w,saved,"autoFillBackground");setProperty(w,fillKey,saved);}setFill(w,true);}
                variantDtor(saved);
            }
        }
        void* mdi=((CastGet)(*(void***)w)[1])(w,"QMdiArea");
        if(mdi){
            const char* mdiKey="_wps_eye_original_mdi_background";
            property(w,saved,mdiKey);has=valid(saved);
            if(restore){if(has)setMdi(mdi,data(saved));alignas(8) char empty[64]={};variantCtor(empty);setProperty(w,mdiKey,empty);variantDtor(empty);}
            else{if(!has){variantDtor(saved);property(w,saved,"background");setProperty(w,mdiKey,saved);}setMdi(mdi,brush);}
            variantDtor(saved);
        }
    }
    brushDtor(brush);paletteDtor(areaPal);paletteDtor(fieldPal);paletteDtor(pal);return true;
}
}
