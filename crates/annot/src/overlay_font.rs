//! Installed-face redaction overlay layout, shared by marking and applying.
//!
//! Only the explicitly registered face and our existing licensed CJK sources can supply
//! outlines. A pending annotation never supplies a font or PDF program.

use std::collections::HashMap;

use pdfcraft_fonts::{GlyphError, GlyphOutline, ShapedScript, cjk_document, embedding::ExplicitFace, shape_cjk, shaped_glyph};

use crate::OverlayLook;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverlayLayoutError {
    MissingFace,
    MissingGlyph,
    Layout,
    Limit,
}

#[derive(Clone, Debug)]
pub struct OverlayGlyph {
    pub position: [f64; 2],
    id: u32,
    fallback: Option<ShapedScript>,
}

pub struct OverlayLayout {
    face: ExplicitFace,
    pub glyphs: Vec<OverlayGlyph>,
    pub advance: f64,
    pub gap: f64,
}

impl OverlayLayout {
    pub fn outline(&self, glyph: &OverlayGlyph) -> Result<GlyphOutline, GlyphError> {
        match glyph.fallback {
            None => self.face.outline(glyph.id),
            Some(script) => shaped_glyph(script, glyph.id),
        }
    }

    fn append(&mut self, text: &str, cjk: bool) -> Result<(), OverlayLayoutError> {
        if cjk {
            let clusters = shape_cjk(text).map_err(|_| OverlayLayoutError::MissingGlyph)?;
            for cluster in clusters {
                let x = self.advance;
                self.glyphs.extend(cluster.glyphs.into_iter().map(|(id, [dx, y])| OverlayGlyph {
                    position: [x + dx, y],
                    id,
                    fallback: Some(cluster.script),
                }));
                self.advance += cluster.advance;
            }
        } else {
            let run = self.face.layout(text).map_err(|_| OverlayLayoutError::Layout)?;
            let x = self.advance;
            self.glyphs.extend(run.glyphs.into_iter().map(|glyph| OverlayGlyph {
                position: [x + glyph.position[0], glyph.position[1]],
                id: glyph.id.to_u32(),
                fallback: None,
            }));
            self.advance += run.advance;
        }
        if !(0.0..=32768.0).contains(&self.advance) || self.glyphs.len() > OverlayLook::MAX_LOCAL_TEXT_BYTES {
            return Err(OverlayLayoutError::Limit);
        }
        Ok(())
    }
}

impl OverlayLook {
    /// Retain the selected face for text it covers. If it misses Han or other CJK
    /// characters, shape only those runs through our local-first document-font fallback.
    /// An unavailable explicit face is an error, not permission to replace the whole font.
    pub fn layout_local_overlay(&self, text: &str) -> Result<OverlayLayout, OverlayLayoutError> {
        let face = self.selected_local_font().ok_or(OverlayLayoutError::MissingFace)?;
        if text.len() > Self::MAX_LOCAL_TEXT_BYTES {
            return Err(OverlayLayoutError::Limit);
        }
        if text.contains(['\r', '\n']) {
            return Err(OverlayLayoutError::Layout);
        }
        let gap = if self.repeat { face.layout(" ").map_err(|_| OverlayLayoutError::Layout)?.advance.max(0.0) } else { 0.0 };
        let mut layout = OverlayLayout { face, glyphs: Vec::new(), advance: 0.0, gap };
        // Keep the existing whole-line bidirectional layout when the chosen face covers it.
        if layout.face.first_missing(text).is_none() {
            layout.append(text, false)?;
            return Ok(layout);
        }
        let mut coverage = HashMap::new();
        let mut start = 0;
        let mut fallback = None;
        for (offset, c) in text.char_indices() {
            let missing = *coverage.entry(c).or_insert_with(|| layout.face.first_missing(c.encode_utf8(&mut [0; 4])).is_some());
            if missing && !cjk_document::is_cjk(c) {
                return Err(OverlayLayoutError::MissingGlyph);
            }
            if let Some(prior) = fallback
                && prior != missing
            {
                layout.append(&text[start..offset], prior)?;
                start = offset;
            }
            fallback = Some(missing);
        }
        if let Some(fallback) = fallback {
            layout.append(&text[start..], fallback)?;
        }
        Ok(layout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixed_local_overlay_keeps_the_selected_face_for_covered_text() {
        if !cjk_document::covers("已遮盖") {
            return;
        }
        let mut look = OverlayLook::default();
        look.select_local_font(pdfcraft_fonts::INTER_REGULAR.to_vec()).unwrap();
        let latin = look.layout_local_overlay("CONFIDENTIAL ").unwrap();
        let mixed = look.layout_local_overlay("CONFIDENTIAL 已遮盖").unwrap();
        for (expected, actual) in latin.glyphs.iter().zip(&mixed.glyphs) {
            assert_eq!((expected.id, expected.position), (actual.id, actual.position));
            assert!(actual.fallback.is_none(), "the supported Latin glyph keeps the selected face");
        }
        assert!(mixed.glyphs[latin.glyphs.len()..].iter().all(|glyph| glyph.fallback.is_some()));
        assert!(mixed.glyphs.len() > latin.glyphs.len());
        assert!(matches!(look.layout_local_overlay("🦄"), Err(OverlayLayoutError::MissingGlyph)));
    }
}
