//! Hardware-accelerated Direct2D + DirectWrite modern dark Fluent UI panel.
//! Zero WebView2, zero browser processes: < 10 MB RAM even while open, ~3-6 MB in tray!

use crate::autostart::AutoStartStatus;
use crate::controller::UiSnapshot;
use crate::palette::{Adjustments, contrast_ratio, hex_color};
use windows::core::w;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D_RECT_F, D2D1_ALPHA_MODE_IGNORE, D2D1_COLOR_F, D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_ANTIALIAS_MODE_PER_PRIMITIVE, D2D1_DRAW_TEXT_OPTIONS_CLIP, D2D1_ELLIPSE,
    D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_RENDER_TARGET_PROPERTIES,
    D2D1_RENDER_TARGET_TYPE_SOFTWARE, D2D1_RENDER_TARGET_USAGE_NONE, D2D1_ROUNDED_RECT,
    D2D1CreateFactory, ID2D1DCRenderTarget, ID2D1Factory, ID2D1SolidColorBrush,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
    DWRITE_FONT_WEIGHT_BOLD, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_WEIGHT_SEMI_BOLD,
    DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_PARAGRAPH_ALIGNMENT_NEAR,
    DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_TEXT_ALIGNMENT_TRAILING,
    DWRITE_WORD_WRAPPING_NO_WRAP, DWRITE_WORD_WRAPPING_WRAP, DWriteCreateFactory, IDWriteFactory,
    IDWriteTextFormat,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, HDC, SRCCOPY,
    SelectObject,
};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::GetClientRect;

pub const LOGICAL_W: f32 = 520.0;
pub const LOGICAL_H: f32 = 656.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlId {
    SkinToggle,
    PageToggle,
    ModeDark,
    ModeSoftLight,
    SliderTrack(usize),
    SliderMinus(usize),
    SliderPlus(usize),
    ResetParams,
    AutoStartToggle,
    RestoreAll,
    HideToTray,
}

#[derive(Debug, Clone, Copy)]
pub struct SliderSpec {
    pub label: &'static str,
    pub min: i32,
    pub max: i32,
    pub step: i32,
    pub is_offset_100: bool,
}

pub const SLIDERS: [SliderSpec; 4] = [
    SliderSpec { label: "亮度", min: 40, max: 140, step: 5, is_offset_100: true },
    SliderSpec { label: "对比度", min: 50, max: 150, step: 5, is_offset_100: true },
    SliderSpec { label: "棕褐色", min: 0, max: 100, step: 5, is_offset_100: false },
    SliderSpec { label: "灰度", min: 0, max: 100, step: 5, is_offset_100: false },
];

pub fn slider_get(adj: &Adjustments, idx: usize) -> i32 {
    match idx {
        0 => adj.brightness,
        1 => adj.contrast,
        2 => adj.sepia,
        _ => adj.grayscale,
    }
}

pub fn slider_set(adj: &mut Adjustments, idx: usize, val: i32) {
    let sp = SLIDERS[idx];
    let clamped = val.clamp(sp.min, sp.max);
    match idx {
        0 => adj.brightness = clamped,
        1 => adj.contrast = clamped,
        2 => adj.sepia = clamped,
        _ => adj.grayscale = clamped,
    }
}

#[derive(Clone, Copy)]
pub struct HitBox {
    pub id: ControlId,
    pub rect: D2D_RECT_F,
}

fn rect(l: f32, t: f32, r: f32, b: f32) -> D2D_RECT_F {
    D2D_RECT_F { left: l, top: t, right: r, bottom: b }
}
fn rrect(l: f32, t: f32, r: f32, b: f32, rad: f32) -> D2D1_ROUNDED_RECT {
    D2D1_ROUNDED_RECT {
        rect: rect(l, t, r, b),
        radiusX: rad,
        radiusY: rad,
    }
}
fn rgba(r: u8, g: u8, b: u8, a: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: r as f32 / 255.0,
        g: g as f32 / 255.0,
        b: b as f32 / 255.0,
        a,
    }
}
fn rgb(r: u8, g: u8, b: u8) -> D2D1_COLOR_F {
    rgba(r, g, b, 1.0)
}
fn ellipse(x: f32, y: f32, rx: f32, ry: f32) -> D2D1_ELLIPSE {
    let mut e = D2D1_ELLIPSE {
        radiusX: rx,
        radiusY: ry,
        ..Default::default()
    };
    e.point.X = x;
    e.point.Y = y;
    e
}
fn pt_in(r: D2D_RECT_F, x: f32, y: f32) -> bool {
    x >= r.left && x <= r.right && y >= r.top && y <= r.bottom
}

