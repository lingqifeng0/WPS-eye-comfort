// WPS 12.8.2.19823 x86: Native zero-flicker canvas eye comfort (Mode 1 Excel dark sheet,
// Mode 2 Word/Excel/PPT QPainter::CompositionMode_Multiply soft-dim) + independent
// document image night-brightness via kso.dll VariantImage::render + QImageEffects.
// Zero overlays, zero screen capture, zero yellow border, zero document mutation.
namespace canvas_eye {

struct QRectRaw { int x1, y1, x2, y2; };

// 0 = off, 1 = dark (Excel native dark sheet; PPT auto soft-dim; Word uses mode=12 BlueScreen),
// 2 = soft_dim (Word + Excel + PPT native QPainter::CompositionMode_Multiply).
static volatile LONG g_canvasMode = 0;
static volatile LONG g_dimR = 176;
static volatile LONG g_dimG = 172;
static volatile LONG g_dimB = 162;
static volatile LONG g_darkBg = 0xFF262C34;
static volatile LONG g_darkFg = 0xFFE0E4EA;
static volatile LONG g_imgEnabled = 0;
static volatile LONG g_imgBrightness = 70; // 25..100 (%)

static volatile LONG g_paintCalls = 0;
static volatile LONG g_paintCanvasOk = 0;
static volatile LONG g_paintHits = 0;
static volatile LONG g_etCacheHits = 0;
static volatile LONG g_etFillAll = 0;
static volatile LONG g_etDarkHits = 0;
static volatile LONG g_picLayerCalls = 0;
static volatile LONG g_imgCalls = 0;
static volatile LONG g_natBmpCalls = 0;
static volatile LONG g_imgHits = 0;
static volatile LONG g_viewsFound = 0;
static char g_lastViewClass[64] = {};

static bool g_loaded = false;
static bool g_hooksInstalled = false;
static bool g_hooksEnabled = false;
static bool g_isWps = false;
static bool g_isEt = false;
static bool g_isWpp = false;

static __declspec(thread) int tls_inMultiply = 0;
static __declspec(thread) int tls_inRemap = 0;
static __declspec(thread) int tls_etPaintDepth = 0;
static __declspec(thread) int tls_inPictureLayer = 0;
static __declspec(thread) int tls_inImgDim = 0;
static __declspec(thread) void* tls_etCachePainter = nullptr;
static __declspec(thread) void* tls_curMultiplyView = nullptr;
static __declspec(thread) QRectRaw tls_curDirtyRect = {0, 0, -1, -1};
static __declspec(thread) bool tls_multipliedThisPaint = false;

// Qt5 / KSO function pointers (all official exported symbols from COMMON_DLLS)
using ClassNameFn = const char* (__thiscall*)(const void*);
using WidgetWindowFn = void* (__thiscall*)(const void*);
using WidgetBoolFn = bool (__thiscall*)(const void*);
using WidgetWinIdFn = unsigned int (__thiscall*)(const void*);
using PaintEventRectFn = const QRectRaw* (__thiscall*)(const void*);
using QColorCtor4Fn = void* (__thiscall*)(void*, int, int, int, int);
using QColorRgbaFn = unsigned int (__thiscall*)(const void*);
using QPainterCtorDevFn = void* (__thiscall*)(void*, void*);
using QPainterDtorFn = void (__thiscall*)(void*);
using QPainterIsActiveFn = bool (__thiscall*)(const void*);
using QPainterDeviceFn = void* (__thiscall*)(const void*);
using QPainterEndFn = bool (__thiscall*)(void*);
using QPainterSaveRestoreFn = void (__thiscall*)(void*);
using QPainterSetCompFn = void (__thiscall*)(void*, int);
using QPainterSetClipBoolFn = void (__thiscall*)(void*, bool);
using QWidgetDimFn = int (__thiscall*)(const void*);
using QPaintDevDimFn = int (__thiscall*)(const void*);
using QPaintDevDprFn = double (__thiscall*)(const void*);
using QImageBitsFn = unsigned char* (__thiscall*)(void*);
using QImageIntFn = int (__thiscall*)(const void*);
using QPixmapHandleFn = void* (__thiscall*)(const void*);
using QRasterPpBufferFn = void* (__thiscall*)(void*);
using KxViewAddInvalidFn = void (__thiscall*)(void*, const QRectRaw*);
using KxViewVoidFn = void (__thiscall*)(void*);
using NativeBitmapRenderFn = void (__thiscall*)(const void*, void*, const void*, const void*, const void*);
using PictureLayerRenderFn = void (__thiscall*)(void*);
using PeDrawImage5Fn = void (__thiscall*)(void*, const void*, const void*, const void*, const void*, int);
using PeDrawImage4Fn = void (__thiscall*)(void*, const void*, const void*, const void*, int);
using QPainterFillRectQRectColorFn = void (__thiscall*)(void*, const void*, const void*);
using QPainterFillRectQRectFColorFn = void (__thiscall*)(void*, const void*, const void*);
using QPainterFillRectInt4ColorFn = void (__thiscall*)(void*, int, int, int, int, const void*);
using QPainterFillRectQRectBrushFn = void (__thiscall*)(void*, const void*, const void*);
using QPainterFillRectQRectFBrushFn = void (__thiscall*)(void*, const void*, const void*);
using QPainterFillRectInt4BrushFn = void (__thiscall*)(void*, int, int, int, int, const void*);
using QPainterSetPenColorFn = void (__thiscall*)(void*, const void*);
using QPainterSetPenPenFn = void (__thiscall*)(void*, const void*);
using QBrushCopyFn = void* (__thiscall*)(void*, const void*);
using QBrushDtorFn = void (__thiscall*)(void*);
using QBrushStyleFn = int (__thiscall*)(const void*);
using QBrushColorFn = const void* (__thiscall*)(const void*);
using QBrushSetColorFn = void (__thiscall*)(void*, const void*);
using QPenCopyFn = void* (__thiscall*)(void*, const void*);
using QPenDtorFn = void (__thiscall*)(void*);
using QPenStyleFn = int (__thiscall*)(const void*);
using QPenColorFn = void* (__thiscall*)(const void*, void*);
using QPenSetColorFn = void (__thiscall*)(void*, const void*);
using QFxDefaultCtorFn = void* (__thiscall*)(void*);
using QFxCopyCtorFn = void* (__thiscall*)(void*, const void*);
using QFxDtorFn = void (__thiscall*)(void*);
using QFxGetDoubleFn = double (__thiscall*)(const void*);
using QFxSetDoubleFn = void (__thiscall*)(void*, double);
using KxViewPaintFn = void (__thiscall*)(void*, void*);
using KxViewGetCacheDCFn = void* (__stdcall*)(void*);
using KxViewRelCacheDCFn = int (__stdcall*)(void*, void*, bool);
using KxViewRelCacheDCNoRepaintFn = int (__stdcall*)(void*, void*);
using VariantImageRenderFn = void (__thiscall*)(
    const void*, void*, const void*, const void*, const void*, bool, bool, bool, bool);
using PainterExtFillColorFn = void (__thiscall*)(void*, const void*);
using PainterExtSetPenColorFn = void (__thiscall*)(void*, const void*);
using PainterExtSetPenPenFn = void (__thiscall*)(void*, const void*);

static ClassNameFn qMetaClassName = nullptr;
static WidgetWindowFn qWidgetWindow = nullptr;
static WidgetBoolFn qWidgetIsFullScreen = nullptr;
static WidgetWinIdFn qWidgetEffectiveWinId = nullptr;
static PaintEventRectFn qPaintEventRect = nullptr;
static QColorCtor4Fn qColorCtor4 = nullptr;
static QColorRgbaFn qColorRgba = nullptr;
static QPainterCtorDevFn qPainterCtorDev = nullptr;
static QPainterDtorFn qPainterDtor = nullptr;
static QPainterIsActiveFn qPainterIsActive = nullptr;
static QPainterDeviceFn qPainterDevice = nullptr;
static QPainterSaveRestoreFn qPainterSave = nullptr;
static QPainterSaveRestoreFn qPainterRestore = nullptr;
static QPainterSaveRestoreFn qPainterResetTransform = nullptr;
static QPainterSetCompFn qPainterSetComp = nullptr;
static QPainterSetClipBoolFn qPainterSetClipping = nullptr;
static QWidgetDimFn qWidgetWidth = nullptr;
static QWidgetDimFn qWidgetHeight = nullptr;
static QPaintDevDimFn qPaintDevWidth = nullptr;
static QPaintDevDimFn qPaintDevHeight = nullptr;
static QPaintDevDprFn qImageDpr = nullptr;
static QPaintDevDprFn qPixmapDpr = nullptr;
static QImageBitsFn qImageBits = nullptr;
static QImageIntFn qImageBytesPerLine = nullptr;
static QImageIntFn qImageFormat = nullptr;
static QPixmapHandleFn qPixmapHandle = nullptr;
static QRasterPpBufferFn qRasterPpBuffer = nullptr;
static void* vtblQRasterPlatformPixmap = nullptr;
static QBrushCopyFn qBrushCopy = nullptr;
static QBrushDtorFn qBrushDtor = nullptr;
static QBrushStyleFn qBrushStyle = nullptr;
static QBrushColorFn qBrushColor = nullptr;
static QBrushSetColorFn qBrushSetColor = nullptr;
static QPenCopyFn qPenCopy = nullptr;
static QPenDtorFn qPenDtor = nullptr;
static QPenStyleFn qPenStyle = nullptr;
static QPenColorFn qPenColor = nullptr;
static QPenSetColorFn qPenSetColor = nullptr;
static QFxDefaultCtorFn qFxDefaultCtor = nullptr;
static QFxCopyCtorFn qFxCopyCtor = nullptr;
static QFxDtorFn qFxDtor = nullptr;
static QFxGetDoubleFn qFxBrightness = nullptr;
static QFxGetDoubleFn qFxContrast = nullptr;
static QFxSetDoubleFn qFxSetBrightness = nullptr;
static QFxSetDoubleFn qFxSetContrast = nullptr;

// MinHook target addresses & originals
static void* tgtKxViewPaint = nullptr;
static void* origKxViewPaint = nullptr;
static void* tgtKxEtViewPaint = nullptr;
static void* origKxEtViewPaint = nullptr;
static void* tgtQPainterEnd = nullptr;
static void* origQPainterEnd = nullptr;
static void* tgtPictureLayerRender = nullptr;
static void* origPictureLayerRender = nullptr;
static void* tgtVariantRender = nullptr;
static void* origVariantRender = nullptr;
static void* tgtNativeBitmapRender = nullptr;
static void* origNativeBitmapRender = nullptr;
static void* tgtPeDrawImage5 = nullptr;
static void* origPeDrawImage5 = nullptr;
static void* tgtPeDrawImage4 = nullptr;
static void* origPeDrawImage4 = nullptr;

// Excel-only Mode 1 hooks
static void* tgtGetCacheDC = nullptr;
static void* origGetCacheDC = nullptr;
static void* tgtRelCacheDC = nullptr;
static void* origRelCacheDC = nullptr;
static void* tgtRelCacheDCNR = nullptr;
static void* origRelCacheDCNR = nullptr;
static void* tgtFillQRectColor = nullptr;
static void* origFillQRectColor = nullptr;
static void* tgtFillQRectFColor = nullptr;
static void* origFillQRectFColor = nullptr;
static void* tgtFillInt4Color = nullptr;
static void* origFillInt4Color = nullptr;
static void* tgtFillQRectBrush = nullptr;
static void* origFillQRectBrush = nullptr;
static void* tgtFillQRectFBrush = nullptr;
static void* origFillQRectFBrush = nullptr;
static void* tgtFillInt4Brush = nullptr;
static void* origFillInt4Brush = nullptr;
static void* tgtSetPenColor = nullptr;
static void* origSetPenColor = nullptr;
static void* tgtSetPenPen = nullptr;
static void* origSetPenPen = nullptr;
static void* tgtPeFillColor = nullptr;
static void* origPeFillColor = nullptr;
static void* tgtPeSetPenColor = nullptr;
static void* origPeSetPenColor = nullptr;
static void* tgtPeSetPenPen = nullptr;
static void* origPeSetPenPen = nullptr;

static bool ciContains(const char* hay, const char* needle) {
    if (!hay || !needle || !*needle) return false;
    size_t nlen = strlen(needle);
    for (const char* p = hay; *p; p++) {
        size_t i = 0;
        while (i < nlen && p[i]) {
            char a = p[i];
            char b = needle[i];
            if (a >= 'A' && a <= 'Z') a = (char)(a + 32);
            if (b >= 'A' && b <= 'Z') b = (char)(b + 32);
            if (a != b) break;
            i++;
        }
        if (i == nlen) return true;
    }
    return false;
}

static const char* getWidgetClassName(const void* obj) {
    if (!obj || !qMetaClassName) return "";
    void* mo = ((MetaGet)(*(void***)obj)[0])((void*)obj);
    if (!mo) return "";
    const char* name = qMetaClassName(mo);
    return name ? name : "";
}

static bool isFullscreenSlideshowWidget(const void* widget) {
    if (!g_isWpp || !widget) return false;
    if (qWidgetWindow && qWidgetIsFullScreen) {
        void* top = qWidgetWindow(widget);
        if (top) {
            if (qWidgetIsFullScreen(top)) return true;
            if (qWidgetEffectiveWinId) {
                HWND hw = (HWND)(uintptr_t)qWidgetEffectiveWinId(top);
                if (hw) {
                    char wcls[64] = {};
                    if (GetClassNameA(hw, wcls, sizeof(wcls)) > 0 && ciContains(wcls, "screenClass")) {
                        return true;
                    }
                }
            }
        }
    }
    return false;
}

static BOOL CALLBACK enumWppSlideshowCb(HWND hw, LPARAM lp) {
    if (!IsWindowVisible(hw)) return TRUE;
    char wcls[64] = {};
    if (GetClassNameA(hw, wcls, sizeof(wcls)) > 0) {
        if (ciContains(wcls, "screenClass") || ciContains(wcls, "slideshow") || ciContains(wcls, "play")) {
            *(bool*)lp = true;
            return FALSE;
        }
    }
    // Also detect borderless fullscreen slideshow windows on any monitor (single or dual-monitor Presenter View).
    LONG style = GetWindowLongA(hw, GWL_STYLE);
    if ((style & WS_CAPTION) == 0) {
        RECT r = {};
        if (GetWindowRect(hw, &r)) {
            HMONITOR hm = MonitorFromWindow(hw, MONITOR_DEFAULTTONEAREST);
            MONITORINFO mi = { sizeof(mi) };
            if (hm && GetMonitorInfoA(hm, &mi)) {
                int rw = r.right - r.left, rh = r.bottom - r.top;
                int mw = mi.rcMonitor.right - mi.rcMonitor.left;
                int mh = mi.rcMonitor.bottom - mi.rcMonitor.top;
                if (rw >= mw - 2 && rh >= mh - 2 && mw >= 640 && mh >= 480) {
                    *(bool*)lp = true;
                    return FALSE;
                }
            }
        }
    }
    return TRUE;
}

static bool isSlideshowForeground() {
    if (!g_isWpp) return false;
    bool found = false;
    EnumThreadWindows(GetCurrentThreadId(), enumWppSlideshowCb, (LPARAM)&found);
    return found;
}

static bool isDocCanvasView(const char* cls) {
    if (!cls || !cls[0]) return false;
    // Exclude thumbnails, previews, slide shows, outlines, and mini popups.
    if (ciContains(cls, "thumb") || ciContains(cls, "slideshow") || ciContains(cls, "play") ||
        ciContains(cls, "screen") || ciContains(cls, "preview") || ciContains(cls, "outline") ||
        ciContains(cls, "mini")) {
        return false;
    }
    return true;
}

static inline int clamp255(int v) {
    return v < 0 ? 0 : (v > 255 ? 255 : v);
}

static bool remapEtFillRgba(unsigned int rgba, int& outR, int& outG, int& outB, int& outA) {
    int a = (rgba >> 24) & 0xFF;
    int r = (rgba >> 16) & 0xFF;
    int g = (rgba >> 8) & 0xFF;
    int b = rgba & 0xFF;
    if (a < 200) return false; // keep semi-transparent selection highlights intact

    int minC = r < g ? (r < b ? r : b) : (g < b ? g : b);
    int maxC = r > g ? (r > b ? r : b) : (g > b ? g : b);
    int sat = maxC - minC;

    DWORD bg = (DWORD)g_darkBg;
    int bgR = (bg >> 16) & 0xFF;
    int bgG = (bg >> 8) & 0xFF;
    int bgB = bg & 0xFF;

    // Pure / near-white spreadsheet grid canvas
    if (minC >= 244 && sat <= 14) {
        outR = bgR; outG = bgG; outB = bgB; outA = a;
        return true;
    }
    // Light neutral row/column headers & sheet margins
    if (minC >= 218 && sat <= 14) {
        outR = clamp255(bgR + 12);
        outG = clamp255(bgG + 14);
        outB = clamp255(bgB + 18);
        outA = a;
        return true;
    }
    // Very bright pastel fills: keep original hue, reduce harsh luminance for dark sheet
    if (minC >= 205 && sat > 14) {
        outR = clamp255((r * 46) / 100);
        outG = clamp255((g * 46) / 100);
        outB = clamp255((b * 46) / 100);
        outA = a;
        return true;
    }
    return false;
}

static bool remapEtPenRgba(unsigned int rgba, int& outR, int& outG, int& outB, int& outA) {
    int a = (rgba >> 24) & 0xFF;
    int r = (rgba >> 16) & 0xFF;
    int g = (rgba >> 8) & 0xFF;
    int b = rgba & 0xFF;
    if (a < 160) return false;

    int minC = r < g ? (r < b ? r : b) : (g < b ? g : b);
    int maxC = r > g ? (r > b ? r : b) : (g > b ? g : b);
    int sat = maxC - minC;

    DWORD bg = (DWORD)g_darkBg;
    int bgR = (bg >> 16) & 0xFF;
    int bgG = (bg >> 8) & 0xFF;
    int bgB = bg & 0xFF;

    DWORD fg = (DWORD)g_darkFg;
    int fgR = (fg >> 16) & 0xFF;
    int fgG = (fg >> 8) & 0xFF;
    int fgB = fg & 0xFF;

    // Default black / near-black cell text & thin dark cell borders
    if (maxC <= 48 && sat <= 18) {
        outR = fgR; outG = fgG; outB = fgB; outA = a;
        return true;
    }
    // Medium-dark gray text (row/column labels, secondary text)
    if (maxC > 48 && maxC <= 125 && sat <= 16) {
        outR = clamp255((bgR * 28 + fgR * 72) / 100);
        outG = clamp255((bgG * 28 + fgG * 72) / 100);
        outB = clamp255((bgB * 28 + fgB * 72) / 100);
        outA = a;
        return true;
    }
    // Light gray gridlines (#D4D4D4 / #E0E0E0)
    if (minC >= 180 && maxC <= 242 && sat <= 16) {
        outR = clamp255(bgR + 24);
        outG = clamp255(bgG + 28);
        outB = clamp255(bgB + 34);
        outA = a;
        return true;
    }
    // Dark saturated text (e.g. dark blue links / dark red) -> lift brightness for legibility
    if (sat > 18 && maxC < 150) {
        int lift = 95;
        outR = clamp255(r + lift);
        outG = clamp255(g + lift);
        outB = clamp255(b + lift);
        outA = a;
        return true;
    }
    return false;
}

static inline bool inEtDarkPainter(void* qp) {
    if (!g_isEt || g_canvasMode != 1 || tls_inRemap > 0 || tls_inMultiply > 0) return false;
    if (tls_etPaintDepth > 0) return true;
    if (tls_etCachePainter != nullptr && qp == (void*)((char*)tls_etCachePainter + 4)) return true;
    return false;
}

static void multiplyQImageRectInPlace(void* qimg, int px1, int py1, int px2, int py2) {
    if (!qimg || !qImageBits || !qImageBytesPerLine || !qImageFormat) return;
    int fmt = qImageFormat(qimg);
    // 4 = Format_RGB32, 5 = Format_ARGB32, 6 = Format_ARGB32_Premultiplied
    if (fmt != 4 && fmt != 5 && fmt != 6) return;
    int bpl = qImageBytesPerLine(qimg);
    unsigned char* base = qImageBits(qimg);
    if (!base || bpl <= 0) return;
    unsigned int mR = (unsigned int)g_dimR;
    unsigned int mG = (unsigned int)g_dimG;
    unsigned int mB = (unsigned int)g_dimB;
    bool checkAlpha = (fmt == 5 || fmt == 6);
    for (int y = py1; y <= py2; y++) {
        unsigned char* row = base + (size_t)y * (size_t)bpl + (size_t)px1 * 4u;
        int count = px2 - px1 + 1;
        for (int i = 0; i < count; i++, row += 4) {
            if (checkAlpha && row[3] == 0) continue; // Keep transparent overlay/blur pixels untouched!
            row[0] = (unsigned char)((row[0] * mB + 127u) / 255u);
            row[1] = (unsigned char)((row[1] * mG + 127u) / 255u);
            row[2] = (unsigned char)((row[2] * mR + 127u) / 255u);
        }
    }
    InterlockedIncrement(&g_paintHits);
}

static bool __fastcall hookQPainterEnd(void* qp, void*) {
    void* targetImg = nullptr;
    int px1 = 0, py1 = 0, px2 = -1, py2 = -1;

    if (g_isWpp && tls_curMultiplyView != nullptr && tls_inMultiply == 0 && qp &&
        qPainterDevice && qPainterIsActive && qPaintDevWidth && qPaintDevHeight &&
        qWidgetWidth && qWidgetHeight) {
        bool wantMultiply = (g_canvasMode == 2) || (g_canvasMode == 1);
        if (wantMultiply && qPainterIsActive(qp)) {
            void* dev = qPainterDevice(qp);
            void* widgetDev = (void*)((char*)tls_curMultiplyView + 8);
            if (dev && dev != widgetDev) {
                using DevTypeFn = int (__thiscall*)(void*);
                int dt = ((DevTypeFn)(*(void***)dev)[1])(dev);
                // QInternal::Pixmap == 2 or QInternal::Image == 3 (offscreen cache with (0,0) origin!)
                if (dt == 2 || dt == 3) {
                    int vw = qWidgetWidth(tls_curMultiplyView);
                    int vh = qWidgetHeight(tls_curMultiplyView);
                    int dw = qPaintDevWidth(dev);
                    int dh = qPaintDevHeight(dev);
                    if (vw > 16 && vh > 16 && dw >= vw - 4 && dh >= vh - 4 &&
                        tls_curDirtyRect.x2 >= tls_curDirtyRect.x1 &&
                        tls_curDirtyRect.y2 >= tls_curDirtyRect.y1) {
                        void* backingImg = nullptr;
                        if (dt == 3) {
                            backingImg = dev;
                        } else if (dt == 2 && qPixmapHandle) {
                            void* pp = qPixmapHandle(dev);
                            if (pp) {
                                // QPlatformPixmap::buffer() is virtual vtable slot [21] (0x54)
                                using VBufferFn = void* (__thiscall*)(void*);
                                backingImg = ((VBufferFn)(*(void***)pp)[21])(pp);
                            }
                        }
                        if (backingImg) {
                            int iw = qPaintDevWidth(backingImg);
                            int ih = qPaintDevHeight(backingImg);
                            if (iw > 0 && ih > 0) {
                                double sx = (double)iw / (double)vw;
                                double sy = (double)ih / (double)vh;
                                px1 = (int)(tls_curDirtyRect.x1 * sx);
                                py1 = (int)(tls_curDirtyRect.y1 * sy);
                                px2 = (int)((tls_curDirtyRect.x2 + 1) * sx) - 1;
                                py2 = (int)((tls_curDirtyRect.y2 + 1) * sy) - 1;
                                if (px1 < 0) px1 = 0;
                                if (py1 < 0) py1 = 0;
                                if (px2 >= iw) px2 = iw - 1;
                                if (py2 >= ih) py2 = ih - 1;
                                if (px2 >= px1 && py2 >= py1) {
                                    targetImg = backingImg;
                                }
                            }
                        } else if (qPainterSave && qPainterRestore && qPainterResetTransform && qPainterSetComp) {
                            // Non-raster QPlatformPixmap fallback: CompositionMode_SourceAtop (9)
                            // Preserves alpha==0 transparent overlay pixels 100% untouched!
                            int dR = (int)g_dimR, dG = (int)g_dimG, dB = (int)g_dimB;
                            int minC = dR < dG ? (dR < dB ? dR : dB) : (dG < dB ? dG : dB);
                            int alpha = 255 - minC;
                            if (alpha > 0) {
                                int sR = clamp255(((dR - minC) * 255 + alpha / 2) / alpha);
                                int sG = clamp255(((dG - minC) * 255 + alpha / 2) / alpha);
                                int sB = clamp255(((dB - minC) * 255 + alpha / 2) / alpha);
                                double dpr = (qPixmapDpr && dt == 2) ? qPixmapDpr(dev) : 1.0;
                                QRectRaw r = tls_curDirtyRect;
                                if (dpr <= 1.01 && dw > vw + 4 && dh > vh + 4) {
                                    double sx = (double)dw / (double)vw;
                                    double sy = (double)dh / (double)vh;
                                    r.x1 = (int)(r.x1 * sx);
                                    r.y1 = (int)(r.y1 * sy);
                                    r.x2 = (int)((r.x2 + 1) * sx);
                                    r.y2 = (int)((r.y2 + 1) * sy);
                                }
                                tls_inMultiply++;
                                qPainterSave(qp);
                                if (qPainterSetClipping) qPainterSetClipping(qp, false);
                                qPainterResetTransform(qp);
                                qPainterSetComp(qp, 9); // QPainter::CompositionMode_SourceAtop == 9
                                alignas(8) unsigned char cBuf[32] = {};
                                qColorCtor4(cBuf, sR, sG, sB, alpha);
                                ((QPainterFillRectQRectColorFn)origFillQRectColor)(qp, &r, cBuf);
                                qPainterRestore(qp);
                                InterlockedIncrement(&g_paintHits);
                                tls_inMultiply--;
                            }
                        }
                    }
                }
            }
        }
    }

    bool res = ((QPainterEndFn)origQPainterEnd)(qp);

    // Modify QImage pixels strictly in-place AFTER QPainter::end() releases its lock.
    // Zero extra memory, zero alpha corruption, zero top-level window offset!
    if (targetImg) {
        tls_inMultiply++;
        multiplyQImageRectInPlace(targetImg, px1, py1, px2, py2);
        tls_inMultiply--;
    }
    return res;
}

static void __fastcall hookKxEtViewPaint(void* self, void*, void* paintEvent) {
    bool etDark = (self && paintEvent && g_isEt && g_canvasMode == 1);
    if (etDark) tls_etPaintDepth++;
    ((KxViewPaintFn)origKxEtViewPaint)(self, paintEvent);
    if (etDark) tls_etPaintDepth--;
}

static void __fastcall hookKxViewPaint(void* self, void*, void* paintEvent) {
    InterlockedIncrement(&g_paintCalls);
    const char* cls = getWidgetClassName(self);
    if (cls && cls[0]) {
        strncpy(g_lastViewClass, cls, sizeof(g_lastViewClass) - 1);
        g_lastViewClass[sizeof(g_lastViewClass) - 1] = 0;
    }
    bool canvasOk = self && paintEvent && isDocCanvasView(cls) && !isFullscreenSlideshowWidget(self);
    if (canvasOk) InterlockedIncrement(&g_paintCanvasOk);
    bool etDark = (canvasOk && g_isEt && g_canvasMode == 1);
    bool wantMultiply = (g_canvasMode == 2) || (g_isWpp && g_canvasMode == 1);
    const QRectRaw* dirty = (wantMultiply && canvasOk && qPaintEventRect) ? qPaintEventRect(paintEvent) : nullptr;

    void* prevWppView = tls_curMultiplyView;
    QRectRaw prevDirty = tls_curDirtyRect;
    if (g_isWpp && wantMultiply && canvasOk && dirty && dirty->x2 >= dirty->x1 && dirty->y2 >= dirty->y1) {
        tls_curMultiplyView = self;
        tls_curDirtyRect = *dirty;
    }
    if (etDark) tls_etPaintDepth++;

    ((KxViewPaintFn)origKxViewPaint)(self, paintEvent);

    if (etDark) tls_etPaintDepth--;
    tls_curMultiplyView = prevWppView;
    tls_curDirtyRect = prevDirty;

    // On PPT (wpp.exe), multiply is applied in-place on WPS's own QPainter in hookQPainterEnd
    // to avoid opening a second QPainter on KxWppView's GaussianBlur backing store.
    if (g_isWpp) return;

    // Mode 2 on Word & Excel: native in-place QPainter::CompositionMode_Multiply (13).
    if (!wantMultiply || !canvasOk || tls_inMultiply > 0) return;
    if (!dirty || dirty->x2 < dirty->x1 || dirty->y2 < dirty->y1) return;

    alignas(8) unsigned char pBuf[64] = {};
    void* paintDev = (char*)self + 8; // QWidget : QObject (8 bytes), QPaintDevice
    tls_inMultiply++;
    qPainterCtorDev(pBuf, paintDev);
    if (qPainterIsActive(pBuf)) {
        qPainterSetComp(pBuf, 13); // QPainter::CompositionMode_Multiply == 13!
        alignas(8) unsigned char cBuf[32] = {};
        qColorCtor4(cBuf, (int)g_dimR, (int)g_dimG, (int)g_dimB, 255);
        ((QPainterFillRectQRectColorFn)origFillQRectColor)(pBuf, dirty, cBuf);
        InterlockedIncrement(&g_paintHits);
    }
    qPainterDtor(pBuf);
    tls_inMultiply--;
}

// Excel Mode 1 GetCacheDC / ReleaseCacheDC scope tracking
static void* __stdcall hookGetCacheDC(void* self) {
    InterlockedIncrement(&g_etCacheHits);
    void* pe = ((KxViewGetCacheDCFn)origGetCacheDC)(self);
    if (g_isEt && g_canvasMode == 1 && pe) {
        tls_etCachePainter = pe;
    }
    return pe;
}
static int __stdcall hookRelCacheDC(void* self, void* pe, bool repaint) {
    if (tls_etCachePainter == pe) tls_etCachePainter = nullptr;
    return ((KxViewRelCacheDCFn)origRelCacheDC)(self, pe, repaint);
}
static int __stdcall hookRelCacheDCNR(void* self, void* pe) {
    if (tls_etCachePainter == pe) tls_etCachePainter = nullptr;
    return ((KxViewRelCacheDCNoRepaintFn)origRelCacheDCNR)(self, pe);
}

// Excel Mode 1 fillRect & setPen hooks
static void __fastcall hookFillQRectColor(void* qp, void*, const void* r, const void* col) {
    if (g_isEt) InterlockedIncrement(&g_etFillAll);
    if (inEtDarkPainter(qp) && col) {
        int nr, ng, nb, na;
        if (remapEtFillRgba(qColorRgba(col), nr, ng, nb, na)) {
            alignas(8) unsigned char cBuf[32] = {};
            qColorCtor4(cBuf, nr, ng, nb, na);
            tls_inRemap++;
            ((QPainterFillRectQRectColorFn)origFillQRectColor)(qp, r, cBuf);
            tls_inRemap--;
            InterlockedIncrement(&g_etDarkHits);
            return;
        }
    }
    ((QPainterFillRectQRectColorFn)origFillQRectColor)(qp, r, col);
}

static void __fastcall hookFillQRectFColor(void* qp, void*, const void* r, const void* col) {
    if (inEtDarkPainter(qp) && col) {
        int nr, ng, nb, na;
        if (remapEtFillRgba(qColorRgba(col), nr, ng, nb, na)) {
            alignas(8) unsigned char cBuf[32] = {};
            qColorCtor4(cBuf, nr, ng, nb, na);
            tls_inRemap++;
            ((QPainterFillRectQRectFColorFn)origFillQRectFColor)(qp, r, cBuf);
            tls_inRemap--;
            InterlockedIncrement(&g_etDarkHits);
            return;
        }
    }
    ((QPainterFillRectQRectFColorFn)origFillQRectFColor)(qp, r, col);
}

static void __fastcall hookFillInt4Color(void* qp, void*, int x, int y, int w, int h, const void* col) {
    if (inEtDarkPainter(qp) && col) {
        int nr, ng, nb, na;
        if (remapEtFillRgba(qColorRgba(col), nr, ng, nb, na)) {
            alignas(8) unsigned char cBuf[32] = {};
            qColorCtor4(cBuf, nr, ng, nb, na);
            tls_inRemap++;
            ((QPainterFillRectInt4ColorFn)origFillInt4Color)(qp, x, y, w, h, cBuf);
            tls_inRemap--;
            InterlockedIncrement(&g_etDarkHits);
            return;
        }
    }
    ((QPainterFillRectInt4ColorFn)origFillInt4Color)(qp, x, y, w, h, col);
}

static bool tryRemapSolidBrush(const void* brush, unsigned char (&bBuf)[16]) {
    if (!brush || !qBrushStyle || !qBrushColor || !qBrushCopy || !qBrushSetColor) return false;
    if (qBrushStyle(brush) != 1) return false; // Qt::SolidPattern == 1
    const void* col = qBrushColor(brush);
    if (!col) return false;
    int nr, ng, nb, na;
    if (!remapEtFillRgba(qColorRgba(col), nr, ng, nb, na)) return false;
    qBrushCopy(bBuf, brush);
    alignas(8) unsigned char cBuf[32] = {};
    qColorCtor4(cBuf, nr, ng, nb, na);
    qBrushSetColor(bBuf, cBuf);
    return true;
}

static void __fastcall hookFillQRectBrush(void* qp, void*, const void* r, const void* brush) {
    if (inEtDarkPainter(qp)) {
        alignas(8) unsigned char bBuf[16] = {};
        if (tryRemapSolidBrush(brush, bBuf)) {
            tls_inRemap++;
            ((QPainterFillRectQRectBrushFn)origFillQRectBrush)(qp, r, bBuf);
            tls_inRemap--;
            qBrushDtor(bBuf);
            InterlockedIncrement(&g_etDarkHits);
            return;
        }
    }
    ((QPainterFillRectQRectBrushFn)origFillQRectBrush)(qp, r, brush);
}

static void __fastcall hookFillQRectFBrush(void* qp, void*, const void* r, const void* brush) {
    if (inEtDarkPainter(qp)) {
        alignas(8) unsigned char bBuf[16] = {};
        if (tryRemapSolidBrush(brush, bBuf)) {
            tls_inRemap++;
            ((QPainterFillRectQRectFBrushFn)origFillQRectFBrush)(qp, r, bBuf);
            tls_inRemap--;
            qBrushDtor(bBuf);
            InterlockedIncrement(&g_etDarkHits);
            return;
        }
    }
    ((QPainterFillRectQRectFBrushFn)origFillQRectFBrush)(qp, r, brush);
}

static void __fastcall hookFillInt4Brush(void* qp, void*, int x, int y, int w, int h, const void* brush) {
    if (inEtDarkPainter(qp)) {
        alignas(8) unsigned char bBuf[16] = {};
        if (tryRemapSolidBrush(brush, bBuf)) {
            tls_inRemap++;
            ((QPainterFillRectInt4BrushFn)origFillInt4Brush)(qp, x, y, w, h, bBuf);
            tls_inRemap--;
            qBrushDtor(bBuf);
            InterlockedIncrement(&g_etDarkHits);
            return;
        }
    }
    ((QPainterFillRectInt4BrushFn)origFillInt4Brush)(qp, x, y, w, h, brush);
}

static bool tryRemapPen(const void* pen, unsigned char (&penBuf)[16]) {
    if (!pen || !qPenStyle || !qPenColor || !qPenCopy || !qPenSetColor) return false;
    if (qPenStyle(pen) == 0) return false; // Qt::NoPen == 0
    alignas(8) unsigned char origCol[32] = {};
    qPenColor(pen, origCol);
    int nr, ng, nb, na;
    if (!remapEtPenRgba(qColorRgba(origCol), nr, ng, nb, na)) return false;
    qPenCopy(penBuf, pen);
    alignas(8) unsigned char newCol[32] = {};
    qColorCtor4(newCol, nr, ng, nb, na);
    qPenSetColor(penBuf, newCol);
    return true;
}

static void __fastcall hookSetPenColor(void* qp, void*, const void* col) {
    if (inEtDarkPainter(qp) && col) {
        int nr, ng, nb, na;
        if (remapEtPenRgba(qColorRgba(col), nr, ng, nb, na)) {
            alignas(8) unsigned char cBuf[32] = {};
            qColorCtor4(cBuf, nr, ng, nb, na);
            tls_inRemap++;
            ((QPainterSetPenColorFn)origSetPenColor)(qp, cBuf);
            tls_inRemap--;
            InterlockedIncrement(&g_etDarkHits);
            return;
        }
    }
    ((QPainterSetPenColorFn)origSetPenColor)(qp, col);
}

static void __fastcall hookSetPenPen(void* qp, void*, const void* pen) {
    if (inEtDarkPainter(qp)) {
        alignas(8) unsigned char pBuf[16] = {};
        if (tryRemapPen(pen, pBuf)) {
            tls_inRemap++;
            ((QPainterSetPenPenFn)origSetPenPen)(qp, pBuf);
            tls_inRemap--;
            qPenDtor(pBuf);
            InterlockedIncrement(&g_etDarkHits);
            return;
        }
    }
    ((QPainterSetPenPenFn)origSetPenPen)(qp, pen);
}

static void __fastcall hookPeFillColor(void* pe, void*, const void* col) {
    if (pe && inEtDarkPainter((char*)pe + 4) && col) {
        int nr, ng, nb, na;
        if (remapEtFillRgba(qColorRgba(col), nr, ng, nb, na)) {
            alignas(8) unsigned char cBuf[32] = {};
            qColorCtor4(cBuf, nr, ng, nb, na);
            tls_inRemap++;
            ((PainterExtFillColorFn)origPeFillColor)(pe, cBuf);
            tls_inRemap--;
            InterlockedIncrement(&g_etDarkHits);
            return;
        }
    }
    ((PainterExtFillColorFn)origPeFillColor)(pe, col);
}

static void __fastcall hookPeSetPenColor(void* pe, void*, const void* col) {
    if (pe && inEtDarkPainter((char*)pe + 4) && col) {
        int nr, ng, nb, na;
        if (remapEtPenRgba(qColorRgba(col), nr, ng, nb, na)) {
            alignas(8) unsigned char cBuf[32] = {};
            qColorCtor4(cBuf, nr, ng, nb, na);
            tls_inRemap++;
            ((PainterExtSetPenColorFn)origPeSetPenColor)(pe, cBuf);
            tls_inRemap--;
            InterlockedIncrement(&g_etDarkHits);
            return;
        }
    }
    ((PainterExtSetPenColorFn)origPeSetPenColor)(pe, col);
}

static void __fastcall hookPeSetPenPen(void* pe, void*, const void* pen) {
    if (pe && inEtDarkPainter((char*)pe + 4)) {
        alignas(8) unsigned char pBuf[16] = {};
        if (tryRemapPen(pen, pBuf)) {
            tls_inRemap++;
            ((PainterExtSetPenPenFn)origPeSetPenPen)(pe, pBuf);
            tls_inRemap--;
            qPenDtor(pBuf);
            InterlockedIncrement(&g_etDarkHits);
            return;
        }
    }
    ((PainterExtSetPenPenFn)origPeSetPenPen)(pe, pen);
}

static bool buildDimEffects(void* qp, const void* origEffects, unsigned char (&fxBuf)[16]) {
    int pct = (int)g_imgBrightness;
    if (!g_imgEnabled || pct >= 100 || !qp || !qFxDefaultCtor || !qFxCopyCtor || !qFxDtor ||
        !qFxSetBrightness || !qFxSetContrast) {
        return false;
    }
    // Never dim printers/PDF exports (QInternal::Printer == 4) or PPT fullscreen slideshow.
    if (qPainterDevice) {
        void* dev = qPainterDevice(qp);
        if (dev) {
            using DevTypeFn = int (__thiscall*)(void*);
            int dt = ((DevTypeFn)(*(void***)dev)[1])(dev);
            if (dt == 4) return false;
        }
    }
    if (isSlideshowForeground()) return false;

    if (pct < 25) pct = 25;
    double k = (double)pct / 100.0;
    if (origEffects) {
        qFxCopyCtor(fxBuf, origEffects);
    } else {
        qFxDefaultCtor(fxBuf);
    }
    double origB = qFxBrightness ? qFxBrightness(fxBuf) : 0.0;
    double origC = qFxContrast ? qFxContrast(fxBuf) : 1.0;
    if (origC <= 0.01) origC = 1.0;
    double newC = origC * k;
    double newB = origB * k + (k - 1.0) / (k + 1.0);
    if (newB < -0.95) newB = -0.95;
    if (newB > 0.95) newB = 0.95;
    qFxSetContrast(fxBuf, newC);
    qFxSetBrightness(fxBuf, newB);
    return true;
}

static void __fastcall hookPictureLayerRender(void* self, void*) {
    InterlockedIncrement(&g_picLayerCalls);
    tls_inPictureLayer++;
    ((PictureLayerRenderFn)origPictureLayerRender)(self);
    tls_inPictureLayer--;
}

// Unified Document Image Night-Mode Dimming Hooks (Word + Excel + PPT!)
static void __fastcall hookVariantImageRender(
    const void* self,
    void*,
    void* painter,
    const void* targetRect,
    const void* cropRect,
    const void* effects,
    bool b1,
    bool b2,
    bool b3,
    bool b4)
{
    InterlockedIncrement(&g_imgCalls);
    alignas(8) unsigned char fxBuf[16] = {};
    if (tls_inImgDim == 0 && painter && buildDimEffects((char*)painter + 4, effects, fxBuf)) {
        tls_inImgDim++;
        InterlockedIncrement(&g_imgHits);
        ((VariantImageRenderFn)origVariantRender)(self, painter, targetRect, cropRect, fxBuf, b1, b2, b3, b4);
        tls_inImgDim--;
        qFxDtor(fxBuf);
        return;
    }
    ((VariantImageRenderFn)origVariantRender)(self, painter, targetRect, cropRect, effects, b1, b2, b3, b4);
}

static void __fastcall hookNativeBitmapRender(
    const void* self,
    void*,
    void* qp,
    const void* dstRect,
    const void* srcRect,
    const void* effects)
{
    if (tls_inPictureLayer > 0) InterlockedIncrement(&g_natBmpCalls);
    alignas(8) unsigned char fxBuf[16] = {};
    if (tls_inImgDim == 0 && tls_inPictureLayer > 0 && buildDimEffects(qp, effects, fxBuf)) {
        tls_inImgDim++;
        InterlockedIncrement(&g_imgHits);
        ((NativeBitmapRenderFn)origNativeBitmapRender)(self, qp, dstRect, srcRect, fxBuf);
        tls_inImgDim--;
        qFxDtor(fxBuf);
        return;
    }
    ((NativeBitmapRenderFn)origNativeBitmapRender)(self, qp, dstRect, srcRect, effects);
}

static void __fastcall hookPeDrawImage5(
    void* pe,
    void*,
    const void* dstRect,
    const void* img,
    const void* srcRect,
    const void* effects,
    int flags)
{
    alignas(8) unsigned char fxBuf[16] = {};
    if (tls_inImgDim == 0 && tls_inPictureLayer > 0 && pe && buildDimEffects((char*)pe + 4, effects, fxBuf)) {
        tls_inImgDim++;
        InterlockedIncrement(&g_imgHits);
        ((PeDrawImage5Fn)origPeDrawImage5)(pe, dstRect, img, srcRect, fxBuf, flags);
        tls_inImgDim--;
        qFxDtor(fxBuf);
        return;
    }
    ((PeDrawImage5Fn)origPeDrawImage5)(pe, dstRect, img, srcRect, effects, flags);
}

static void __fastcall hookPeDrawImage4(
    void* pe,
    void*,
    const void* dstRect,
    const void* img,
    const void* srcRect,
    int flags)
{
    alignas(8) unsigned char fxBuf[16] = {};
    if (tls_inImgDim == 0 && tls_inPictureLayer > 0 && pe && tgtPeDrawImage5 &&
        buildDimEffects((char*)pe + 4, nullptr, fxBuf)) {
        tls_inImgDim++;
        InterlockedIncrement(&g_imgHits);
        ((PeDrawImage5Fn)origPeDrawImage5)(pe, dstRect, img, srcRect, fxBuf, flags);
        tls_inImgDim--;
        qFxDtor(fxBuf);
        return;
    }
    ((PeDrawImage4Fn)origPeDrawImage4)(pe, dstRect, img, srcRect, flags);
}

static bool loadSymbols() {
    if (g_loaded) return true;
    HMODULE qcore = GetModuleHandleW(L"Qt5CoreKso.dll");
    HMODULE qgui  = GetModuleHandleW(L"Qt5GuiKso.dll");
    HMODULE qwid  = GetModuleHandleW(L"Qt5WidgetsKso.dll");
    HMODULE shell = GetModuleHandleW(L"kshell.dll");
    HMODULE kso   = GetModuleHandleW(L"kso.dll");
    if (!qcore || !qgui || !qwid || !shell || !kso) return false;

    g_isWps = GetModuleHandleW(L"wpsmain.dll") != nullptr;
    g_isEt  = GetModuleHandleW(L"etmain.dll") != nullptr;
    g_isWpp = GetModuleHandleW(L"wppmain.dll") != nullptr;

    qMetaClassName = (ClassNameFn)GetProcAddress(qcore, "?className@QMetaObject@kso_qt@@QBEPBDXZ");
    qWidgetWindow = (WidgetWindowFn)GetProcAddress(qwid, "?window@QWidget@kso_qt@@QBEPAV12@XZ");
    qWidgetIsFullScreen = (WidgetBoolFn)GetProcAddress(qwid, "?isFullScreen@QWidget@kso_qt@@QBE_NXZ");
    qWidgetEffectiveWinId = (WidgetWinIdFn)GetProcAddress(qwid, "?effectiveWinId@QWidget@kso_qt@@QBEIXZ");
    qPaintEventRect = (PaintEventRectFn)GetProcAddress(qgui, "?rect@QPaintEvent@kso_qt@@QBEABVQRect@2@XZ");
    qColorCtor4 = (QColorCtor4Fn)GetProcAddress(qgui, "??0QColor@kso_qt@@QAE@HHHH@Z");
    qColorRgba = (QColorRgbaFn)GetProcAddress(qgui, "?rgba@QColor@kso_qt@@QBEIXZ");
    qPainterCtorDev = (QPainterCtorDevFn)GetProcAddress(qgui, "??0QPainter@kso_qt@@QAE@PAVQPaintDevice@1@@Z");
    qPainterDtor = (QPainterDtorFn)GetProcAddress(qgui, "??1QPainter@kso_qt@@QAE@XZ");
    qPainterIsActive = (QPainterIsActiveFn)GetProcAddress(qgui, "?isActive@QPainter@kso_qt@@QBE_NXZ");
    qPainterDevice = (QPainterDeviceFn)GetProcAddress(qgui, "?device@QPainter@kso_qt@@QBEPAVQPaintDevice@2@XZ");
    qPainterSave = (QPainterSaveRestoreFn)GetProcAddress(qgui, "?save@QPainter@kso_qt@@QAEXXZ");
    qPainterRestore = (QPainterSaveRestoreFn)GetProcAddress(qgui, "?restore@QPainter@kso_qt@@QAEXXZ");
    qPainterResetTransform = (QPainterSaveRestoreFn)GetProcAddress(qgui, "?resetTransform@QPainter@kso_qt@@QAEXXZ");
    qPainterSetComp = (QPainterSetCompFn)GetProcAddress(qgui, "?setCompositionMode@QPainter@kso_qt@@QAEXW4CompositionMode@12@@Z");
    qPainterSetClipping = (QPainterSetClipBoolFn)GetProcAddress(qgui, "?setClipping@QPainter@kso_qt@@QAEX_N@Z");
    qWidgetWidth = (QWidgetDimFn)GetProcAddress(qwid, "?width@QWidget@kso_qt@@QBEHXZ");
    qWidgetHeight = (QWidgetDimFn)GetProcAddress(qwid, "?height@QWidget@kso_qt@@QBEHXZ");
    qPaintDevWidth = (QPaintDevDimFn)GetProcAddress(qgui, "?width@QPaintDevice@kso_qt@@QBEHXZ");
    qPaintDevHeight = (QPaintDevDimFn)GetProcAddress(qgui, "?height@QPaintDevice@kso_qt@@QBEHXZ");
    qImageDpr = (QPaintDevDprFn)GetProcAddress(qgui, "?devicePixelRatio@QImage@kso_qt@@QBENXZ");
    qPixmapDpr = (QPaintDevDprFn)GetProcAddress(qgui, "?devicePixelRatio@QPixmap@kso_qt@@QBENXZ");
    qImageBits = (QImageBitsFn)GetProcAddress(qgui, "?bits@QImage@kso_qt@@QAEPAEXZ");
    qImageBytesPerLine = (QImageIntFn)GetProcAddress(qgui, "?bytesPerLine@QImage@kso_qt@@QBEHXZ");
    qImageFormat = (QImageIntFn)GetProcAddress(qgui, "?format@QImage@kso_qt@@QBE?AW4Format@12@XZ");
    qPixmapHandle = (QPixmapHandleFn)GetProcAddress(qgui, "?handle@QPixmap@kso_qt@@QBEPAVQPlatformPixmap@2@XZ");
    qRasterPpBuffer = (QRasterPpBufferFn)GetProcAddress(qgui, "?buffer@QRasterPlatformPixmap@kso_qt@@UAEPAVQImage@2@XZ");
    vtblQRasterPlatformPixmap = (void*)GetProcAddress(qgui, "??_7QRasterPlatformPixmap@kso_qt@@6B@");

    qBrushCopy = (QBrushCopyFn)GetProcAddress(qgui, "??0QBrush@kso_qt@@QAE@ABV01@@Z");
    qBrushDtor = (QBrushDtorFn)GetProcAddress(qgui, "??1QBrush@kso_qt@@QAE@XZ");
    qBrushStyle = (QBrushStyleFn)GetProcAddress(qgui, "?style@QBrush@kso_qt@@QBE?AW4BrushStyle@Qt@2@XZ");
    qBrushColor = (QBrushColorFn)GetProcAddress(qgui, "?color@QBrush@kso_qt@@QBEABVQColor@2@XZ");
    qBrushSetColor = (QBrushSetColorFn)GetProcAddress(qgui, "?setColor@QBrush@kso_qt@@QAEXABVQColor@2@@Z");

    qPenCopy = (QPenCopyFn)GetProcAddress(qgui, "??0QPen@kso_qt@@QAE@ABV01@@Z");
    qPenDtor = (QPenDtorFn)GetProcAddress(qgui, "??1QPen@kso_qt@@QAE@XZ");
    qPenStyle = (QPenStyleFn)GetProcAddress(qgui, "?style@QPen@kso_qt@@QBE?AW4PenStyle@Qt@2@XZ");
    qPenColor = (QPenColorFn)GetProcAddress(qgui, "?color@QPen@kso_qt@@QBE?AVQColor@2@XZ");
    qPenSetColor = (QPenSetColorFn)GetProcAddress(qgui, "?setColor@QPen@kso_qt@@QAEXABVQColor@2@@Z");

    qFxDefaultCtor = (QFxDefaultCtorFn)GetProcAddress(qgui, "??0QImageEffects@kso_qt@@QAE@XZ");
    qFxCopyCtor = (QFxCopyCtorFn)GetProcAddress(qgui, "??0QImageEffects@kso_qt@@QAE@ABV01@@Z");
    qFxDtor = (QFxDtorFn)GetProcAddress(qgui, "??1QImageEffects@kso_qt@@QAE@XZ");
    qFxBrightness = (QFxGetDoubleFn)GetProcAddress(qgui, "?brightness@QImageEffects@kso_qt@@QBENXZ");
    qFxContrast = (QFxGetDoubleFn)GetProcAddress(qgui, "?contrast@QImageEffects@kso_qt@@QBENXZ");
    qFxSetBrightness = (QFxSetDoubleFn)GetProcAddress(qgui, "?setBrightness@QImageEffects@kso_qt@@QAEXN@Z");
    qFxSetContrast = (QFxSetDoubleFn)GetProcAddress(qgui, "?setContrast@QImageEffects@kso_qt@@QAEXN@Z");

    tgtKxViewPaint = (void*)GetProcAddress(shell, "?paintEvent@KxView@@MAEXPAVQPaintEvent@kso_qt@@@Z");
    tgtQPainterEnd = (void*)GetProcAddress(qgui, "?end@QPainter@kso_qt@@QAE_NXZ");
    tgtPictureLayerRender = (void*)GetProcAddress(kso, "?render@PictureRenderLayer@drawing@@UAEXXZ");
    tgtVariantRender = (void*)GetProcAddress(kso, "?render@VariantImage@kpt@@QBEXPAVPainterExt@2@ABVQRectF@kso_qt@@ABVRelativeRect@2@PBVQImageEffects@5@_N444@Z");
    tgtNativeBitmapRender = (void*)GetProcAddress(kso, "?render@NativeBitmap@kpt@@QBEXPAVQPainter@kso_qt@@ABVQRectF@4@1PBVQImageEffects@4@@Z");
    tgtPeDrawImage5 = (void*)GetProcAddress(kso, "?drawImage@PainterExt@kpt@@QAEXABVQRectF@kso_qt@@ABVQImage@4@0PBVQImageEffects@4@V?$QFlags@W4ImageConversionFlag@Qt@kso_qt@@@4@@Z");
    tgtPeDrawImage4 = (void*)GetProcAddress(kso, "?drawImage@PainterExt@kpt@@QAEXABVQRectF@kso_qt@@ABVQImage@4@0V?$QFlags@W4ImageConversionFlag@Qt@kso_qt@@@4@@Z");
    tgtFillQRectColor = (void*)GetProcAddress(qgui, "?fillRect@QPainter@kso_qt@@QAEXABVQRect@2@ABVQColor@2@@Z");

    if (!qMetaClassName || !qPaintEventRect || !qColorCtor4 || !qColorRgba ||
        !qPainterCtorDev || !qPainterDtor || !qPainterIsActive || !qPainterSetComp ||
        !tgtKxViewPaint || !tgtVariantRender || !tgtFillQRectColor) {
        return false;
    }

    if (g_isEt) {
        HMODULE etm = GetModuleHandleW(L"etmain.dll");
        if (etm) {
            tgtKxEtViewPaint = (void*)GetProcAddress(etm, "?paintEvent@KxEtView@@MAEXPAVQPaintEvent@kso_qt@@@Z");
        }
        tgtGetCacheDC = (void*)GetProcAddress(shell, "?GetCacheDC@KxView@@MAGPAVPainterExt@kpt@@XZ");
        tgtRelCacheDC = (void*)GetProcAddress(shell, "?ReleaseCacheDC@KxView@@MAGHPAVPainterExt@kpt@@_N@Z");
        tgtRelCacheDCNR = (void*)GetProcAddress(shell, "?ReleaseCacheDC_NotRepaint@KxView@@MAGHPAVPainterExt@kpt@@@Z");
        tgtFillQRectFColor = (void*)GetProcAddress(qgui, "?fillRect@QPainter@kso_qt@@QAEXABVQRectF@2@ABVQColor@2@@Z");
        tgtFillInt4Color = (void*)GetProcAddress(qgui, "?fillRect@QPainter@kso_qt@@QAEXHHHHABVQColor@2@@Z");
        tgtFillQRectBrush = (void*)GetProcAddress(qgui, "?fillRect@QPainter@kso_qt@@QAEXABVQRect@2@ABVQBrush@2@@Z");
        tgtFillQRectFBrush = (void*)GetProcAddress(qgui, "?fillRect@QPainter@kso_qt@@QAEXABVQRectF@2@ABVQBrush@2@@Z");
        tgtFillInt4Brush = (void*)GetProcAddress(qgui, "?fillRect@QPainter@kso_qt@@QAEXHHHHABVQBrush@2@@Z");
        tgtSetPenColor = (void*)GetProcAddress(qgui, "?setPen@QPainter@kso_qt@@QAEXABVQColor@2@@Z");
        tgtSetPenPen = (void*)GetProcAddress(qgui, "?setPen@QPainter@kso_qt@@QAEXABVQPen@2@@Z");
        tgtPeFillColor = (void*)GetProcAddress(kso, "?fill@PainterExt@kpt@@QAEXABVQColor@kso_qt@@@Z");
        tgtPeSetPenColor = (void*)GetProcAddress(kso, "?setPen@PainterExt@kpt@@QAEXABVQColor@kso_qt@@@Z");
        tgtPeSetPenPen = (void*)GetProcAddress(kso, "?setPen@PainterExt@kpt@@QAEXABVQPen@kso_qt@@@Z");
    }

    // Point origFillQRectColor to direct target until hooked (for non-ET Mode 2 calls)
    if (!origFillQRectColor) origFillQRectColor = tgtFillQRectColor;
    g_loaded = true;
    return true;
}

static void toggleHookTarget(void* tgt, bool enable) {
    if (!tgt) return;
    if (enable) MH_EnableHook(tgt);
    else MH_DisableHook(tgt);
}

static bool ensureHooks(bool wantEnabled) {
    if (!wantEnabled) {
        if (g_hooksEnabled) {
            toggleHookTarget(tgtKxViewPaint, false);
            if (g_isWpp) toggleHookTarget(tgtQPainterEnd, false);
            toggleHookTarget(tgtPictureLayerRender, false);
            toggleHookTarget(tgtVariantRender, false);
            toggleHookTarget(tgtNativeBitmapRender, false);
            toggleHookTarget(tgtPeDrawImage5, false);
            toggleHookTarget(tgtPeDrawImage4, false);
            if (g_isEt) {
                toggleHookTarget(tgtKxEtViewPaint, false);
                toggleHookTarget(tgtGetCacheDC, false);
                toggleHookTarget(tgtRelCacheDC, false);
                toggleHookTarget(tgtRelCacheDCNR, false);
                toggleHookTarget(tgtFillQRectColor, false);
                toggleHookTarget(tgtFillQRectFColor, false);
                toggleHookTarget(tgtFillInt4Color, false);
                toggleHookTarget(tgtFillQRectBrush, false);
                toggleHookTarget(tgtFillQRectFBrush, false);
                toggleHookTarget(tgtFillInt4Brush, false);
                toggleHookTarget(tgtSetPenColor, false);
                toggleHookTarget(tgtSetPenPen, false);
                toggleHookTarget(tgtPeFillColor, false);
                toggleHookTarget(tgtPeSetPenColor, false);
                toggleHookTarget(tgtPeSetPenPen, false);
            }
            g_hooksEnabled = false;
        }
        return true;
    }

    if (!g_hooksInstalled) {
        MH_STATUS st = MH_Initialize();
        if (st != MH_OK && st != MH_ERROR_ALREADY_INITIALIZED) return false;
        if (MH_CreateHook(tgtKxViewPaint, (void*)&hookKxViewPaint, &origKxViewPaint) != MH_OK) return false;
        if (g_isWpp && tgtQPainterEnd) MH_CreateHook(tgtQPainterEnd, (void*)&hookQPainterEnd, &origQPainterEnd);
        if (tgtPictureLayerRender) MH_CreateHook(tgtPictureLayerRender, (void*)&hookPictureLayerRender, &origPictureLayerRender);
        if (MH_CreateHook(tgtVariantRender, (void*)&hookVariantImageRender, &origVariantRender) != MH_OK) return false;
        if (tgtNativeBitmapRender) MH_CreateHook(tgtNativeBitmapRender, (void*)&hookNativeBitmapRender, &origNativeBitmapRender);
        if (tgtPeDrawImage5) MH_CreateHook(tgtPeDrawImage5, (void*)&hookPeDrawImage5, &origPeDrawImage5);
        if (tgtPeDrawImage4) MH_CreateHook(tgtPeDrawImage4, (void*)&hookPeDrawImage4, &origPeDrawImage4);
        if (g_isEt) {
            if (tgtKxEtViewPaint) MH_CreateHook(tgtKxEtViewPaint, (void*)&hookKxEtViewPaint, &origKxEtViewPaint);
            if (tgtGetCacheDC) MH_CreateHook(tgtGetCacheDC, (void*)&hookGetCacheDC, &origGetCacheDC);
            if (tgtRelCacheDC) MH_CreateHook(tgtRelCacheDC, (void*)&hookRelCacheDC, &origRelCacheDC);
            if (tgtRelCacheDCNR) MH_CreateHook(tgtRelCacheDCNR, (void*)&hookRelCacheDCNR, &origRelCacheDCNR);
            if (tgtFillQRectColor) MH_CreateHook(tgtFillQRectColor, (void*)&hookFillQRectColor, &origFillQRectColor);
            if (tgtFillQRectFColor) MH_CreateHook(tgtFillQRectFColor, (void*)&hookFillQRectFColor, &origFillQRectFColor);
            if (tgtFillInt4Color) MH_CreateHook(tgtFillInt4Color, (void*)&hookFillInt4Color, &origFillInt4Color);
            if (tgtFillQRectBrush) MH_CreateHook(tgtFillQRectBrush, (void*)&hookFillQRectBrush, &origFillQRectBrush);
            if (tgtFillQRectFBrush) MH_CreateHook(tgtFillQRectFBrush, (void*)&hookFillQRectFBrush, &origFillQRectFBrush);
            if (tgtFillInt4Brush) MH_CreateHook(tgtFillInt4Brush, (void*)&hookFillInt4Brush, &origFillInt4Brush);
            if (tgtSetPenColor) MH_CreateHook(tgtSetPenColor, (void*)&hookSetPenColor, &origSetPenColor);
            if (tgtSetPenPen) MH_CreateHook(tgtSetPenPen, (void*)&hookSetPenPen, &origSetPenPen);
            if (tgtPeFillColor) MH_CreateHook(tgtPeFillColor, (void*)&hookPeFillColor, &origPeFillColor);
            if (tgtPeSetPenColor) MH_CreateHook(tgtPeSetPenColor, (void*)&hookPeSetPenColor, &origPeSetPenColor);
            if (tgtPeSetPenPen) MH_CreateHook(tgtPeSetPenPen, (void*)&hookPeSetPenPen, &origPeSetPenPen);
        }
        g_hooksInstalled = true;
    }

    if (!g_hooksEnabled) {
        toggleHookTarget(tgtKxViewPaint, true);
        if (g_isWpp) toggleHookTarget(tgtQPainterEnd, true);
        toggleHookTarget(tgtPictureLayerRender, true);
        toggleHookTarget(tgtVariantRender, true);
        toggleHookTarget(tgtNativeBitmapRender, true);
        toggleHookTarget(tgtPeDrawImage5, true);
        toggleHookTarget(tgtPeDrawImage4, true);
        if (g_isEt) {
            toggleHookTarget(tgtKxEtViewPaint, true);
            toggleHookTarget(tgtGetCacheDC, true);
            toggleHookTarget(tgtRelCacheDC, true);
            toggleHookTarget(tgtRelCacheDCNR, true);
            toggleHookTarget(tgtFillQRectColor, true);
            toggleHookTarget(tgtFillQRectFColor, true);
            toggleHookTarget(tgtFillInt4Color, true);
            toggleHookTarget(tgtFillQRectBrush, true);
            toggleHookTarget(tgtFillQRectFBrush, true);
            toggleHookTarget(tgtFillInt4Brush, true);
            toggleHookTarget(tgtSetPenColor, true);
            toggleHookTarget(tgtSetPenPen, true);
            toggleHookTarget(tgtPeFillColor, true);
            toggleHookTarget(tgtPeSetPenColor, true);
            toggleHookTarget(tgtPeSetPenPen, true);
        }
        g_hooksEnabled = true;
    }
    return true;
}

static void refreshAllViews() {
    HMODULE qwid = GetModuleHandleW(L"Qt5WidgetsKso.dll");
    HMODULE shell = GetModuleHandleW(L"kshell.dll");
    if (!qwid || !shell) return;
    auto all = (AllWidgets)GetProcAddress(qwid, "?allWidgets@QApplication@kso_qt@@SA?AV?$QList@PAVQWidget@kso_qt@@@2@XZ");
    auto update = (NoArg)GetProcAddress(qwid, "?update@QWidget@kso_qt@@QAEXXZ");
    auto invalidate = (NoArg)GetProcAddress(shell, "?invalidateViewDirtyRegion@KxView@@UAEXXZ");
    auto addInvalid = (KxViewAddInvalidFn)GetProcAddress(shell, "?addToInvalidateArea@KxView@@MAEXABVQRect@kso_qt@@@Z");
    auto viewInvalidated = (KxViewVoidFn)GetProcAddress(shell, "?viewInvalidated@KxView@@QAEXXZ");
    auto updateView = (NoArg)GetProcAddress(shell, "?updateView@KxView@@IAEXXZ");
    if (!all || !update) return;
    QStringList list = {nullptr};
    all(&list);
    if (list.d) {
        LONG found = 0;
        for (int i = list.d->begin; i < list.d->end; i++) {
            void* w = list.d->array[i];
            if (!w) continue;
            void* v = ((CastGet)(*(void***)w)[1])(w, "KxView");
            if (v) {
                found++;
                if (addInvalid && qWidgetWidth && qWidgetHeight) {
                    int ww = qWidgetWidth(w);
                    int hh = qWidgetHeight(w);
                    if (ww > 0 && hh > 0) {
                        QRectRaw full = {0, 0, ww - 1, hh - 1};
                        addInvalid(v, &full);
                    }
                }
                if (invalidate) invalidate(v);
                if (viewInvalidated) viewInvalidated(v);
                if (updateView) updateView(v);
                update(w);
            }
        }
        g_viewsFound = found;
        blue::releaseWidgets(list);
    }
}

static int parseDec(const char* s, int fallback) {
    if (!s || !*s) return fallback;
    int sign = 1, val = 0;
    if (*s == '-') { sign = -1; s++; }
    while (*s >= '0' && *s <= '9') {
        val = val * 10 + (*s - '0');
        s++;
    }
    return val * sign;
}

static bool apply(Request* req) {
    if (!loadSymbols()) {
        req->status = -91;
        return false;
    }

    bool doSet = false;
    for (DWORD i = 0; i < req->count; i++) {
        auto& r = req->records[i];
        if (!strcmp(r.kind, "set")) {
            doSet = true;
            if (!strcmp(r.name, "canvas_mode")) {
                if (!strcmp(r.value, "dark")) g_canvasMode = 1;
                else if (!strcmp(r.value, "soft_dim")) g_canvasMode = 2;
                else g_canvasMode = 0;
            } else if (!strcmp(r.name, "dim_color")) {
                DWORD rgb = 0;
                if (blue::parseHex(r.value, rgb)) {
                    g_dimR = (rgb >> 16) & 0xFF;
                    g_dimG = (rgb >> 8) & 0xFF;
                    g_dimB = rgb & 0xFF;
                }
            } else if (!strcmp(r.name, "dark_bg")) {
                DWORD argb = 0;
                if (blue::parseHex(r.value, argb)) g_darkBg = (LONG)argb;
            } else if (!strcmp(r.name, "dark_fg")) {
                DWORD argb = 0;
                if (blue::parseHex(r.value, argb)) g_darkFg = (LONG)argb;
            } else if (!strcmp(r.name, "img_enabled")) {
                g_imgEnabled = (!strcmp(r.value, "1") || !strcmp(r.value, "true")) ? 1 : 0;
            } else if (!strcmp(r.name, "img_brightness")) {
                int v = parseDec(r.value, 70);
                if (v < 25) v = 25;
                if (v > 100) v = 100;
                g_imgBrightness = v;
            }
        }
    }

    if (doSet) {
        bool needHooks = (g_canvasMode != 0) || (g_imgEnabled != 0 && g_imgBrightness < 100);
        if (!ensureHooks(needHooks)) {
            req->status = -92;
            return false;
        }
        refreshAllViews();
    }

    for (DWORD i = 0; i < req->count; i++) {
        auto& r = req->records[i];
        if (!strcmp(r.kind, "query")) {
            wsprintfA(r.value, "mode=%d|img=%d:%d|hooks=%d|views=%ld|paint=%ld/%ld/%ld|et=%ld/%ld/%ld|pic=%ld/%ld/%ld/%ld|cls=%s",
                (int)g_canvasMode,
                (int)g_imgEnabled,
                (int)g_imgBrightness,
                g_hooksEnabled ? 1 : 0,
                (long)g_viewsFound,
                (long)g_paintCalls,
                (long)g_paintCanvasOk,
                (long)g_paintHits,
                (long)g_etCacheHits,
                (long)g_etFillAll,
                (long)g_etDarkHits,
                (long)g_picLayerCalls,
                (long)g_imgCalls,
                (long)g_natBmpCalls,
                (long)g_imgHits,
                g_lastViewClass[0] ? g_lastViewClass : "-");
        }
    }
    return true;
}

} // namespace canvas_eye
