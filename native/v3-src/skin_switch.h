// WPS 12.8.2.19823 x86: native .kuip/.rcc skin switching (no hooks, no registry/kcc writes).
// Works across WPS Writer (wps.exe), Presentation (wpp.exe), and Spreadsheets (et.exe).
namespace skin_switch {

using AppSkinFn = void* (__thiscall*)(void*);
using SwitchTemp2 = void (__thiscall*)(void*, const QString*, const QString*);
using GetStrFn = QString* (__thiscall*)(void*, QString*);
using IsDarkFn = bool (__thiscall*)(void*);
using AddSearchPathFn = void (__thiscall*)(void*, const QString*);

static bool apply(Request* req) {
    HMODULE promeDll = GetModuleHandleW(L"kprometheus.dll");
    HMODULE shellDll = GetModuleHandleW(L"kshell.dll");
    HMODULE liteDll  = GetModuleHandleW(L"ksolite.dll");
    void* app = api.instance();
    if (!app || !promeDll || !shellDll) {
        req->status = -81;
        return false;
    }

    void* promeSkin = api.skin ? api.skin(app) : nullptr;
    auto getAppSkin = (AppSkinFn)GetProcAddress(shellDll, "?skin@KApplication@@QAEPAVKAppSkin@@XZ");
    void* appSkin = getAppSkin ? getAppSkin(app) : nullptr;
    if (!promeSkin || !appSkin) {
        req->status = -82;
        return false;
    }

    // Optional switch command in record 0: kind="switch", name=skinName, value=searchDir (may be empty)
    bool doSwitch = false;
    const char* targetName = nullptr;
    const char* searchDir = "";
    for (DWORD i = 0; i < req->count; i++) {
        if (!strcmp(req->records[i].kind, "switch")) {
            doSwitch = true;
            targetName = req->records[i].name;
            searchDir = req->records[i].value;
            break;
        }
    }

    if (doSwitch) {
        if (!targetName || !targetName[0]) {
            req->status = -83;
            return false;
        }
        QString qName = qstr(targetName);
        QString qDir  = qstr(searchDir);

        void* promeInner = *(void**)((char*)promeSkin + 0x0c);
        void* appInner   = *(void**)((char*)appSkin + 0x20);
        auto removeGlobalSync = liteDll ? (NoArg)GetProcAddress(liteDll, "?removeGlobalSkinChangeSupport@KSkin@@QAEXXZ") : nullptr;
        auto addGlobalSync    = liteDll ? (NoArg)GetProcAddress(liteDll, "?addGlobalSkinChangeSupport@KSkin@@QAEXXZ") : nullptr;

        // Disable WPS's cross-process skin sync broadcast before switching so switching WPP/ET
        // never triggers an IPC reload of "2018white" in already-dark WPS Writer/PPT processes!
        if (removeGlobalSync) {
            if (promeInner) removeGlobalSync(promeInner);
            if (appInner)   removeGlobalSync(appInner);
        }

        // Register custom skin search path in KPromeSkin's inner KSkin when switching to a custom dir.
        if (searchDir[0] && liteDll) {
            auto addSearch = (AddSearchPathFn)GetProcAddress(liteDll, "?addSearchPath@KLiteSkin@@IAEXABVQString@kso_qt@@@Z");
            if (addSearch && promeInner) {
                addSearch(promeInner, &qDir);
            }
        }

        // Switch KPromeSkin with bSave=false, bAll=false, bForce=true (zero kcc/registry mutation).
        if (api.switchSkin) {
            api.switchSkin(promeSkin, &qName, false, false, true);
        }

        // Switch KAppSkin temporarily (ribbon, toolbar, statusbar, popups) with zero persistence.
        auto swTemp = (SwitchTemp2)GetProcAddress(shellDll, "?switchTempSkin@KAppSkin@@QAEXABVQString@kso_qt@@0@Z");
        if (swTemp) {
            swTemp(appSkin, &qName, &qDir);
        }

        auto updateUi = (NoArg)GetProcAddress(shellDll, "?updateUiColor@KApplication@@QAEXXZ");
        if (updateUi) {
            updateUi(app);
        }

        // When restoring back to built-in skin (searchDir empty), re-enable WPS's native sync cleanly.
        if (!searchDir[0] && addGlobalSync) {
            if (promeInner) addGlobalSync(promeInner);
            if (appInner)   addGlobalSync(appInner);
        }

        api.destroy(&qDir);
        api.destroy(&qName);
    }

    // Read back live state into records where kind=="query" or kind=="switch"
    char promeCur[96] = {0};
    char appCur[96] = {0};
    int promeIsDark = -1;
    int appIsDark = -1;

    auto getPromeName = (GetStrFn)GetProcAddress(promeDll, "?getSkinName@KPromeSkin@@QBE?AVQString@kso_qt@@XZ");
    auto isPromeDark  = (IsDarkFn)GetProcAddress(promeDll, "?isDarkSkin@KPromeSkin@@QBE_NXZ");
    if (getPromeName) {
        QString out = {nullptr};
        getPromeName(promeSkin, &out);
        if (out.d) {
            textOf(out, promeCur, sizeof(promeCur));
            api.destroy(&out);
        }
    }
    if (isPromeDark) {
        promeIsDark = isPromeDark(promeSkin) ? 1 : 0;
    }

    auto getAppCur = (GetStrFn)GetProcAddress(shellDll, "?getCurSkinName@KAppSkin@@QBE?AVQString@kso_qt@@XZ");
    auto isAppDark = (IsDarkFn)GetProcAddress(shellDll, "?isDarkSkin@KAppSkin@@QAE_NXZ");
    if (getAppCur) {
        QString out = {nullptr};
        getAppCur(appSkin, &out);
        if (out.d) {
            textOf(out, appCur, sizeof(appCur));
            api.destroy(&out);
        }
    }
    if (isAppDark) {
        appIsDark = isAppDark(appSkin) ? 1 : 0;
    }

    // Populate output records (up to req->count)
    for (DWORD i = 0; i < req->count; i++) {
        auto& r = req->records[i];
        if (!strcmp(r.kind, "switch") || !strcmp(r.kind, "query")) {
            if (!strcmp(r.style, "prome")) {
                strncpy(r.name, promeCur, sizeof(r.name) - 1);
                r.name[sizeof(r.name) - 1] = 0;
                wsprintfA(r.value, "%d", promeIsDark);
            } else if (!strcmp(r.style, "app")) {
                strncpy(r.name, appCur, sizeof(r.name) - 1);
                r.name[sizeof(r.name) - 1] = 0;
                wsprintfA(r.value, "%d", appIsDark);
            } else {
                // Default summary record: style="state", name=promeCur, value="appCur|promeDark|appDark"
                strncpy(r.name, promeCur, sizeof(r.name) - 1);
                r.name[sizeof(r.name) - 1] = 0;
                wsprintfA(r.value, "%s|%d|%d", appCur, promeIsDark, appIsDark);
            }
        }
    }

    if (doSwitch && targetName) {
        if (strcmp(promeCur, targetName) != 0 || strcmp(appCur, targetName) != 0) {
            req->status = -84;
            return false;
        }
    }
    return true;
}

} // namespace skin_switch