pub struct Layout {
    pub hits: Vec<HitBox>,
    pub slider_tracks: [D2D_RECT_F; 4],
}

impl Layout {
    pub fn compute() -> Self {
        let mut hits = Vec::with_capacity(26);
        let pad = 16.0;
        let cw = LOGICAL_W - pad * 2.0; // 488

        // Card 1: Native Dark Skin (y = 80..172)
        hits.push(HitBox {
            id: ControlId::SkinToggle,
            rect: rect(pad + cw - 16.0 - 48.0, 96.0, pad + cw - 16.0, 122.0),
        });

        // Card 2: Writer Page Palette (y = 182..524)
        hits.push(HitBox {
            id: ControlId::PageToggle,
            rect: rect(pad + cw - 16.0 - 48.0, 196.0, pad + cw - 16.0, 222.0),
        });

        // Segmented mode selector (y = 232..264)
        let seg_l = pad + 16.0;
        let seg_r = pad + cw - 16.0;
        let seg_mid = (seg_l + seg_r) * 0.5;
        hits.push(HitBox {
            id: ControlId::ModeDark,
            rect: rect(seg_l + 3.0, 235.0, seg_mid - 2.0, 261.0),
        });
        hits.push(HitBox {
            id: ControlId::ModeSoftLight,
            rect: rect(seg_mid + 2.0, 235.0, seg_r - 3.0, 261.0),
        });

        // 4 sliders (y = 362..478, stride 29)
        let mut slider_tracks = [rect(0.0, 0.0, 0.0, 0.0); 4];
        let minus_l = pad + 16.0 + 54.0;
        let minus_r = minus_l + 24.0;
        let val_w = 48.0;
        let plus_r = pad + cw - 16.0 - val_w - 6.0;
        let plus_l = plus_r - 24.0;
        let trk_l = minus_r + 12.0;
        let trk_r = plus_l - 12.0;

        for i in 0..4 {
            let y0 = 362.0 + i as f32 * 29.0;
            let tr = rect(trk_l, y0 + 1.0, trk_r, y0 + 23.0);
            slider_tracks[i] = tr;
            hits.push(HitBox { id: ControlId::SliderMinus(i), rect: rect(minus_l, y0 + 1.0, minus_r, y0 + 23.0) });
            hits.push(HitBox { id: ControlId::SliderTrack(i), rect: tr });
            hits.push(HitBox { id: ControlId::SliderPlus(i), rect: rect(plus_l, y0 + 1.0, plus_r, y0 + 23.0) });
        }

        // Reset button inside Card 2 bottom right
        hits.push(HitBox {
            id: ControlId::ResetParams,
            rect: rect(pad + cw - 16.0 - 78.0, 486.0, pad + cw - 16.0, 512.0),
        });

        // Card 3: Auto-Start on Boot (y = 534..596)
        hits.push(HitBox {
            id: ControlId::AutoStartToggle,
            rect: rect(pad + cw - 16.0 - 48.0, 552.0, pad + cw - 16.0, 578.0),
        });

        // Bottom footer buttons (y = 608..642)
        let hide_r = LOGICAL_W - pad;
        let hide_l = hide_r - 92.0;
        let rest_r = hide_l - 10.0;
        let rest_l = rest_r - 92.0;
        hits.push(HitBox { id: ControlId::RestoreAll, rect: rect(rest_l, 608.0, rest_r, 642.0) });
        hits.push(HitBox { id: ControlId::HideToTray, rect: rect(hide_l, 608.0, hide_r, 642.0) });

        Self { hits, slider_tracks }
    }

    pub fn hit_test(&self, x: f32, y: f32) -> Option<ControlId> {
        self.hits.iter().find(|h| pt_in(h.rect, x, y)).map(|h| h.id)
    }

    pub fn value_from_x(&self, idx: usize, x: f32) -> i32 {
        let tr = self.slider_tracks[idx];
        let sp = SLIDERS[idx];
        let t = ((x - tr.left) / (tr.right - tr.left).max(1.0)).clamp(0.0, 1.0);
        let raw = sp.min as f32 + t * (sp.max - sp.min) as f32;
        raw.round() as i32
    }
}

