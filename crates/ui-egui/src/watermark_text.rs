//! Rasterize watermark text, including Simplified Chinese, without redistributing OS fonts.
//!
//! Existing PDF watermark text uses WinAnsi and cannot encode Chinese. We outline the
//! characters with a locally installed face, render transparent pixels using resvg, and
//! pass a PNG to the existing editable/removable PDF watermark-image pipeline.
//! This is the same "transparent text image" approach as the supplied HTML reference.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::{FontRef, MetadataProvider};

/// PDF point -> raster pixel density. The caller marks it in Watermark::raster_dpi.
pub const DPI: f64 = 3.0;
const MAX_FONT_BYTES: u64 = 96 << 20;
const MAX_GLYPHS: usize = 80;
const MAX_PIXELS: u32 = 8192;

#[derive(Default)]
struct SvgPen {
    commands: String,
}
impl OutlinePen for SvgPen {
    fn move_to(&mut self, x: f32, y: f32) {
        let _ = write!(self.commands, "M{x:.3} {y:.3} ");
    }
    fn line_to(&mut self, x: f32, y: f32) {
        let _ = write!(self.commands, "L{x:.3} {y:.3} ");
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let _ = write!(self.commands, "Q{x1:.3} {y1:.3} {x:.3} {y:.3} ");
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let _ = write!(self.commands, "C{x1:.3} {y1:.3} {x2:.3} {y2:.3} {x:.3} {y:.3} ");
    }
    fn close(&mut self) {
        self.commands.push_str("Z ");
    }
}

fn windows_fonts() -> PathBuf {
    std::env::var_os("WINDIR")
        .or_else(|| std::env::var_os("SystemRoot"))
        .map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from)
        .join("Fonts")
}

/// Fonts are read from the local OS only, never copied into an installer or embedded in PDFs.
/// Prefer the two Chinese fonts users know from the reference HTML.
fn font_paths(family: usize) -> Vec<PathBuf> {
    if cfg!(windows) {
        let base = windows_fonts();
        let files: &[&str] = match family {
            1 => &["simhei.ttf", "msyh.ttc", "simsun.ttc", "msjh.ttc"],
            2 => &["arial.ttf", "msyh.ttc", "simsun.ttc"],
            3 => &["georgia.ttf", "msyh.ttc", "simsun.ttc"],
            4 => &["consola.ttf", "msyh.ttc", "simsun.ttc"],
            _ => &["msyh.ttc", "simhei.ttf", "simsun.ttc", "msjh.ttc"],
        };
        files.iter().map(|f| base.join(f)).collect()
    } else if cfg!(target_os = "macos") {
        ["PingFang.ttc", "Hiragino Sans GB.ttc", "STHeiti Light.ttc"]
            .iter().map(|s| Path::new("/System/Library/Fonts").join(s)).collect()
    } else {
        [
            "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
            "/usr/share/fonts/truetype/arphic/uming.ttc",
            "/usr/share/fonts/truetype/arphic-gbsn00lp/gbsn00lp.ttf",
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        ].iter().map(PathBuf::from).collect()
    }
}

fn supports(font: &FontRef<'_>, text: &str) -> bool {
    text.chars().all(|c| c == '\n' || c == '\r' || font.charmap().map(c).is_some_and(|id| id.to_u32() != 0))
}

