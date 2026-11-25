//! Pure-Rust 2025-base native dark .kuip + Qt RCC v2 SVG skin builder.
//!
//! Reads WPS `office6/res/themes/*.kuip` and `office6/skins/2019dark/default/prometheus_wpsoffice_res.rcc`
//! strictly read-only and generates `%LOCALAPPDATA%\WpsNativeEye\skins\2018white_dark\`.

use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use quick_xml::events::{BytesEnd, BytesStart, Event};
use quick_xml::{Reader, Writer};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

pub const DARK_SKIN_NAME: &str = "2018white_dark";
pub const LIGHT_SKIN_FALLBACK: &str = "2018white";
const BUILDER_VERSION: u32 = 3;

const SKIN_INI: &str = "[common]\r\nxmlFile=ongmani\r\nstyle=KWPSStyle_2018\r\nrccFile=prometheus_kso_res.rcc\r\nbaseSkin=2018white\r\n\r\n[wpsoffice]\r\nstyle=KPromeStyle\r\nrccFile=prometheus_wpsoffice_res.rcc\r\n";

fn palette_override(name: &str) -> Option<&'static str> {
    Some(match name {
        // Surfaces
        "background-01" => "#1C1F24FF",
        "background-02" => "#23262CFF",
        "background-03" => "#292D34FF",
        "background-04" => "#30353EFF",
        "background-05" => "#292D34FF",
        // Text
        "text-01" => "#ECF0F6FF",
        "text-02" => "#D5DBE5FF",
        "text-03" => "#AAB3C2FF",
        "text-04" => "#808A9CFF",
        "text-05" => "#616B7CFF",
        "text-06" => "#495261FF",
        "texts-01" => "#E0E5EEFF",
        "texts-02" => "#B6BFCFFF",
        "texts-03" => "#FFFFFFFF",
        "texts-04" => "#FFFFFFFF",
        // Icons
        "icon-01" => "#E8ECF3FF",
        "icon-02" => "#D2D8E2FF",
        "icon-03" => "#A6B0C0FF",
        "icon-04" => "#7F899BFF",
        "icon-05" => "#5F697AFF",
        "icon-06" => "#47505FFF",
        "icons-01" => "#E0E5EEFF",
        "icons-02" => "#B4BECDFF",
        "icons-03" => "#FFFFFFFF",
        "icons-04" => "#FFFFFFFF",
        "icons-05" => "#FFFFFF24",
        // Separators, borders & controls
        "separator-01" => "#2E333BFF",
        "separator-02" => "#393F4AFF",
        "separator-03" => "#474E5BFF",
        "separator-04" => "#2E333BFF",
        "scroll-01" => "#FFFFFF26",
        "scroll-02" => "#FFFFFF44",
        "scroll-03" => "#FFFFFF64",
        "ctl-border-01" => "#444B57FF",
        "ctl-border-02" => "#586070FF",
        "ctl-border-03" => "#6D778AFF",
        "ctl-border-04" => "#383E48FF",
        "ctl-background-01" => "#282C33FF",
        "ctl-background-02" => "#31363FFF",
        "ctl-background-03" => "#3A404BFF",
        "ctl-background-04" => "#23262CFF",
        "dlg-border-01" => "#FFFFFF26",
        "dlg-background-01" => "#252930FF",
        "dlg-background-02" => "#1D2025FF",
        "loading-01" => "#FFFFFF24",
        "status-01" => "#FFFFFF00",
        "status-02" => "#FFFFFF18",
        "status-03" => "#FFFFFF2E",
        "status-04" => "#FFFFFF22",
        "status-001" => "#FFFFFF00",
        "status-002" => "#FFFFFF22",
        "status-003" => "#FFFFFF3A",
        "status-004" => "#FFFFFF2C",
        "status01-01" => "#FFFFFF00",
        "status01-02" => "#FFFFFF18",
        "status01-03" => "#FFFFFF2E",
        "status01-04" => "#FFFFFF2E",
        "theme-common-04" => "#243A61FF",
        "theme-common-05" => "#1D2C47FF",
        "theme-wps-04" => "#243D66FF",
        "theme-wps-05" => "#1D2E4AFF",
        "theme-et-04" => "#1C4736FF",
        "theme-et-05" => "#17362AFF",
        "theme-wpp-04" => "#592D1EFF",
        "theme-wpp-05" => "#422218FF",
        "theme-pdf-04" => "#592424FF",
        "theme-pdf-05" => "#421B1BFF",
        "error-04" => "#592424FF",
        "error-05" => "#3F1B1BFF",
        "warning-04" => "#594318FF",
        "warning-05" => "#403012FF",
        "success-04" => "#1D4A28FF",
        "success-05" => "#17361EFF",
        "kd-color-gray-1" => "#1C1F24",
        "kd-color-gray-2" => "#22262C",
        "kd-color-gray-3" => "#2B3038",
        "kd-color-gray-4" => "#363C46",
        "kd-color-gray-5" => "#464D5A",
        "kd-color-gray-6" => "#687182",
        "kd-color-gray-7" => "#98A2B3",
        "kd-color-gray-8" => "#C1C8D5",
        "kd-color-gray-9" => "#E4E8F0",
        "kd-color-gray-10" => "#FFFFFF",
        "kd-color-white" => "#FFFFFF",
        "kd-color-black" => "#000000",
        _ => return None,
    })
}