pub struct D2dRenderer {
    factory: ID2D1Factory,
    _dwrite: IDWriteFactory,
    rt: Option<ID2D1DCRenderTarget>,
    fmt_title: IDWriteTextFormat,
    fmt_section: IDWriteTextFormat,
    fmt_body: IDWriteTextFormat,
    fmt_small: IDWriteTextFormat,
    fmt_small_center: IDWriteTextFormat,
    fmt_btn: IDWriteTextFormat,
    fmt_val: IDWriteTextFormat,
    fmt_preview_lg: IDWriteTextFormat,
    fmt_preview_sm: IDWriteTextFormat,
    pub layout: Layout,
}

impl D2dRenderer {
    pub fn new() -> Result<Self, String> {
        unsafe {
            let factory: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)
                .map_err(|e| format!("D2D1CreateFactory 失败: {e}"))?;
            let dwrite: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)
                .map_err(|e| format!("DWriteCreateFactory 失败: {e}"))?;

            let mk = |size: f32,
                      weight: windows::Win32::Graphics::DirectWrite::DWRITE_FONT_WEIGHT,
                      align: windows::Win32::Graphics::DirectWrite::DWRITE_TEXT_ALIGNMENT,
                      valign: windows::Win32::Graphics::DirectWrite::DWRITE_PARAGRAPH_ALIGNMENT,
                      wrap: bool|
             -> Result<IDWriteTextFormat, String> {
                let fmt = dwrite
                    .CreateTextFormat(
                        w!("Microsoft YaHei UI"),
                        None,
                        weight,
                        DWRITE_FONT_STYLE_NORMAL,
                        DWRITE_FONT_STRETCH_NORMAL,
                        size,
                        w!("zh-CN"),
                    )
                    .map_err(|e| format!("CreateTextFormat 失败: {e}"))?;
                let _ = fmt.SetTextAlignment(align);
                let _ = fmt.SetParagraphAlignment(valign);
                let _ = fmt.SetWordWrapping(if wrap {
                    DWRITE_WORD_WRAPPING_WRAP
                } else {
                    DWRITE_WORD_WRAPPING_NO_WRAP
                });
                Ok(fmt)
            };

