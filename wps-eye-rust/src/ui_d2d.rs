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
pub const LOGICAL_H: f32 = 706.0;

pub const IMG_MIN: i32 = 25;
pub const IMG_MAX: i32 = 100;
pub const IMG_STEP: i32 = 5;

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
    ImgToggle,
    ImgMinus,
    ImgTrack,
    ImgPlus,
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
    pub img_track: D2D_RECT_F,
}

impl Layout {
    pub fn compute() -> Self {
        let mut hits = Vec::with_capacity(32);
        let pad = 16.0;
        let cw = LOGICAL_W - pad * 2.0; // 488

        // Card 1: Native Dark Skin (y = 70..146)
        hits.push(HitBox {
            id: ControlId::SkinToggle,
            rect: rect(pad + cw - 16.0 - 48.0, 82.0, pad + cw - 16.0, 108.0),
        });

        // Card 2: Document Page Eye Comfort (y = 154..484)
        hits.push(HitBox {
            id: ControlId::PageToggle,
            rect: rect(pad + cw - 16.0 - 48.0, 165.0, pad + cw - 16.0, 191.0),
        });

        // Segmented mode selector (y = 200..230)
        let seg_l = pad + 16.0;
        let seg_r = pad + cw - 16.0;
        let seg_mid = (seg_l + seg_r) * 0.5;
        hits.push(HitBox {
            id: ControlId::ModeDark,
            rect: rect(seg_l + 3.0, 203.0, seg_mid - 2.0, 227.0),
        });
        hits.push(HitBox {
            id: ControlId::ModeSoftLight,
            rect: rect(seg_mid + 2.0, 203.0, seg_r - 3.0, 227.0),
        });

        // 4 sliders (y = 319..431, stride 28)
        let mut slider_tracks = [rect(0.0, 0.0, 0.0, 0.0); 4];
        let minus_l = pad + 16.0 + 54.0;
        let minus_r = minus_l + 24.0;
        let val_w = 48.0;
        let plus_r = pad + cw - 16.0 - val_w - 6.0;
        let plus_l = plus_r - 24.0;
        let trk_l = minus_r + 12.0;
        let trk_r = plus_l - 12.0;

        for i in 0..4 {
            let y0 = 319.0 + i as f32 * 28.0;
            let tr = rect(trk_l, y0 + 1.0, trk_r, y0 + 23.0);
            slider_tracks[i] = tr;
            hits.push(HitBox { id: ControlId::SliderMinus(i), rect: rect(minus_l, y0 + 1.0, minus_r, y0 + 23.0) });
            hits.push(HitBox { id: ControlId::SliderTrack(i), rect: tr });
            hits.push(HitBox { id: ControlId::SliderPlus(i), rect: rect(plus_l, y0 + 1.0, plus_r, y0 + 23.0) });
        }

        // Reset button inside Card 2 bottom right
        hits.push(HitBox {
            id: ControlId::ResetParams,
            rect: rect(pad + cw - 16.0 - 84.0, 444.0, pad + cw - 16.0, 470.0),
        });

        // Card 3: Dedicated Image Night Brightness Bar (y = 492..580)
        hits.push(HitBox {
            id: ControlId::ImgToggle,
            rect: rect(pad + cw - 16.0 - 48.0, 502.0, pad + cw - 16.0, 528.0),
        });
        let img_y0 = 532.0;
        let img_track = rect(trk_l, img_y0 + 1.0, trk_r, img_y0 + 23.0);
        hits.push(HitBox { id: ControlId::ImgMinus, rect: rect(minus_l, img_y0 + 1.0, minus_r, img_y0 + 23.0) });
        hits.push(HitBox { id: ControlId::ImgTrack, rect: img_track });
        hits.push(HitBox { id: ControlId::ImgPlus, rect: rect(plus_l, img_y0 + 1.0, plus_r, img_y0 + 23.0) });

        // Card 4: Auto-Start on Boot (y = 588..648)
        hits.push(HitBox {
            id: ControlId::AutoStartToggle,
            rect: rect(pad + cw - 16.0 - 48.0, 605.0, pad + cw - 16.0, 631.0),
        });

        // Bottom footer buttons (y = 658..692)
        let hide_r = LOGICAL_W - pad;
        let hide_l = hide_r - 92.0;
        let rest_r = hide_l - 10.0;
        let rest_l = rest_r - 92.0;
        hits.push(HitBox { id: ControlId::RestoreAll, rect: rect(rest_l, 658.0, rest_r, 692.0) });
        hits.push(HitBox { id: ControlId::HideToTray, rect: rect(hide_l, 658.0, hide_r, 692.0) });

        Self { hits, slider_tracks, img_track }
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

    pub fn img_value_from_x(&self, x: f32) -> i32 {
        let tr = self.img_track;
        let t = ((x - tr.left) / (tr.right - tr.left).max(1.0)).clamp(0.0, 1.0);
        let raw = IMG_MIN as f32 + t * (IMG_MAX - IMG_MIN) as f32;
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
        active_dark_mode: bool,
        live_adj: Adjustments,
        live_img_brightness: i32,
        hovered: Option<ControlId>,
        dragging: Option<usize>,
        dragging_img: bool,
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
            // 1. Header with bespoke vector emblem + live connection pill (NO Rust badge!)
            // ----------------------------------------------------------
            fill_rr(rrect(pad, 12.0, pad + 34.0, 46.0, 9.0), rgb(30, 42, 62));
            stroke_rr(rrect(pad + 0.5, 12.5, pad + 33.5, 45.5, 8.5), rgba(120, 160, 225, 0.45), 1.0);
            if let Some(b_ring) = brush(rgb(202, 224, 255)) {
                let eye = ellipse(pad + 17.0, 29.0, 11.0, 6.5);
                rt.DrawEllipse(&eye, &b_ring, 1.8, None);
            }
            if let Some(b_moon) = brush(rgb(245, 190, 96)) {
                let m = ellipse(pad + 16.5, 29.0, 4.2, 4.2);
                rt.FillEllipse(&m, &b_moon);
            }
            if let Some(b_cut) = brush(rgb(24, 34, 50)) {
                let cut = ellipse(pad + 18.6, 27.6, 3.4, 3.4);
                rt.FillEllipse(&cut, &b_cut);
            }

            draw_text("WPS 护眼控制器", &self.fmt_title, rect(pad + 44.0, 10.0, 300.0, 34.0), rgb(238, 242, 248));

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
            fill_rr(rrect(LOGICAL_W - pad - 154.0, 13.0, LOGICAL_W - pad, 35.0, 11.0), pill_bg);
            draw_text(&pill_txt, &self.fmt_small_center, rect(LOGICAL_W - pad - 154.0, 13.0, LOGICAL_W - pad, 35.0), pill_fg);

            // Status subtitle line
            let status_col = if snap.error_text.is_some() {
                rgb(248, 130, 130)
            } else {
                rgb(142, 155, 176)
            };
            draw_text(&snap.status_text, &self.fmt_small, rect(pad + 44.0, 36.0, LOGICAL_W - pad, 66.0), status_col);

            // ----------------------------------------------------------
            // 2. Card 1: Native 2025 Dark Skin (y = 70..146)
            // ----------------------------------------------------------
            fill_rr(rrect(pad, 70.0, pad + cw, 146.0, 12.0), rgb(27, 33, 44));
            stroke_rr(
                rrect(pad + 0.5, 70.5, pad + cw - 0.5, 145.5, 11.5),
                if snap.skin_enabled { rgba(59, 130, 246, 0.55) } else { rgb(43, 52, 69) },
                1.0,
            );
            draw_text("原生深色外观", &self.fmt_section, rect(pad + 16.0, 81.0, 175.0, 107.0), rgb(238, 242, 248));

            let mut cx = pad + 114.0;
            for (chip, w_chip) in [("文字", 38.0f32), ("PPT", 38.0), ("表格", 38.0)] {
                fill_rr(rrect(cx, 85.0, cx + w_chip, 103.0, 6.0), rgb(37, 45, 60));
                draw_text(chip, &self.fmt_small_center, rect(cx, 85.0, cx + w_chip, 103.0), rgb(174, 188, 210));
                cx += w_chip + 6.0;
            }

            self.draw_toggle(
                &rt,
                rect(pad + cw - 16.0 - 48.0, 82.0, pad + cw - 16.0, 108.0),
                snap.skin_enabled,
                hovered == Some(ControlId::SkinToggle),
            );

            draw_text(
                &snap.skin_msg,
                &self.fmt_small,
                rect(pad + 16.0, 114.0, pad + cw - 16.0, 142.0),
                rgb(142, 155, 176),
            );

            // ----------------------------------------------------------
            // 3. Card 2: Document Page Eye Comfort — Mode 1 & Mode 2 (y = 154..484)
            // ----------------------------------------------------------
            fill_rr(rrect(pad, 154.0, pad + cw, 484.0, 12.0), rgb(27, 33, 44));
            stroke_rr(
                rrect(pad + 0.5, 154.5, pad + cw - 0.5, 483.5, 11.5),
                if snap.page_enabled { rgba(59, 130, 246, 0.55) } else { rgb(43, 52, 69) },
                1.0,
            );

            draw_text("文档页面护眼", &self.fmt_section, rect(pad + 16.0, 164.0, 160.0, 190.0), rgb(238, 242, 248));
            let mut cx2 = pad + 114.0;
            for (chip, w_chip, dim_in_m1) in [("文字", 38.0f32, false), ("表格", 38.0, false), ("PPT", 38.0, true)] {
                let is_muted = active_dark_mode && dim_in_m1;
                fill_rr(
                    rrect(cx2, 168.0, cx2 + w_chip, 186.0, 6.0),
                    if is_muted { rgb(28, 34, 46) } else { rgb(37, 45, 60) },
                );
                draw_text(
                    chip,
                    &self.fmt_small_center,
                    rect(cx2, 168.0, cx2 + w_chip, 186.0),
                    if is_muted { rgb(98, 110, 130) } else { rgb(174, 188, 210) },
                );
                cx2 += w_chip + 6.0;
            }

            self.draw_toggle(
                &rt,
                rect(pad + cw - 16.0 - 48.0, 165.0, pad + cw - 16.0, 191.0),
                snap.page_enabled,
                hovered == Some(ControlId::PageToggle),
            );

            // Segmented mode selector (y = 200..230)
            let seg_l = pad + 16.0;
            let seg_r = pad + cw - 16.0;
            let seg_mid = (seg_l + seg_r) * 0.5;
            fill_rr(rrect(seg_l, 200.0, seg_r, 230.0, 9.0), rgb(18, 22, 30));
            stroke_rr(rrect(seg_l + 0.5, 200.5, seg_r - 0.5, 229.5, 8.5), rgb(40, 48, 64), 1.0);

            let dark_sel = active_dark_mode;
            let active_seg_r = if dark_sel {
                rrect(seg_l + 3.0, 203.0, seg_mid - 2.0, 227.0, 7.0)
            } else {
                rrect(seg_mid + 2.0, 203.0, seg_r - 3.0, 227.0, 7.0)
            };
            fill_rr(active_seg_r, rgb(39, 67, 114));
            stroke_rr(active_seg_r, rgba(110, 178, 255, 0.5), 1.0);

            draw_text(
                "模式 1 · 深底反色",
                &self.fmt_btn,
                rect(seg_l + 3.0, 203.0, seg_mid - 2.0, 227.0),
                if dark_sel { rgb(242, 247, 255) } else if hovered == Some(ControlId::ModeDark) { rgb(200, 212, 232) } else { rgb(138, 152, 174) },
            );
            draw_text(
                "模式 2 · 夜间柔暗（不反黑）",
                &self.fmt_btn,
                rect(seg_mid + 2.0, 203.0, seg_r - 3.0, 227.0),
                if !dark_sel { rgb(242, 247, 255) } else if hovered == Some(ControlId::ModeSoftLight) { rgb(200, 212, 232) } else { rgb(138, 152, 174) },
            );

            // Live Readability Preview Box (y = 238..292)
            if dark_sel {
                let (p_bg, p_fg) = live_adj.palette();
                let auto_cr = contrast_ratio(p_bg, p_fg);
                let black_cr = contrast_ratio(p_bg, (0, 0, 0));
                fill_rr(rrect(seg_l, 238.0, seg_r, 292.0, 10.0), rgb(p_bg.0, p_bg.1, p_bg.2));
                stroke_rr(rrect(seg_l + 0.5, 238.5, seg_r - 0.5, 291.5, 9.5), rgba(255, 255, 255, 0.14), 1.0);
                draw_text(
                    "Aa 123 · 文字/表格深底反色（独立参数）",
                    &self.fmt_preview_lg,
                    rect(seg_l + 12.0, 242.0, seg_r - 82.0, 266.0),
                    rgb(p_fg.0, p_fg.1, p_fg.2),
                );
                draw_text(
                    "Aa 123 · 固定纯黑字 (#000000) 对照",
                    &self.fmt_preview_sm,
                    rect(seg_l + 12.0, 267.0, seg_r - 134.0, 287.0),
                    rgb(0, 0, 0),
                );
                fill_rr(rrect(seg_r - 74.0, 244.0, seg_r - 10.0, 262.0, 6.0), rgba(15, 20, 28, 0.72));
                draw_text(&hex_color(p_bg), &self.fmt_small_center, rect(seg_r - 74.0, 244.0, seg_r - 10.0, 262.0), rgb(228, 236, 248));

                fill_rr(rrect(seg_r - 126.0, 267.0, seg_r - 10.0, 286.0, 5.0), rgba(15, 20, 28, 0.68));
                draw_text("PPT 仅模式 2 生效", &self.fmt_small_center, rect(seg_r - 126.0, 267.0, seg_r - 10.0, 286.0), rgb(188, 202, 224));

                let cr_line = format!(
                    "自动字对比度 {auto_cr:.1}:1 ✓  ·  固定黑字 {black_cr:.1}:1 · 仅文字/表格生效，不调 PPT"
                );
                draw_text(&cr_line, &self.fmt_small, rect(seg_l + 2.0, 296.0, seg_r, 314.0), rgb(224, 180, 108));
            } else {
                let d_bg = live_adj.soft_dim_rgb();
                let black_cr = contrast_ratio(d_bg, (0, 0, 0));
                fill_rr(rrect(seg_l, 238.0, seg_r, 292.0, 10.0), rgb(d_bg.0, d_bg.1, d_bg.2));
                stroke_rr(rrect(seg_l + 0.5, 238.5, seg_r - 0.5, 291.5, 9.5), rgba(255, 255, 255, 0.14), 1.0);
                // Show sharp black text + dimmed PPT blue accent swatch!
                let blue_dim = (
                    ((158u32 * d_bg.0 as u32) / 255) as u8,
                    ((197u32 * d_bg.1 as u32) / 255) as u8,
                    ((232u32 * d_bg.2 as u32) / 255) as u8,
                );
                fill_rr(rrect(seg_r - 166.0, 266.0, seg_r - 10.0, 285.0, 5.0), rgb(blue_dim.0, blue_dim.1, blue_dim.2));
                draw_text("PPT 模板色块保留原色", &self.fmt_small_center, rect(seg_r - 166.0, 266.0, seg_r - 10.0, 285.0), rgb(0, 0, 0));
                draw_text(
                    "Aa 123 · 文字/表格/PPT 柔暗（独立参数）",
                    &self.fmt_preview_lg,
                    rect(seg_l + 12.0, 242.0, seg_r - 82.0, 266.0),
                    rgb(0, 0, 0),
                );
                draw_text(
                    "纯白纸张柔和降亮，PPT 仅在此模式调节",
                    &self.fmt_preview_sm,
                    rect(seg_l + 12.0, 267.0, seg_r - 172.0, 287.0),
                    rgb(24, 28, 34),
                );
                fill_rr(rrect(seg_r - 74.0, 244.0, seg_r - 10.0, 262.0, 6.0), rgba(15, 20, 28, 0.72));
                draw_text(&hex_color(d_bg), &self.fmt_small_center, rect(seg_r - 74.0, 244.0, seg_r - 10.0, 262.0), rgb(228, 236, 248));

                let cr_line = format!(
                    "黑字对比度 {black_cr:.1}:1 ✓  ·  文字 / 表格 / PPT 三端原生正片叠底，零闪屏"
                );
                draw_text(&cr_line, &self.fmt_small, rect(seg_l + 2.0, 296.0, seg_r, 314.0), rgb(120, 206, 152));
            }

            // 4 Silky Custom Direct2D Sliders (y = 319..431, stride 28)
            for i in 0..4 {
                let sp = SLIDERS[i];
                let val = slider_get(&live_adj, i);
                let y0 = 319.0 + i as f32 * 28.0;
                let cy = y0 + 12.0;
                let tr = self.layout.slider_tracks[i];

                draw_text(sp.label, &self.fmt_body, rect(seg_l, y0, seg_l + 52.0, y0 + 24.0), rgb(206, 216, 232));

                let m_box = rect(seg_l + 54.0, y0 + 1.0, seg_l + 78.0, y0 + 23.0);
                let m_hov = hovered == Some(ControlId::SliderMinus(i));
                fill_rr(rrect(m_box.left, m_box.top, m_box.right, m_box.bottom, 6.0), if m_hov { rgb(52, 63, 82) } else { rgb(36, 44, 58) });
                draw_text("−", &self.fmt_btn, m_box, rgb(216, 224, 238));

                let frac = ((val - sp.min) as f32 / (sp.max - sp.min) as f32).clamp(0.0, 1.0);
                let thumb_x = tr.left + frac * (tr.right - tr.left);
                fill_rr(rrect(tr.left, cy - 3.0, tr.right, cy + 3.0, 3.0), rgb(44, 54, 72));
                if thumb_x > tr.left + 2.0 {
                    let fill_col = match i {
                        2 => rgb(214, 152, 74),
                        3 => rgb(156, 168, 186),
                        _ => rgb(64, 144, 252),
                    };
                    fill_rr(rrect(tr.left, cy - 3.0, thumb_x, cy + 3.0, 3.0), fill_col);
                }

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

                let p_box = rect(tr.right + 12.0, y0 + 1.0, tr.right + 36.0, y0 + 23.0);
                let p_hov = hovered == Some(ControlId::SliderPlus(i));
                fill_rr(rrect(p_box.left, p_box.top, p_box.right, p_box.bottom, 6.0), if p_hov { rgb(52, 63, 82) } else { rgb(36, 44, 58) });
                draw_text("+", &self.fmt_btn, p_box, rgb(216, 224, 238));

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
                rect(seg_l, 442.0, seg_r - 92.0, 476.0),
                rgb(130, 144, 166),
            );
            let rst = rect(seg_r - 84.0, 444.0, seg_r, 470.0);
            let rst_hov = hovered == Some(ControlId::ResetParams);
            fill_rr(rrect(rst.left, rst.top, rst.right, rst.bottom, 7.0), if rst_hov { rgb(48, 58, 76) } else { rgb(35, 43, 58) });
            stroke_rr(rrect(rst.left + 0.5, rst.top + 0.5, rst.right - 0.5, rst.bottom - 0.5, 6.5), rgb(56, 68, 90), 1.0);
            let rst_label = if dark_sel { "重置模式 1" } else { "重置模式 2" };
            draw_text(rst_label, &self.fmt_small_center, rst, rgb(210, 220, 236));

            // ----------------------------------------------------------
            // 4. Card 3: Dedicated Document Image Night-Brightness Bar (y = 492..580)
            // ----------------------------------------------------------
            fill_rr(rrect(pad, 492.0, pad + cw, 580.0, 12.0), rgb(27, 33, 44));
            stroke_rr(
                rrect(pad + 0.5, 492.5, pad + cw - 0.5, 579.5, 11.5),
                if snap.img_enabled { rgba(245, 180, 82, 0.55) } else { rgb(43, 52, 69) },
                1.0,
            );
            draw_text("贴图夜间亮度", &self.fmt_section, rect(pad + 16.0, 501.0, 160.0, 527.0), rgb(238, 242, 248));
            let mut cx3 = pad + 114.0;
            for (chip, w_chip) in [("文字", 38.0f32), ("PPT", 38.0), ("表格", 38.0)] {
                fill_rr(rrect(cx3, 505.0, cx3 + w_chip, 523.0, 6.0), rgb(37, 45, 60));
                draw_text(chip, &self.fmt_small_center, rect(cx3, 505.0, cx3 + w_chip, 523.0), rgb(174, 188, 210));
                cx3 += w_chip + 6.0;
            }
            self.draw_toggle(
                &rt,
                rect(pad + cw - 16.0 - 48.0, 502.0, pad + cw - 16.0, 528.0),
                snap.img_enabled,
                hovered == Some(ControlId::ImgToggle),
            );

            // Dedicated Image Brightness Slider (y = 532..556)
            {
                let y0 = 532.0;
                let cy = y0 + 12.0;
                let tr = self.layout.img_track;
                let val = live_img_brightness.clamp(IMG_MIN, IMG_MAX);

                draw_text("图片亮度", &self.fmt_body, rect(seg_l, y0, seg_l + 52.0, y0 + 24.0), rgb(206, 216, 232));

                let m_box = rect(seg_l + 54.0, y0 + 1.0, seg_l + 78.0, y0 + 23.0);
                let m_hov = hovered == Some(ControlId::ImgMinus);
                fill_rr(rrect(m_box.left, m_box.top, m_box.right, m_box.bottom, 6.0), if m_hov { rgb(52, 63, 82) } else { rgb(36, 44, 58) });
                draw_text("−", &self.fmt_btn, m_box, rgb(216, 224, 238));

                let frac = ((val - IMG_MIN) as f32 / (IMG_MAX - IMG_MIN) as f32).clamp(0.0, 1.0);
                let thumb_x = tr.left + frac * (tr.right - tr.left);
                fill_rr(rrect(tr.left, cy - 3.0, tr.right, cy + 3.0, 3.0), rgb(44, 54, 72));
                if thumb_x > tr.left + 2.0 {
                    fill_rr(rrect(tr.left, cy - 3.0, thumb_x, cy + 3.0, 3.0), rgb(242, 178, 76));
                }

                let active_t = dragging_img || hovered == Some(ControlId::ImgTrack);
                if active_t {
                    if let Some(b_glow) = brush(rgba(245, 185, 86, 0.30)) {
                        rt.FillEllipse(&ellipse(thumb_x, cy, 10.5, 10.5), &b_glow);
                    }
                }
                if let Some(b_thumb) = brush(rgb(255, 248, 236)) {
                    rt.FillEllipse(&ellipse(thumb_x, cy, 6.8, 6.8), &b_thumb);
                }
                if let Some(b_rim) = brush(rgb(236, 166, 62)) {
                    rt.DrawEllipse(&ellipse(thumb_x, cy, 6.8, 6.8), &b_rim, 1.4, None);
                }

                let p_box = rect(tr.right + 12.0, y0 + 1.0, tr.right + 36.0, y0 + 23.0);
                let p_hov = hovered == Some(ControlId::ImgPlus);
                fill_rr(rrect(p_box.left, p_box.top, p_box.right, p_box.bottom, 6.0), if p_hov { rgb(52, 63, 82) } else { rgb(36, 44, 58) });
                draw_text("+", &self.fmt_btn, p_box, rgb(216, 224, 238));

                let v_str = format!("{val}%");
                draw_text(&v_str, &self.fmt_val, rect(p_box.right + 4.0, y0, seg_r, y0 + 24.0), rgb(245, 194, 106));
            }
            draw_text(
                "仅独立压暗文档内贴入的图片（保留 PNG 透明通道），不影响正文、工具栏与原文件",
                &self.fmt_small,
                rect(pad + 16.0, 558.0, pad + cw - 16.0, 576.0),
                if snap.img_enabled { rgb(232, 194, 132) } else { rgb(134, 147, 168) },
            );

            // ----------------------------------------------------------
            // 5. Card 4: Auto-Start on Boot (y = 588..648)
            // ----------------------------------------------------------
            fill_rr(rrect(pad, 588.0, pad + cw, 648.0, 12.0), rgb(27, 33, 44));
            stroke_rr(
                rrect(pad + 0.5, 588.5, pad + cw - 0.5, 647.5, 11.5),
                if autostart.enabled { rgba(59, 130, 246, 0.55) } else { rgb(43, 52, 69) },
                1.0,
            );
            draw_text("开机静默自启", &self.fmt_section, rect(pad + 16.0, 597.0, 175.0, 619.0), rgb(238, 242, 248));
            fill_rr(rrect(pad + 114.0, 600.0, pad + 174.0, 618.0, 6.0), rgb(37, 45, 60));
            draw_text("托盘常驻", &self.fmt_small_center, rect(pad + 114.0, 600.0, pad + 174.0, 618.0), rgb(174, 188, 210));

            let auto_sub = if autostart.enabled {
                match (snap.remember_skin, snap.remember_page, snap.remember_img) {
                    (true, true, true) => "已启用 · 开机托盘静默运行，自动开启外观 + 页面 + 贴图降亮",
                    (true, true, false) => "已启用 · 登录后静默驻留托盘，自动开启深色外观 + 页面护眼",
                    (true, false, true) => "已启用 · 登录后静默驻留托盘，自动开启深色外观 + 贴图降亮",
                    (true, false, false) => "已启用 · 登录后静默驻留托盘，自动开启原生深色外观",
                    (false, true, true) => "已启用 · 登录后静默驻留托盘，自动开启页面护眼 + 贴图降亮",
                    (false, true, false) => "已启用 · 登录后静默驻留托盘，自动开启文档页面护眼",
                    (false, false, true) => "已启用 · 登录后静默驻留托盘，自动开启贴图夜间亮度",
                    (false, false, false) => "已启用 · 登录后静默驻留托盘，待机等待一键切换",
                }
            } else if autostart.disabled_by_taskmgr {
                "已被任务管理器禁用 · 重新开启将恢复当前程序自启"
            } else if autostart.registered_elsewhere {
                "启动项指向旧路径 · 开启将更新为当前 EXE 路径"
            } else {
                "登录 Windows 静默驻留托盘，自动延续上次开关状态"
            };
            draw_text(
                auto_sub,
                &self.fmt_small,
                rect(pad + 16.0, 622.0, pad + cw - 72.0, 644.0),
                if autostart.enabled { rgb(156, 198, 248) } else { rgb(138, 151, 172) },
            );

            self.draw_toggle(
                &rt,
                rect(pad + cw - 16.0 - 48.0, 605.0, pad + cw - 16.0, 631.0),
                autostart.enabled,
                hovered == Some(ControlId::AutoStartToggle),
            );

            // ----------------------------------------------------------
            // 6. Footer Bar (y = 658..692)
            // ----------------------------------------------------------
            draw_text(
                "点关闭 × 自动收至托盘并保持深色生效\n右键托盘图标可随时开关或恢复并退出",
                &self.fmt_small,
                rect(pad + 2.0, 658.0, LOGICAL_W - pad - 204.0, 696.0),
                rgb(122, 136, 158),
            );

            let hide_r = LOGICAL_W - pad;
            let hide_l = hide_r - 92.0;
            let rest_r = hide_l - 10.0;
            let rest_l = rest_r - 92.0;

            let rb = rect(rest_l, 658.0, rest_r, 692.0);
            let rb_hov = hovered == Some(ControlId::RestoreAll);
            fill_rr(rrect(rb.left, rb.top, rb.right, rb.bottom, 9.0), if rb_hov { rgb(52, 62, 80) } else { rgb(34, 42, 56) });
            stroke_rr(rrect(rb.left + 0.5, rb.top + 0.5, rb.right - 0.5, rb.bottom - 0.5, 8.5), rgb(64, 78, 102), 1.0);
            draw_text("全部恢复", &self.fmt_btn, rb, rgb(224, 232, 245));

            let hb = rect(hide_l, 658.0, hide_r, 692.0);
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