fn theme_palette_override(name: &str) -> Option<(&'static str, Option<&'static str>)> {
    Some(match name {
        "kd-color-background-top" => ("#2B3038", None),
        "kd-color-background-middle" => ("#24282F", None),
        "kd-color-background-bottom" => ("#1D2025", None),
        "kd-color-background-plate" => ("#1C1F24", None),
        "kd-color-background-base" => ("#20242A", None),
        "kd-color-background-frame" => ("#272B33", None),
        "kd-color-background-ai" => ("#262538", None),
        "kd-color-fill-base" => ("#292E36", None),
        "kd-color-fill-light" => ("#23272E", None),
        "kd-color-fill-regular" => ("#2D323B", None),
        "kd-color-fill-medium" => ("#383E49", None),
        "kd-color-fill-heavy" => ("#474E5C", None),
        "kd-color-fill-extra-heavy" => ("#5B6475", None),
        "kd-color-public-light" => ("#1E314F", None),
        "kd-color-brand-light" => ("#1E314F", None),
        "kd-color-ai-light" => ("#2E224A", None),
        "kd-color-text-public" => ("#70ACFF", None),
        "kd-color-text-brand" => ("#70ACFF", None),
        "kd-color-icon-public" => ("#70ACFF", None),
        "kd-color-icon-brand" => ("#70ACFF", None),
        "kd-color-icon-primary" => ("#E4E8F0", None),
        "kd-color-icon-secondary" => ("#B5BECE", None),
        "kd-color-feedback-regular" => ("#3D4452", None),
        "kd-color-inverse-base" => ("#1D2025", None),
        "kd-color-mask-regular" => ("CommonPalette::kd-color-black", Some("0.55")),
        "kd-color-mask-heavy" => ("CommonPalette::kd-color-black", Some("0.80")),
        "kd-box-shadow-extra-small" => ("CommonPalette::kd-color-black", Some("0.35")),
        "kd-box-shadow-small" => ("CommonPalette::kd-color-black", Some("0.38")),
        "kd-box-shadow-middle" => ("CommonPalette::kd-color-black", Some("0.45")),
        "kd-box-shadow-large" => ("CommonPalette::kd-color-black", Some("0.40")),
        "kd-box-shadow-extra-large" => ("CommonPalette::kd-color-black", Some("0.55")),
        _ => return None,
    })
}

fn is_dark_stop(name: &str) -> Option<u8> {
    let rest = name.strip_prefix("kd-color-")?;
    let (hue, stop) = rest.rsplit_once('-')?;
    let valid_hue = matches!(
        hue,
        "orange" | "yellow" | "verdure" | "green" | "teal" | "azure"
            | "blue" | "navy" | "indigo" | "violet" | "purple" | "red" | "brand"
    );
    if !valid_hue {
        return None;
    }
    match stop {
        "1" => Some(1),
        "2" => Some(2),
        _ => None,
    }
}