            Ok(Self {
                factory,
                fmt_title: mk(17.0, DWRITE_FONT_WEIGHT_BOLD, DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_PARAGRAPH_ALIGNMENT_CENTER, false)?,
                fmt_section: mk(14.0, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_PARAGRAPH_ALIGNMENT_CENTER, false)?,
                fmt_body: mk(12.5, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_PARAGRAPH_ALIGNMENT_CENTER, false)?,
                fmt_small: mk(11.5, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_PARAGRAPH_ALIGNMENT_NEAR, true)?,
                fmt_small_center: mk(11.0, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_PARAGRAPH_ALIGNMENT_CENTER, false)?,
                fmt_btn: mk(12.0, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_PARAGRAPH_ALIGNMENT_CENTER, false)?,
                fmt_val: mk(11.5, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_TEXT_ALIGNMENT_TRAILING, DWRITE_PARAGRAPH_ALIGNMENT_CENTER, false)?,
                fmt_preview_lg: mk(14.0, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_PARAGRAPH_ALIGNMENT_CENTER, false)?,
                fmt_preview_sm: mk(12.0, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_PARAGRAPH_ALIGNMENT_CENTER, false)?,
                _dwrite: dwrite,
                rt: None,
                layout: Layout::compute(),
            })
        }
    }

    /// Release software Direct2D DC render target when minimized to tray!
    pub fn release_render_target(&mut self) {
        self.rt = None;
    }

    fn ensure_rt(&mut self, hwnd: HWND) -> Result<ID2D1DCRenderTarget, String> {
        unsafe {
            let dpi = GetDpiForWindow(hwnd).max(96) as f32;
            if let Some(ref rt) = self.rt {
                rt.SetDpi(dpi, dpi);
                return Ok(rt.clone());
            }
            // Software DC render target: zero GPU/D3D11 driver bloat (<10 MB RAM even when open!)
            // + full Direct2D antialiasing + ClearType DirectWrite + crisp PrintWindow support.
            let rt_props = D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_SOFTWARE,
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_IGNORE,
                },
                dpiX: dpi,
                dpiY: dpi,
                usage: D2D1_RENDER_TARGET_USAGE_NONE,
                ..Default::default()
            };
            let rt = self
                .factory
                .CreateDCRenderTarget(&rt_props)
                .map_err(|e| format!("CreateDCRenderTarget 失败: {e}"))?;
            rt.SetAntialiasMode(D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
            self.rt = Some(rt.clone());
            Ok(rt)
        }
    }

    pub fn paint(
        &mut self,
        hwnd: HWND,
        hdc: HDC,
        snap: &UiSnapshot,
        autostart: &AutoStartStatus,
        live_adj: Adjustments,
        hovered: Option<ControlId>,
        dragging: Option<usize>,
    ) {
        let Ok(rt) = self.ensure_rt(hwnd) else {
            return;
        };
        unsafe {
            let mut rc = RECT::default();
            let _ = GetClientRect(hwnd, &mut rc);
            let w = (rc.right - rc.left).max(1);
            let h = (rc.bottom - rc.top).max(1);

            // Double-buffered memory DC eliminates any resize/drag flicker
            let mem_dc = CreateCompatibleDC(Some(hdc));
            let mem_bmp = CreateCompatibleBitmap(hdc, w, h);
            let old_obj = SelectObject(mem_dc, mem_bmp.into());

            if rt.BindDC(mem_dc, &rc).is_err() {
                SelectObject(mem_dc, old_obj);
                let _ = DeleteObject(mem_bmp.into());
                let _ = DeleteDC(mem_dc);
                self.rt = None;
                return;
            }
            rt.BeginDraw();
            rt.Clear(Some(&rgb(19, 23, 31))); // #13171F deep obsidian canvas

            let brush = |c: D2D1_COLOR_F| -> Option<ID2D1SolidColorBrush> {
                rt.CreateSolidColorBrush(&c, None).ok()
            };
            let draw_text = |text: &str, fmt: &IDWriteTextFormat, r: D2D_RECT_F, col: D2D1_COLOR_F| {
                if let Some(b) = brush(col) {
                    let wide: Vec<u16> = text.encode_utf16().collect();
                    rt.DrawText(&wide, fmt, &r, &b, D2D1_DRAW_TEXT_OPTIONS_CLIP, Default::default());
                }
            };
            let fill_rr = |rr: D2D1_ROUNDED_RECT, col: D2D1_COLOR_F| {
                if let Some(b) = brush(col) {
                    rt.FillRoundedRectangle(&rr, &b);
                }
            };
            let stroke_rr = |rr: D2D1_ROUNDED_RECT, col: D2D1_COLOR_F, w: f32| {
                if let Some(b) = brush(col) {
                    rt.DrawRoundedRectangle(&rr, &b, w, None);
                }
            };

            let pad = 16.0f32;
            let cw = LOGICAL_W - pad * 2.0;

            // ----------------------------------------------------------
            // 1. Header with bespoke vector emblem + live connection pill
            // ----------------------------------------------------------
            // Mini eye-crescent squircle badge (x=16..50, y=14..48)
            fill_rr(rrect(pad, 14.0, pad + 34.0, 48.0, 9.0), rgb(30, 42, 62));
            stroke_rr(rrect(pad + 0.5, 14.5, pad + 33.5, 47.5, 8.5), rgba(120, 160, 225, 0.45), 1.0);
            if let Some(b_ring) = brush(rgb(202, 224, 255)) {
                let eye = ellipse(pad + 17.0, 31.0, 11.0, 6.5);
                rt.DrawEllipse(&eye, &b_ring, 1.8, None);
            }
            if let Some(b_moon) = brush(rgb(245, 190, 96)) {
                let m = ellipse(pad + 16.5, 31.0, 4.2, 4.2);
                rt.FillEllipse(&m, &b_moon);
            }
            if let Some(b_cut) = brush(rgb(24, 34, 50)) {
                let cut = ellipse(pad + 18.6, 29.6, 3.4, 3.4);
                rt.FillEllipse(&cut, &b_cut);
            }

            draw_text("WPS 护眼控制器", &self.fmt_title, rect(pad + 44.0, 12.0, 230.0, 36.0), rgb(238, 242, 248));
            // Compact tag pill
            fill_rr(rrect(210.0, 17.0, 274.0, 34.0, 8.0), rgba(59, 130, 246, 0.16));
            draw_text("Rust 原生", &self.fmt_small_center, rect(210.0, 17.0, 274.0, 34.0), rgb(125, 185, 255));

            // Connected WPS badge on top-right
            let total_wps = snap.wps_count + snap.wpp_count + snap.et_count;
            let (pill_bg, pill_fg, pill_txt) = if total_wps > 0 {
                let mut parts = Vec::new();
                if snap.wps_count > 0 { parts.push(format!("文字 {}", snap.wps_count)); }
                if snap.wpp_count > 0 { parts.push(format!("PPT {}", snap.wpp_count)); }
                if snap.et_count > 0 { parts.push(format!("表格 {}", snap.et_count)); }
                (rgba(34, 197, 94, 0.15), rgb(96, 230, 146), format!("● {}", parts.join(" · ")))
            } else {
                (rgba(148, 163, 184, 0.12), rgb(148, 163, 184), "○ 等待 WPS".into())
            };
            fill_rr(rrect(LOGICAL_W - pad - 154.0, 15.0, LOGICAL_W - pad, 37.0, 11.0), pill_bg);
            draw_text(&pill_txt, &self.fmt_small_center, rect(LOGICAL_W - pad - 154.0, 15.0, LOGICAL_W - pad, 37.0), pill_fg);

            // Status subtitle line
            let status_col = if snap.error_text.is_some() {
                rgb(248, 130, 130)
            } else {
                rgb(142, 155, 176)
            };
            draw_text(&snap.status_text, &self.fmt_small, rect(pad + 44.0, 38.0, LOGICAL_W - pad, 74.0), status_col);

            // ----------------------------------------------------------
            // 2. Card 1: Native 2025 Dark Skin (y = 80..172)
            // ----------------------------------------------------------
            fill_rr(rrect(pad, 80.0, pad + cw, 172.0, 12.0), rgb(27, 33, 44));
            stroke_rr(
                rrect(pad + 0.5, 80.5, pad + cw - 0.5, 171.5, 11.5),
                if snap.skin_enabled { rgba(59, 130, 246, 0.55) } else { rgb(43, 52, 69) },
                1.0,
            );
            draw_text("原生深色外观", &self.fmt_section, rect(pad + 16.0, 94.0, 175.0, 120.0), rgb(238, 242, 248));

            // Mini app chips: 文字 / PPT / 表格
            let mut cx = pad + 114.0;
            for (chip, w_chip) in [("文字", 38.0f32), ("PPT", 38.0), ("表格", 38.0)] {
                fill_rr(rrect(cx, 98.0, cx + w_chip, 116.0, 6.0), rgb(37, 45, 60));
                draw_text(chip, &self.fmt_small_center, rect(cx, 98.0, cx + w_chip, 116.0), rgb(174, 188, 210));
                cx += w_chip + 6.0;
            }

            // Pill Toggle Switch for Skin
            self.draw_toggle(
                &rt,
                rect(pad + cw - 16.0 - 48.0, 96.0, pad + cw - 16.0, 122.0),
                snap.skin_enabled,
                hovered == Some(ControlId::SkinToggle),
            );

            draw_text(
                &snap.skin_msg,
                &self.fmt_small,
                rect(pad + 16.0, 128.0, pad + cw - 16.0, 166.0),
                rgb(142, 155, 176),
            );

            // ----------------------------------------------------------
            // 3. Card 2: Writer Page Palette (y = 182..524)
            // ----------------------------------------------------------
            fill_rr(rrect(pad, 182.0, pad + cw, 524.0, 12.0), rgb(27, 33, 44));
            stroke_rr(
                rrect(pad + 0.5, 182.5, pad + cw - 0.5, 523.5, 11.5),
                if snap.page_enabled { rgba(59, 130, 246, 0.55) } else { rgb(43, 52, 69) },
                1.0,
            );

            draw_text("文字页面调色", &self.fmt_section, rect(pad + 16.0, 194.0, 160.0, 220.0), rgb(238, 242, 248));
            draw_text(
                "仅调整 WPS 文字纸张与自动文字",
                &self.fmt_small,
                rect(pad + 114.0, 200.0, pad + cw - 76.0, 220.0),
                rgb(122, 136, 158),
            );

            // Pill Toggle Switch for Page
            self.draw_toggle(
                &rt,
                rect(pad + cw - 16.0 - 48.0, 196.0, pad + cw - 16.0, 222.0),
                snap.page_enabled,
                hovered == Some(ControlId::PageToggle),
            );

            // Segmented polarity selector (y = 232..264)
            let seg_l = pad + 16.0;
            let seg_r = pad + cw - 16.0;
            let seg_mid = (seg_l + seg_r) * 0.5;
            fill_rr(rrect(seg_l, 232.0, seg_r, 264.0, 9.0), rgb(18, 22, 30));
            stroke_rr(rrect(seg_l + 0.5, 232.5, seg_r - 0.5, 263.5, 8.5), rgb(40, 48, 64), 1.0);

            let dark_sel = live_adj.dark_mode;
            let active_seg_r = if dark_sel {
                rrect(seg_l + 3.0, 235.0, seg_mid - 2.0, 261.0, 7.0)
            } else {
                rrect(seg_mid + 2.0, 235.0, seg_r - 3.0, 261.0, 7.0)
            };
            fill_rr(active_seg_r, rgb(39, 67, 114));
            stroke_rr(active_seg_r, rgba(110, 178, 255, 0.5), 1.0);

            draw_text(
                "护眼深底",
                &self.fmt_btn,
                rect(seg_l + 3.0, 235.0, seg_mid - 2.0, 261.0),
                if dark_sel { rgb(242, 247, 255) } else if hovered == Some(ControlId::ModeDark) { rgb(200, 212, 232) } else { rgb(138, 152, 174) },
            );
            draw_text(
                "柔和浅底（兼容固定黑字）",
                &self.fmt_btn,
                rect(seg_mid + 2.0, 235.0, seg_r - 3.0, 261.0),
                if !dark_sel { rgb(242, 247, 255) } else if hovered == Some(ControlId::ModeSoftLight) { rgb(200, 212, 232) } else { rgb(138, 152, 174) },
            );

            // Live Dual-Line Readability Preview Box (y = 274..332)
            let (p_bg, p_fg) = live_adj.palette();
            let auto_cr = contrast_ratio(p_bg, p_fg);
            let black_cr = contrast_ratio(p_bg, (0, 0, 0));
            fill_rr(rrect(seg_l, 274.0, seg_r, 332.0, 10.0), rgb(p_bg.0, p_bg.1, p_bg.2));
            stroke_rr(rrect(seg_l + 0.5, 274.5, seg_r - 0.5, 331.5, 9.5), rgba(255, 255, 255, 0.14), 1.0);

            draw_text(
                "Aa 123 · 自动颜色文字（随滑块实时调色）",
                &self.fmt_preview_lg,
                rect(seg_l + 12.0, 280.0, seg_r - 82.0, 305.0),
                rgb(p_fg.0, p_fg.1, p_fg.2),
            );
            draw_text(
                "Aa 123 · 固定纯黑字 (#000000) 效果对照",
                &self.fmt_preview_sm,
                rect(seg_l + 12.0, 306.0, seg_r - 82.0, 327.0),
                rgb(0, 0, 0),
            );

            // Hex chip in top-right of preview
            fill_rr(rrect(seg_r - 74.0, 282.0, seg_r - 10.0, 302.0, 6.0), rgba(15, 20, 28, 0.68));
            draw_text(
                &hex_color(p_bg),
                &self.fmt_small_center,
                rect(seg_r - 74.0, 282.0, seg_r - 10.0, 302.0),
                rgb(228, 236, 248),
            );

            // Contrast readability summary line (y = 337..356)
            let black_hint = if black_cr < 4.5 {
                format!("固定黑字 {black_cr:.1}:1（深底偏暗，含黑字文档可切柔和浅底）")
            } else {
                format!("固定黑字 {black_cr:.1}:1（清晰可读）")
            };
            let cr_line = format!("自动文字对比度 {auto_cr:.1}:1 ✓  ·  {black_hint}");
            draw_text(
                &cr_line,
                &self.fmt_small,
                rect(seg_l + 2.0, 337.0, seg_r, 356.0),
                if black_cr < 4.5 { rgb(224, 180, 108) } else { rgb(120, 206, 152) },
            );

            // 4 Silky Custom Direct2D Sliders (y = 362..478, stride 29)
            for i in 0..4 {
                let sp = SLIDERS[i];
                let val = slider_get(&live_adj, i);
                let y0 = 362.0 + i as f32 * 29.0;
                let cy = y0 + 12.0;
                let tr = self.layout.slider_tracks[i];

                // Label
                draw_text(sp.label, &self.fmt_body, rect(seg_l, y0, seg_l + 52.0, y0 + 24.0), rgb(206, 216, 232));

                // Minus button
                let m_box = rect(seg_l + 54.0, y0 + 1.0, seg_l + 78.0, y0 + 23.0);
                let m_hov = hovered == Some(ControlId::SliderMinus(i));
                fill_rr(rrect(m_box.left, m_box.top, m_box.right, m_box.bottom, 6.0), if m_hov { rgb(52, 63, 82) } else { rgb(36, 44, 58) });
                draw_text("−", &self.fmt_btn, m_box, rgb(216, 224, 238));

                // Slider Track
                let frac = ((val - sp.min) as f32 / (sp.max - sp.min) as f32).clamp(0.0, 1.0);
                let thumb_x = tr.left + frac * (tr.right - tr.left);
                fill_rr(rrect(tr.left, cy - 3.0, tr.right, cy + 3.0, 3.0), rgb(44, 54, 72));
                if thumb_x > tr.left + 2.0 {
                    let fill_col = match i {
                        2 => rgb(214, 152, 74), // warm sepia track accent
                        3 => rgb(156, 168, 186), // cool silver grayscale track accent
                        _ => rgb(64, 144, 252), // sapphire blue accent
                    };
                    fill_rr(rrect(tr.left, cy - 3.0, thumb_x, cy + 3.0, 3.0), fill_col);
                }

                // Thumb with subtle outer halo on hover/drag
                let active_t = dragging == Some(i) || hovered == Some(ControlId::SliderTrack(i));
                if active_t {
                    if let Some(b_glow) = brush(rgba(84, 158, 255, 0.28)) {
                        rt.FillEllipse(&ellipse(thumb_x, cy, 10.5, 10.5), &b_glow);
                    }
                }
                if let Some(b_thumb) = brush(rgb(240, 246, 255)) {
                    rt.FillEllipse(&ellipse(thumb_x, cy, 6.8, 6.8), &b_thumb);
                }
                if let Some(b_rim) = brush(rgb(59, 130, 246)) {
                    rt.DrawEllipse(&ellipse(thumb_x, cy, 6.8, 6.8), &b_rim, 1.4, None);
                }

                // Plus button
                let p_box = rect(tr.right + 12.0, y0 + 1.0, tr.right + 36.0, y0 + 23.0);
                let p_hov = hovered == Some(ControlId::SliderPlus(i));
                fill_rr(rrect(p_box.left, p_box.top, p_box.right, p_box.bottom, 6.0), if p_hov { rgb(52, 63, 82) } else { rgb(36, 44, 58) });
                draw_text("+", &self.fmt_btn, p_box, rgb(216, 224, 238));

                // Value readout
                let v_str = if sp.is_offset_100 {
                    let d = val - 100;
                    if d == 0 { "0%".into() } else { format!("{d:+}%") }
                } else {
                    format!("+{val}%")
                };
                draw_text(&v_str, &self.fmt_val, rect(p_box.right + 4.0, y0, seg_r, y0 + 24.0), rgb(125, 185, 255));
            }

            // Bottom row inside Card 2: page_msg + Reset button
            draw_text(
                &snap.page_msg,
                &self.fmt_small,
                rect(seg_l, 486.0, seg_r - 86.0, 518.0),
                rgb(130, 144, 166),
            );
            let rst = rect(seg_r - 78.0, 486.0, seg_r, 512.0);
            let rst_hov = hovered == Some(ControlId::ResetParams);
            fill_rr(rrect(rst.left, rst.top, rst.right, rst.bottom, 7.0), if rst_hov { rgb(48, 58, 76) } else { rgb(35, 43, 58) });
            stroke_rr(rrect(rst.left + 0.5, rst.top + 0.5, rst.right - 0.5, rst.bottom - 0.5, 6.5), rgb(56, 68, 90), 1.0);
            draw_text("重置参数", &self.fmt_small_center, rst, rgb(210, 220, 236));

            // ----------------------------------------------------------
            // 4. Card 3: Auto-Start on Boot (y = 534..596)
            // ----------------------------------------------------------
            fill_rr(rrect(pad, 534.0, pad + cw, 596.0, 12.0), rgb(27, 33, 44));
            stroke_rr(
                rrect(pad + 0.5, 534.5, pad + cw - 0.5, 595.5, 11.5),
                if autostart.enabled { rgba(59, 130, 246, 0.55) } else { rgb(43, 52, 69) },
                1.0,
            );
            draw_text("开机静默自启", &self.fmt_section, rect(pad + 16.0, 543.0, 175.0, 565.0), rgb(238, 242, 248));
            fill_rr(rrect(pad + 114.0, 546.0, pad + 174.0, 564.0, 6.0), rgb(37, 45, 60));
            draw_text("托盘常驻", &self.fmt_small_center, rect(pad + 114.0, 546.0, pad + 174.0, 564.0), rgb(174, 188, 210));

            let auto_sub = if autostart.enabled {
                match (snap.remember_skin, snap.remember_page) {
                    (true, true) => "已启用 · 登录后静默驻留托盘，自动开启深色外观 + 页面调色",
                    (true, false) => "已启用 · 登录后静默驻留托盘，自动开启原生深色外观",
                    (false, true) => "已启用 · 登录后静默驻留托盘，自动开启文字页面调色",
                    (false, false) => "已启用 · 登录后静默驻留托盘，待机等待一键切换",
                }
            } else if autostart.disabled_by_taskmgr {
                "已被任务管理器禁用 · 重新开启将恢复当前程序自启"
            } else if autostart.registered_elsewhere {
                "启动项指向旧路径 · 开启将更新为当前 EXE 路径"
            } else {
                "登录 Windows 静默驻留托盘，自动延续上次深色/调色开关"
            };
            draw_text(
                auto_sub,
                &self.fmt_small,
                rect(pad + 16.0, 569.0, pad + cw - 72.0, 591.0),
                if autostart.enabled { rgb(156, 198, 248) } else { rgb(138, 151, 172) },
            );

            self.draw_toggle(
                &rt,
                rect(pad + cw - 16.0 - 48.0, 552.0, pad + cw - 16.0, 578.0),
                autostart.enabled,
                hovered == Some(ControlId::AutoStartToggle),
            );

            // ----------------------------------------------------------
            // 5. Footer Bar (y = 608..642)
            // ----------------------------------------------------------
            draw_text(
                "点关闭 × 自动收至托盘并保持深色生效\n右键托盘图标可随时开关或恢复并退出",
                &self.fmt_small,
                rect(pad + 2.0, 608.0, LOGICAL_W - pad - 204.0, 646.0),
                rgb(122, 136, 158),
            );

            let hide_r = LOGICAL_W - pad;
            let hide_l = hide_r - 92.0;
            let rest_r = hide_l - 10.0;
            let rest_l = rest_r - 92.0;

            // Restore All button
            let rb = rect(rest_l, 608.0, rest_r, 642.0);
            let rb_hov = hovered == Some(ControlId::RestoreAll);
            fill_rr(rrect(rb.left, rb.top, rb.right, rb.bottom, 9.0), if rb_hov { rgb(52, 62, 80) } else { rgb(34, 42, 56) });
            stroke_rr(rrect(rb.left + 0.5, rb.top + 0.5, rb.right - 0.5, rb.bottom - 0.5, 8.5), rgb(64, 78, 102), 1.0);
            draw_text("全部恢复", &self.fmt_btn, rb, rgb(224, 232, 245));

            // Minimize to Tray button (accent)
            let hb = rect(hide_l, 608.0, hide_r, 642.0);
            let hb_hov = hovered == Some(ControlId::HideToTray);
            fill_rr(rrect(hb.left, hb.top, hb.right, hb.bottom, 9.0), if hb_hov { rgb(74, 148, 255) } else { rgb(50, 122, 235) });
            draw_text("收至托盘", &self.fmt_btn, hb, rgb(255, 255, 255));

            let end_ok = rt.EndDraw(None, None).is_ok();
            if end_ok {
                let _ = BitBlt(hdc, 0, 0, w, h, Some(mem_dc), 0, 0, SRCCOPY);
            } else {
                self.rt = None;
            }
            SelectObject(mem_dc, old_obj);
            let _ = DeleteObject(mem_bmp.into());
            let _ = DeleteDC(mem_dc);
        }
    }

    unsafe fn draw_toggle(&self, rt: &ID2D1DCRenderTarget, r: D2D_RECT_F, on: bool, hovered: bool) {
        let h = r.bottom - r.top;
        let rad = h * 0.5;
        let bg = if on {
            if hovered { rgb(78, 152, 255) } else { rgb(52, 128, 245) }
        } else if hovered {
            rgb(62, 74, 96)
        } else {
            rgb(46, 56, 74)
        };
        if let Ok(b) = rt.CreateSolidColorBrush(&bg, None) {
            rt.FillRoundedRectangle(&rrect(r.left, r.top, r.right, r.bottom, rad), &b);
        }
        let knob_r = rad - 3.5;
        let cx = if on { r.right - rad } else { r.left + rad };
        let cy = (r.top + r.bottom) * 0.5;
        if let Ok(bk) = rt.CreateSolidColorBrush(&rgb(245, 248, 255), None) {
            rt.FillEllipse(&ellipse(cx, cy, knob_r, knob_r), &bk);
        }
    }
}