fn choose_face(text: &str, family: usize) -> Result<(Vec<u8>, u32), String> {
    // Use an installed Chinese face for Han and punctuation. Inter handles Latin on
    // machines with no Chinese fonts, such as isolated CI tests on Linux.
    let latin = include_bytes!("../../../assets/fonts/Inter-Regular.ttf");
    if let Ok(f) = FontRef::new(latin) {
        if supports(&f, text) && family == 2 {
            return Ok((latin.to_vec(), 0));
        }
    }
    for path in font_paths(family) {
        let Ok(meta) = std::fs::metadata(&path) else { continue };
        if !meta.is_file() || meta.len() > MAX_FONT_BYTES {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else { continue };
        for index in 0..16 {
            if skrifa::FontRef::from_index(&bytes, index).is_ok_and(|f| supports(&f, text)) {
                return Ok((bytes, index));
            }
        }
    }
    if let Ok(f) = FontRef::new(latin) {
        if supports(&f, text) {
            return Ok((latin.to_vec(), 0));
        }
    }
    Err("无法生成中文水印：此电脑没有覆盖所输入汉字的可用字体（建议安装微软雅黑或黑体）。".into())
}

/// Render one line of watermark text to a transparent RGBA PNG.
/// Includes all target glyphs as SVG paths, so no font assets are exported.
pub fn png(text: &str, size: f64, color: [f64; 3], family: usize) -> Result<Vec<u8>, String> {
    let text = text.trim();
    if text.is_empty() || text.chars().count() > MAX_GLYPHS || (text.contains('\n') || text.contains('\r')) {
        return Err("水印文字须为 1 至 80 个字符且不能包含换行。".into());
    }
    if !size.is_finite() || !(8.0..=100.0).contains(&size) {
        return Err("水印字号必须介于 8–100 pt。".into());
    }
    let (bytes, face_index) = choose_face(text, family)?;
    let font = FontRef::from_index(&bytes, face_index).map_err(|_| "无法读取水印字体。")?;
    let loc = LocationRef::default();
    let units = f64::from(font.metrics(Size::unscaled(), loc).units_per_em.max(1));
    let advances = font.glyph_metrics(Size::unscaled(), loc);
    let mut x = 0.0f64;
    let mut paths = String::new();
    for ch in text.chars() {
        let id = font.charmap().map(ch).ok_or_else(|| format!("水印字体缺少字符：{ch}"))?;
        if let Some(outline) = font.outline_glyphs().get(id) {
            let mut pen = SvgPen::default();
            outline.draw(DrawSettings::unhinted(Size::unscaled(), loc), &mut pen)
                .map_err(|_| format!("无法绘制水印字符：{ch}"))?;
            if !pen.commands.is_empty() {
                let _ = write!(paths, "<path transform=\"translate({x:.3} 0)\" d=\"{}\"/>", pen.commands);
            }
        }
        x += f64::from(advances.advance_width(id).unwrap_or(0.0));
    }
    let padding = size * 0.7;
    let width_pts = x / units * size + padding * 2.0;
    let height_pts = size * 1.7 + padding * 2.0;
    let width = (width_pts * DPI).ceil() as u32;
    let height = (height_pts * DPI).ceil() as u32;
    if width == 0 || height == 0 || width > MAX_PIXELS || height > MAX_PIXELS {
        return Err("水印文字过长或字号过大，请缩短内容后重试。".into());
    }
    let rgb = color.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8);
    let baseline = height_pts / 2.0 + size * 0.34;
    let glyph_scale = size / units;
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" viewBox=\"0 0 {width_pts:.3} {height_pts:.3}\">\
        <g fill=\"rgb({},{},{})\" transform=\"translate({padding:.3} {baseline:.3}) scale({glyph_scale:.7} -{glyph_scale:.7})\">{paths}</g></svg>",
        rgb[0], rgb[1], rgb[2],
    );
    let tree = resvg::usvg::Tree::from_str(&svg, &resvg::usvg::Options::default())
        .map_err(|e| format!("水印图形解析失败：{e}"))?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height).ok_or("无法分配水印图像内存。")?;
    resvg::render(&tree, resvg::tiny_skia::Transform::default(), &mut pixmap.as_mut());
    if !pixmap.data().chunks_exact(4).any(|p| p[3] > 0) {
        return Err("水印文字没有绘制出可见像素。".into());
    }
    pixmap.encode_png().map_err(|e| format!("无法编码水印 PNG：{e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn latin_watermark_produces_png() {
        let bytes = png("CONFIDENTIAL", 34.0, [0.4, 0.5, 0.6], 2).unwrap();
        assert!(bytes.starts_with(&[137, 80, 78, 71, 13, 10, 26, 10]));
    }
    #[test]
    fn chinese_watermark_renders_when_os_font_available() {
        let result = png("内部资料 · 请勿外传", 34.0, [0.3, 0.4, 0.5], 0);
        if cfg!(windows) {
            let bytes = result.expect("Windows has a Simplified Chinese system font");
            assert!(bytes.len() > 400);
        } else if let Ok(bytes) = result {
            assert!(bytes.len() > 400);
        }
    }
    #[test]
    fn long_untrusted_string_is_rejected() {
        assert!(png(&"机密".repeat(90), 34.0, [0.1; 3], 0).is_err());
    }
}