fn parse_hex(val: &str) -> Option<(u8, u8, u8, String)> {
    let s = val.trim().strip_prefix('#')?;
    if s.len() != 6 && s.len() != 8 {
        return None;
    }
    let r = u8::from_str_radix(&s[0..2], 16).ok()?;
    let g = u8::from_str_radix(&s[2..4], 16).ok()?;
    let b = u8::from_str_radix(&s[4..6], 16).ok()?;
    let a = if s.len() == 8 { s[6..8].to_string() } else { String::new() };
    if !a.is_empty() && u8::from_str_radix(&a, 16).is_err() {
        return None;
    }
    Some((r, g, b, a))
}

fn rgb_to_hls(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    let maxc = r.max(g).max(b);
    let minc = r.min(g).min(b);
    let l = (minc + maxc) * 0.5;
    if (maxc - minc).abs() < 1e-12 {
        return (0.0, l, 0.0);
    }
    let s = if l <= 0.5 {
        (maxc - minc) / (maxc + minc)
    } else {
        (maxc - minc) / (2.0 - maxc - minc)
    };
    let rc = (maxc - r) / (maxc - minc);
    let gc = (maxc - g) / (maxc - minc);
    let bc = (maxc - b) / (maxc - minc);
    let mut h = if (r - maxc).abs() < 1e-12 {
        bc - gc
    } else if (g - maxc).abs() < 1e-12 {
        2.0 + rc - bc
    } else {
        4.0 + gc - rc
    };
    h = (h / 6.0).rem_euclid(1.0);
    (h, l, s)
}

fn hls_to_rgb(h: f64, l: f64, s: f64) -> (f64, f64, f64) {
    if s.abs() < 1e-12 {
        return (l, l, l);
    }
    let m2 = if l <= 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let m1 = 2.0 * l - m2;
    let v = |hue: f64| -> f64 {
        let hue = hue.rem_euclid(1.0);
        if hue < 1.0 / 6.0 {
            m1 + (m2 - m1) * hue * 6.0
        } else if hue < 0.5 {
            m2
        } else if hue < 2.0 / 3.0 {
            m1 + (m2 - m1) * (2.0 / 3.0 - hue) * 6.0
        } else {
            m1
        }
    };
    (v(h + 1.0 / 3.0), v(h), v(h - 1.0 / 3.0))
}

fn fmt_hex(r: f64, g: f64, b: f64, a: &str) -> String {
    let ri = r.round().clamp(0.0, 255.0) as u8;
    let gi = g.round().clamp(0.0, 255.0) as u8;
    let bi = b.round().clamp(0.0, 255.0) as u8;
    format!("#{ri:02x}{gi:02x}{bi:02x}{a}")
}

fn map_dark_color(val: &str, is_icon: bool, attr_name: &str, style_cls: &str) -> String {
    let Some((r, g, b, a)) = parse_hex(val) else {
        return val.to_string();
    };
    let alpha_u8 = if a.is_empty() {
        0xFF
    } else {
        u8::from_str_radix(&a, 16).unwrap_or(0xFF)
    };
    if !a.is_empty() && alpha_u8 == 0 {
        return val.to_string();
    }
    let attr_lo = attr_name.to_ascii_lowercase();
    if style_cls.contains("CloseButton") && (attr_lo.contains("hover") || attr_lo.contains("down")) {
        return val.to_string();
    }

    let (h, l, s) = rgb_to_hls(r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0);
    let low_alpha = !a.is_empty() && alpha_u8 < 0x88;

    if is_icon {
        if s < 0.22 {
            let new_l = 0.89 - 0.72 * l;
            let (nr, ng, nb) = hls_to_rgb(0.60, new_l, if new_l < 0.4 { 0.05 } else { 0.04 });
            return fmt_hex(nr * 255.0, ng * 255.0, nb * 255.0, &a);
        }
        let new_l = (l + 0.14).clamp(0.58, 0.76);
        let (nr, ng, nb) = hls_to_rgb(h, new_l, (s * 1.05).min(1.0));
        return fmt_hex(nr * 255.0, ng * 255.0, nb * 255.0, &a);
    }

    if low_alpha && s < 0.18 {
        if l < 0.25 {
            let av = ((alpha_u8 as f64) * 1.15).min(0x55 as f64) as u8;
            return format!("#ffffff{av:02x}");
        }
        if l > 0.75 {
            return format!("#ffffff{a}");
        }
    }

    if s < 0.18 {
        let new_l = 0.89 - 0.735 * l;
        let (nr, ng, nb) = hls_to_rgb(0.60, new_l, if new_l < 0.35 { 0.07 } else { 0.04 });
        return fmt_hex(nr * 255.0, ng * 255.0, nb * 255.0, &a);
    }
    if l > 0.78 {
        let new_l = 0.19 + (1.0 - l) * 0.60;
        let (nr, ng, nb) = hls_to_rgb(h, new_l, (s * 0.75).min(0.55));
        return fmt_hex(nr * 255.0, ng * 255.0, nb * 255.0, &a);
    }
    if l < 0.28 {
        let new_l = (0.75 + (0.28 - l) * 0.42).min(0.90);
        let (nr, ng, nb) = hls_to_rgb(h, new_l, (s * 0.70).min(0.50));
        return fmt_hex(nr * 255.0, ng * 255.0, nb * 255.0, &a);
    }
    if l < 0.50 {
        let new_l = (l + 0.14).min(0.66);
        let (nr, ng, nb) = hls_to_rgb(h, new_l, s);
        return fmt_hex(nr * 255.0, ng * 255.0, nb * 255.0, &a);
    }
    val.to_string()
}

#[derive(Clone, Copy)]
enum KuipMode {
    Palette,
    ThemePalette,
    Generic,
}

#[derive(Default)]
struct KuipState {
    cur_style_cls: String,
    skip_style: bool,
    is_icon_style: bool,
    in_style_tp_widget: bool,
    seen_tp_keys: Vec<String>,
}

impl KuipState {
    fn reset_style(&mut self) {
        self.cur_style_cls.clear();
        self.skip_style = false;
        self.is_icon_style = false;
        self.in_style_tp_widget = false;
        self.seen_tp_keys.clear();
    }

    fn rewrite_tag(&mut self, e: &BytesStart, is_empty: bool, mode: KuipMode) -> Result<Event<'static>, String> {
        let tag_bytes = e.name().as_ref().to_vec();
        let tag = String::from_utf8_lossy(&tag_bytes).into_owned();
        let mut attrs: Vec<(String, String)> = Vec::new();
        for a in e.attributes().flatten() {
            let k = String::from_utf8_lossy(a.key.as_ref()).into_owned();
            let v = String::from_utf8_lossy(&a.value).into_owned();
            attrs.push((k, v));
        }

        let get_attr = |attrs: &[(String, String)], key: &str| -> String {
            attrs
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.clone())
                .unwrap_or_default()
        };
        let set_attr = |attrs: &mut Vec<(String, String)>, key: &str, val: String| {
            if let Some((_, v)) = attrs.iter_mut().find(|(k, _)| k == key) {
                *v = val;
            } else {
                attrs.push((key.to_string(), val));
            }
        };

        if tag == "style" {
            let cls = get_attr(&attrs, "class");
            self.cur_style_cls = cls.clone();
            let lo = cls.to_ascii_lowercase();
            self.skip_style = lo.contains("bluescreen") || lo.contains("paper");
            self.is_icon_style = cls.contains("IconColor") || cls.ends_with("_Icon");
            self.in_style_tp_widget = cls == "KxTpStyleFormatWidget";
            self.seen_tp_keys.clear();
        } else {
            match mode {
                KuipMode::Palette => {
                    if tag == "hint" && get_attr(&attrs, "name") == "is-dark-skin" {
                        set_attr(&mut attrs, "value", "1".to_string());
                    } else if tag == "color" {
                        let name = get_attr(&attrs, "name");
                        if let Some(ov) = palette_override(&name) {
                            set_attr(&mut attrs, "value", ov.to_string());
                        } else if let Some(stop) = is_dark_stop(&name) {
                            let val = get_attr(&attrs, "value");
                            if let Some((r, g, b, a)) = parse_hex(&val) {
                                let (h, _l, s) = rgb_to_hls(r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0);
                                let new_l = if stop == 1 { 0.18 } else { 0.25 };
                                let (nr, ng, nb) = hls_to_rgb(h, new_l, (s * 0.75).min(0.55));
                                set_attr(&mut attrs, "value", fmt_hex(nr * 255.0, ng * 255.0, nb * 255.0, &a));
                            }
                        }
                    }
                }
                KuipMode::ThemePalette => {
                    if tag == "color" {
                        let name = get_attr(&attrs, "name");
                        if let Some((val, alpha)) = theme_palette_override(&name) {
                            set_attr(&mut attrs, "value", val.to_string());
                            match alpha {
                                Some(av) => set_attr(&mut attrs, "a", av.to_string()),
                                None => attrs.retain(|(k, _)| k != "a"),
                            }
                        }
                    }
                }
                KuipMode::Generic => {
                    if tag == "color" && !self.skip_style {
                        let name = get_attr(&attrs, "name");
                        let val = get_attr(&attrs, "value");
                        if !val.is_empty() {
                            if self.cur_style_cls == "KxWpsStyleGallery" && name == "item-background" {
                                set_attr(&mut attrs, "value", "#B8C0CC".to_string());
                            } else if self.in_style_tp_widget {
                                self.seen_tp_keys.push(name.clone());
                                let mapped = match name.as_str() {
                                    "panel_background" | "blockBackground" | "editor_background" => {
                                        Some("#B8C0CC")
                                    }
                                    "blockBackground-hover" => Some("#A8B3C2"),
                                    "blockBackground-down" => Some("#9AA7B8"),
                                    _ => None,
                                };
                                if let Some(m) = mapped {
                                    set_attr(&mut attrs, "value", m.to_string());
                                } else if val.starts_with('#') {
                                    let nc = map_dark_color(&val, self.is_icon_style, &name, &self.cur_style_cls);
                                    set_attr(&mut attrs, "value", nc);
                                }
                            } else if val.starts_with('#') {
                                let nc = map_dark_color(&val, self.is_icon_style, &name, &self.cur_style_cls);
                                set_attr(&mut attrs, "value", nc);
                            } else if val == "CommonPalette::kd-color-white" {
                                let nlo = name.to_ascii_lowercase();
                                if nlo.contains("background")
                                    || nlo.contains("bg")
                                    || nlo.contains("fill")
                                    || nlo.contains("card")
                                {
                                    set_attr(
                                        &mut attrs,
                                        "value",
                                        "CommonThemePalette::kd-color-background-middle".to_string(),
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }

        let mut out = BytesStart::new(tag);
        for (k, v) in &attrs {
            out.push_attribute((k.as_str(), v.as_str()));
        }
        Ok(if is_empty {
            Event::Empty(out.into_owned())
        } else {
            Event::Start(out.into_owned())
        })
    }
}

fn transform_kuip(src: &Path, dst: &Path, mode: KuipMode) -> Result<(), String> {
    let raw = fs::read_to_string(src).map_err(|e| format!("读取 {} 失败: {e}", src.display()))?;
    let mut reader = Reader::from_str(&raw);
    let mut writer = Writer::new(Cursor::new(Vec::new()));
    let mut st = KuipState::default();

    let tp_extras = [
        ("panel_background", "#B8C0CC"),
        ("blockBackground", "#B8C0CC"),
        ("editor_background", "#B8C0CC"),
        ("blockBackground-hover", "#A8B3C2"),
        ("blockBackground-down", "#9AA7B8"),
    ];

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let ev = st.rewrite_tag(&e, false, mode)?;
                writer.write_event(ev).map_err(|e| e.to_string())?;
            }
            Ok(Event::Empty(e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                let ev = st.rewrite_tag(&e, true, mode)?;
                if matches!(mode, KuipMode::Generic) && tag == "style" && st.in_style_tp_widget {
                    if let Event::Empty(bs) = ev {
                        writer.write_event(Event::Start(bs)).map_err(|e| e.to_string())?;
                        for (k, v) in tp_extras {
                            let mut c = BytesStart::new("color");
                            c.push_attribute(("name", k));
                            c.push_attribute(("value", v));
                            writer.write_event(Event::Empty(c)).map_err(|e| e.to_string())?;
                        }
                        writer
                            .write_event(Event::End(BytesEnd::new("style")))
                            .map_err(|e| e.to_string())?;
                    }
                    st.reset_style();
                } else {
                    writer.write_event(ev).map_err(|e| e.to_string())?;
                    if tag == "style" {
                        st.reset_style();
                    }
                }
            }
            Ok(Event::End(e)) => {
                if e.name().as_ref() == b"style" {
                    if matches!(mode, KuipMode::Generic) && st.in_style_tp_widget {
                        for (k, v) in tp_extras {
                            if !st.seen_tp_keys.iter().any(|x| x == k) {
                                let mut c = BytesStart::new("color");
                                c.push_attribute(("name", k));
                                c.push_attribute(("value", v));
                                writer.write_event(Event::Empty(c)).map_err(|e| e.to_string())?;
                            }
                        }
                    }
                    st.reset_style();
                }
                writer.write_event(Event::End(e.into_owned())).map_err(|e| e.to_string())?;
            }
            Ok(Event::Eof) => break,
            Ok(ev) => {
                writer.write_event(ev.into_owned()).map_err(|e| e.to_string())?;
            }
            Err(e) => return Err(format!("解析 {} 出错: {e}", src.display())),
        }
    }

    let bytes = writer.into_inner().into_inner();
    fs::write(dst, bytes).map_err(|e| format!("写入 {} 失败: {e}", dst.display()))
}

fn recolor_svg_for_dark(svg: &str) -> String {
    // Lightweight single-pass scanner replacing dark stroke/fill hex colors (#RGB / #RRGGBB)
    let bytes = svg.as_bytes();
    let mut out = String::with_capacity(svg.len() + 64);
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'#' {
            // Check preceding context contains stroke or fill within 16 bytes
            let lookback_start = i.saturating_sub(18);
            let ctx = svg[lookback_start..i].to_ascii_lowercase();
            let is_paint = ctx.contains("stroke") || ctx.contains("fill");
            if is_paint {
                let mut hex_len = 0;
                while i + 1 + hex_len < bytes.len() && bytes[i + 1 + hex_len].is_ascii_hexdigit() {
                    hex_len += 1;
                }
                if hex_len == 3 || hex_len == 6 {
                    let hstr = &svg[i + 1..i + 1 + hex_len];
                    let (r, g, b) = if hex_len == 3 {
                        let d = |idx: usize| {
                            let c = u8::from_str_radix(&hstr[idx..idx + 1], 16).unwrap_or(0);
                            c * 17
                        };
                        (d(0), d(1), d(2))
                    } else {
                        (
                            u8::from_str_radix(&hstr[0..2], 16).unwrap_or(0),
                            u8::from_str_radix(&hstr[2..4], 16).unwrap_or(0),
                            u8::from_str_radix(&hstr[4..6], 16).unwrap_or(0),
                        )
                    };
                    let (_h, l, s) = rgb_to_hls(r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0);
                    if s < 0.25 && l < 0.45 {
                        let new_l = 0.88 - 0.35 * l;
                        let (nr, ng, nb) = hls_to_rgb(0.60, new_l, 0.05);
                        out.push_str(&fmt_hex(nr * 255.0, ng * 255.0, nb * 255.0, ""));
                        i += 1 + hex_len;
                        continue;
                    }
                }
            }
        }
        let ch = svg[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

fn read_be_u32(b: &[u8], off: usize) -> u32 {
    u32::from_be_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}
fn read_be_u16(b: &[u8], off: usize) -> u16 {
    u16::from_be_bytes([b[off], b[off + 1]])
}

fn build_dark_rcc(src_rcc: &Path, dst_rcc: &Path) -> Result<usize, String> {
    let raw = fs::read(src_rcc).map_err(|e| format!("读取 RCC 失败: {e}"))?;
    if raw.len() < 20 || &raw[0..4] != b"qres" {
        return Err("WPS RCC 皮肤资源头格式不符。".into());
    }
    let ver = read_be_u32(&raw, 4);
    let tree_off = read_be_u32(&raw, 8) as usize;
    let data_off = read_be_u32(&raw, 12) as usize;
    let names_off = read_be_u32(&raw, 16) as usize;
    if ver != 2 {
        return Err(format!("不支持的 Qt RCC 版本: {ver}"));
    }

    let mut sections = [(tree_off, "tree"), (data_off, "data"), (names_off, "names")];
    sections.sort_by_key(|x| x.0);
    let span = |key: &str| -> (usize, usize) {
        let idx = sections.iter().position(|x| x.1 == key).unwrap();
        let start = sections[idx].0;
        let end = if idx + 1 < sections.len() { sections[idx + 1].0 } else { raw.len() };
        (start, end)
    };

    let (t0, t1) = span("tree");
    let (n0, n1) = span("names");
    let mut tree_buf = raw[t0..t1].to_vec();
    let names_buf = &raw[n0..n1];
    let nsz = 22usize;
    let node_count = tree_buf.len() / nsz;

    let read_name = |no: usize| -> String {
        if no + 6 > names_buf.len() {
            return String::new();
        }
        let ln = read_be_u16(names_buf, no) as usize;
        let start = no + 6;
        let end = (start + ln * 2).min(names_buf.len());
        let units: Vec<u16> = names_buf[start..end]
            .chunks_exact(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&units)
    };

    let mut new_data: Vec<u8> = Vec::new();
    let mut recolored = 0usize;

    for idx in 0..node_count {
        let pos = idx * nsz;
        let name_off = read_be_u32(&tree_buf, pos) as usize;
        let flags = read_be_u16(&tree_buf, pos + 4);
        if (flags & 2) != 0 {
            continue;
        }
        let nm = if idx > 0 { read_name(name_off) } else { String::new() };
        let old_d_off = read_be_u32(&tree_buf, pos + 10) as usize;
        let d_len = read_be_u32(&raw, data_off + old_d_off) as usize;
        let blob = &raw[data_off + old_d_off + 4..data_off + old_d_off + 4 + d_len];

        let mut plain = if (flags & 1) != 0 {
            let mut dec = ZlibDecoder::new(&blob[4..]);
            let mut buf = Vec::new();
            dec.read_to_end(&mut buf).map_err(|e| format!("RCC zlib 解压失败: {e}"))?;
            buf
        } else {
            blob.to_vec()
        };

        if nm.to_ascii_lowercase().ends_with(".svg") {
            let txt = String::from_utf8_lossy(&plain).into_owned();
            let new_txt = recolor_svg_for_dark(&txt);
            if new_txt != txt {
                recolored += 1;
            }
            plain = new_txt.into_bytes();
        }

        let new_off = new_data.len() as u32;
        tree_buf[pos + 10..pos + 14].copy_from_slice(&new_off.to_be_bytes());

        if (flags & 1) != 0 {
            let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
            enc.write_all(&plain).map_err(|e| format!("RCC zlib 压缩失败: {e}"))?;
            let compressed = enc.finish().map_err(|e| e.to_string())?;
            let total_len = (4 + compressed.len()) as u32;
            new_data.extend_from_slice(&total_len.to_be_bytes());
            new_data.extend_from_slice(&(plain.len() as u32).to_be_bytes());
            new_data.extend_from_slice(&compressed);
        } else {
            new_data.extend_from_slice(&(plain.len() as u32).to_be_bytes());
            new_data.extend_from_slice(&plain);
        }
    }

    let new_tree_off: u32 = 20;
    let new_names_off: u32 = new_tree_off + tree_buf.len() as u32;
    let new_data_off: u32 = new_names_off + names_buf.len() as u32;

    let mut out = Vec::with_capacity(20 + tree_buf.len() + names_buf.len() + new_data.len());
    out.extend_from_slice(b"qres");
    out.extend_from_slice(&2u32.to_be_bytes());
    out.extend_from_slice(&new_tree_off.to_be_bytes());
    out.extend_from_slice(&new_data_off.to_be_bytes());
    out.extend_from_slice(&new_names_off.to_be_bytes());
    out.extend_from_slice(&tree_buf);
    out.extend_from_slice(names_buf);
    out.extend_from_slice(&new_data);

    if let Some(parent) = dst_rcc.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(dst_rcc, out).map_err(|e| e.to_string())?;
    Ok(recolored)
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct BuildStamp {
    builder_version: u32,
    office6: String,
    files: BTreeMap<String, (u128, u64)>,
}

fn file_sig(p: &Path) -> Option<(u128, u64)> {
    let md = fs::metadata(p).ok()?;
    let mtime = md
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_nanos();
    Some((mtime, md.len()))
}

fn compute_stamp(office6_dir: &Path) -> BuildStamp {
    let mut files = BTreeMap::new();
    let themes_dir = office6_dir.join("res").join("themes");
    if let Ok(rd) = fs::read_dir(&themes_dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().and_then(|s| s.to_str()) == Some("kuip") {
                if let (Some(name), Some(sig)) = (p.file_name().and_then(|s| s.to_str()), file_sig(&p)) {
                    files.insert(name.to_string(), sig);
                }
            }
        }
    }
    let src_rcc = office6_dir
        .join("skins")
        .join("2019dark")
        .join("default")
        .join("prometheus_wpsoffice_res.rcc");
    if let Some(sig) = file_sig(&src_rcc) {
        files.insert("prometheus_wpsoffice_res.rcc".to_string(), sig);
    }
    BuildStamp {
        builder_version: BUILDER_VERSION,
        office6: office6_dir.to_string_lossy().into_owned(),
        files,
    }
}

pub fn ensure_dark_skin(office6_dir: &Path, cache_root: &Path) -> Result<(String, PathBuf), String> {
    let themes_dir = office6_dir.join("res").join("themes");
    if !themes_dir.is_dir() {
        return Err(format!("未找到 WPS 原生皮肤模板目录：{}", themes_dir.display()));
    }
    fs::create_dir_all(cache_root).map_err(|e| e.to_string())?;
    let dst_skin = cache_root.join(DARK_SKIN_NAME);
    let stamp_path = dst_skin.join(".build-stamp.json");
    let expected = compute_stamp(office6_dir);

    if stamp_path.is_file() && dst_skin.join("wps.kuip").is_file() && dst_skin.join("wpp.kuip").is_file() {
        if let Ok(txt) = fs::read_to_string(&stamp_path) {
            if let Ok(existing) = serde_json::from_str::<BuildStamp>(&txt) {
                if existing == expected {
                    return Ok((DARK_SKIN_NAME.to_string(), cache_root.to_path_buf()));
                }
            }
        }
    }

    let stage_dir = cache_root.join(format!(".{DARK_SKIN_NAME}.staging"));
    if stage_dir.exists() {
        let _ = fs::remove_dir_all(&stage_dir);
    }
    fs::create_dir_all(&stage_dir).map_err(|e| e.to_string())?;
    fs::write(stage_dir.join("skin.ini"), SKIN_INI).map_err(|e| e.to_string())?;

    let src_rcc = office6_dir
        .join("skins")
        .join("2019dark")
        .join("default")
        .join("prometheus_wpsoffice_res.rcc");
    if src_rcc.is_file() {
        let _ = build_dark_rcc(
            &src_rcc,
            &stage_dir.join("default").join("prometheus_wpsoffice_res.rcc"),
        )?;
    }

    let entries = fs::read_dir(&themes_dir).map_err(|e| e.to_string())?;
    for entry in entries.flatten() {
        let p = entry.path();
        if p.extension().and_then(|s| s.to_str()) != Some("kuip") {
            continue;
        }
        let name = p.file_name().unwrap().to_string_lossy().into_owned();
        let out = stage_dir.join(&name);
        if name == "palette.kuip" || name.starts_with("palette-") {
            transform_kuip(&p, &out, KuipMode::Palette)?;
        } else if name == "theme-palette.kuip" {
            transform_kuip(&p, &out, KuipMode::ThemePalette)?;
        } else if name == "hint.kuip" {
            fs::copy(&p, &out).map_err(|e| e.to_string())?;
        } else {
            transform_kuip(&p, &out, KuipMode::Generic)?;
        }
    }

    for (src_name, dst_name) in [
        ("prometheuswps.kuip", "wps.kuip"),
        ("prometheuswpp.kuip", "wpp.kuip"),
        ("prometheuset.kuip", "et.kuip"),
        ("prometheuspdf.kuip", "pdf.kuip"),
        ("prometheuswpsoffice.kuip", "wpsoffice.kuip"),
    ] {
        let sp = stage_dir.join(src_name);
        if sp.is_file() {
            fs::copy(&sp, stage_dir.join(dst_name)).map_err(|e| e.to_string())?;
        }
    }

    let stamp_json = serde_json::to_string_pretty(&expected).map_err(|e| e.to_string())?;
    fs::write(stage_dir.join(".build-stamp.json"), stamp_json).map_err(|e| e.to_string())?;

    if dst_skin.exists() {
        let _ = fs::remove_dir_all(&dst_skin);
    }
    fs::rename(&stage_dir, &dst_skin).map_err(|e| format!("提交皮肤目录失败: {e}"))?;
    Ok((DARK_SKIN_NAME.to_string(), cache_root.to_path_buf()))
}
