//! Unicode embedding of an explicitly supplied, embeddable TrueType face. Font files are
//! document inputs or the optional craft-fonts build input, never system-interface fallbacks.

use std::collections::HashMap;

use pdfcraft_cos::{Dict, Document, ObjRef, Object};
use skrifa::{
    FontRef, MetadataProvider,
    instance::{LocationRef, Size},
    raw::TableProvider,
};

use craft_pdf::fonts::wire::{
    CidMapping, FontDescriptorSpec, build_cid_encoding_cmap, build_cid_font_dict, build_font_descriptor, build_to_unicode_cmap,
    build_type0_font_dict, font_program_stream,
};

/// Compare PDF subset/PostScript names with the full face's family name.
pub fn face_key(name: &str) -> String {
    name.rsplit('+').next().unwrap_or(name).chars().filter(char::is_ascii_alphanumeric).flat_map(char::to_lowercase).collect()
}

/// Full, approved faces of the source family. This never silently selects another family.
pub fn matching_faces(name: &str) -> Vec<&'static crate::CraftFont> {
    let key = face_key(name);
    crate::CRAFT_FONTS.iter().filter(|f| face_key(f.family) == key || face_key(&f.name()) == key).collect()
}

/// Whether a complete font program can show every non-newline character.
pub fn covers(bytes: &[u8], text: &str) -> bool {
    FontRef::new(bytes).is_ok_and(|f| text.chars().all(|c| matches!(c, '\n' | '\r') || f.charmap().map(c).is_some_and(|g| g.to_u32() != 0)))
}

/// Names come from the program, never a caller's guessed family.
pub fn names(bytes: &[u8]) -> Result<(String, String), String> {
    validate(bytes)?;
    face_names(&FontRef::new(bytes).map_err(|e| e.to_string())?)
}

/// Inspect only the name of a valid font program. For raster use, this does not grant PDF embedding permission.
pub fn postscript_name(bytes: &[u8]) -> Result<String, String> {
    face_names(&FontRef::new(bytes).map_err(|e| e.to_string())?).map(|(_, face)| face)
}

fn face_names(f: &FontRef<'_>) -> Result<(String, String), String> {
    use skrifa::string::StringId;
    let name = |id| {
        f.localized_strings(id)
            .find(|s| s.language() == Some("en"))
            .or_else(|| f.localized_strings(id).next())
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty() && s.len() <= 128)
    };
    let family = name(StringId::FAMILY_NAME).ok_or("the font has no family name")?;
    let postscript = name(StringId::POSTSCRIPT_NAME).unwrap_or_else(|| family.replace(' ', ""));
    Ok((family, postscript))
}

/// Names for browsing installed fonts, including faces that may not be embedded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayFace {
    pub family: String,
    pub postscript: String,
    pub chinese_simplified: Option<String>,
    pub chinese_traditional: Option<String>,
    pub face_index: u32,
    pub can_embed: bool,
    pub can_rasterize: bool,
}

pub fn display_faces(bytes: &[u8], max_faces: u32) -> Vec<DisplayFace> {
    use skrifa::string::StringId;
    (0..max_faces)
        .map_while(|i| FontRef::from_index(bytes, i).ok().map(|f| (i, f)))
        .filter_map(|(face_index, f)| {
            let (family, postscript) = face_names(&f).ok()?;
            let chinese = |traditional: bool| {
                let mut best = None::<(u8, String)>;
                // Prefer explicit localized full names, but some fonts translate only the family.
                for (id, priority) in [(StringId::FULL_NAME, 0), (StringId::FAMILY_NAME, 2)] {
                    for record in f.localized_strings(id).take(128) {
                        let tag = record.language().unwrap_or_default().to_ascii_lowercase();
                        if tag != "zh" && !tag.starts_with("zh-") {
                            continue;
                        }
                        let trad = ["hant", "-tw", "-hk", "-mo"].iter().any(|p| tag.contains(p));
                        let value = record.chars().take(96).collect::<String>();
                        if value.trim().is_empty() || value.chars().any(char::is_control) {
                            continue;
                        }
                        let rank = priority + u8::from(trad != traditional) * 4;
                        if best.as_ref().is_none_or(|(old, _)| rank < *old) {
                            best = Some((rank, value));
                        }
                    }
                }
                best.map(|(_, value)| value)
            };
            let chinese_simplified = chinese(false);
            let chinese_traditional = chinese(true);
            let can_rasterize = standalone_size(bytes, face_index).is_some() && (f.glyf().is_ok() || f.cff().is_ok());
            let can_embed = can_rasterize && validate_document_face(&f).is_ok();
            Some(DisplayFace { family, postscript, chinese_simplified, chinese_traditional, face_index, can_embed, can_rasterize })
        })
        .collect()
}

/// Bound and validate a face's sfnt records. TTC table offsets are relative to the collection,
/// not to the individual face header. The standalone font gets new offsets.
fn directory(bytes: &[u8], index: u32) -> Option<(&[u8], Vec<([u8; 4], [u8; 4], &[u8])>)> {
    const MAX: usize = 16 << 20;
    let read = |i: usize| Some(u32::from_be_bytes(bytes.get(i..i.checked_add(4)?)?.try_into().ok()?) as usize);
    let off = if bytes.starts_with(b"ttcf") {
        if index as usize >= read(8)? {
            return None;
        }
        read(12usize.checked_add((index as usize).checked_mul(4)?)?)?
    } else if index == 0 {
        0
    } else {
        return None;
    };
    let header = bytes.get(off..off.checked_add(12)?)?;
    if !matches!(&header[..4], b"OTTO" | b"true" | b"\x00\x01\x00\x00") {
        return None;
    }
    let count = u16::from_be_bytes(header[4..6].try_into().ok()?) as usize;
    if !(1..=256).contains(&count) {
        return None;
    }
    let start = off.checked_add(12)?;
    let end = start.checked_add(count.checked_mul(16)?)?;
    let mut size = 12 + count * 16;
    let mut records = Vec::with_capacity(count);
    for (i, row) in bytes.get(start..end)?.chunks_exact(16).enumerate() {
        let tag: [u8; 4] = row[..4].try_into().ok()?;
        let checksum = row[4..8].try_into().ok()?;
        if records.iter().any(|(t, _, _)| *t == tag) {
            return None;
        }
        let pos = read(start + i * 16 + 8)?;
        let len = read(start + i * 16 + 12)?;
        let payload = bytes.get(pos..pos.checked_add(len)?)?;
        size = size.checked_add(len.checked_add(3)? & !3)?;
        if size > MAX {
            return None;
        }
        records.push((tag, checksum, payload));
    }
    Some((header, records))
}

fn standalone_size(bytes: &[u8], index: u32) -> Option<usize> {
    if !bytes.starts_with(b"ttcf") {
        return (index == 0 && bytes.len() <= 16 << 20).then_some(bytes.len());
    }
    let (_, rows) = directory(bytes, index)?;
    Some(12 + rows.len() * 16 + rows.iter().map(|(_, _, p)| (p.len() + 3) & !3).sum::<usize>())
}

/// Copy the selected face only. Never write the complete collection or change its licence.
pub fn standalone_face(bytes: Vec<u8>, index: u32) -> Result<Vec<u8>, String> {
    if !bytes.starts_with(b"ttcf") {
        if index == 0 && bytes.len() <= 16 << 20 && FontRef::new(&bytes).is_ok() {
            return Ok(bytes);
        }
        return Err("the installed font is invalid or exceeds 16 MiB".into());
    }
    let (header, mut rows) = directory(&bytes, index).ok_or("the selected font collection face is invalid or exceeds 16 MiB")?;
    rows.sort_unstable_by_key(|(tag, _, _)| *tag);
    let mut result = header.to_vec();
    result.resize(12 + rows.len() * 16, 0);
    let mut head = None;
    for (i, (tag, checksum, data)) in rows.into_iter().enumerate() {
        let pos = result.len();
        let r = 12 + i * 16;
        result[r..r + 4].copy_from_slice(&tag);
        result[r + 4..r + 8].copy_from_slice(&checksum);
        result[r + 8..r + 12].copy_from_slice(&(pos as u32).to_be_bytes());
        result[r + 12..r + 16].copy_from_slice(&(data.len() as u32).to_be_bytes());
        result.extend_from_slice(data);
        result.resize((result.len() + 3) & !3, 0);
        if tag == *b"head" {
            if data.len() < 12 {
                return Err("invalid collection font header".into());
            }
            head = Some(pos + 8);
            result[pos + 8..pos + 12].fill(0);
        }
    }
    let head = head.ok_or("missing collection font header")?;
    let sum = result.chunks_exact(4).fold(0u32, |s, w| s.wrapping_add(u32::from_be_bytes(w.try_into().expect("four bytes"))));
    let adjustment = 0xB1B0AFBAu32.wrapping_sub(sum);
    result[head..head + 4].copy_from_slice(&adjustment.to_be_bytes());
    FontRef::new(&result).map_err(|e| e.to_string())?;
    Ok(result)
}

/// A selected external font is document input. Require its own open-licence declaration;
/// never turn a proprietary system-interface fallback into an exported PDF font.
pub fn validate_document_input(bytes: &[u8]) -> Result<(), String> {
    validate(bytes)?;
    validate_document_face(&FontRef::new(bytes).map_err(|e| e.to_string())?)
}

/// Validate a particular face of a collection before extracting its outlines. This does
/// not permit copying a TTC into a PDF; it applies the same licence and face checks.
pub(crate) fn validate_document_face(f: &FontRef<'_>) -> Result<(), String> {
    use skrifa::string::StringId;
    validate_face(f)?;
    let (family, _) = face_names(f)?;
    let key = face_key(&family);
    if ["sourcehan", "sourceserif", "sourcesans", "notosanscjk", "notoserifcjk"].iter().any(|n| key.starts_with(n)) {
        return Err("this font family is excluded by the project's asset policy".into());
    }
    // An Adobe scan that could not finish fails closed, like a match.
    if any_name_record(f, &[StringId::COPYRIGHT_NOTICE, StringId::MANUFACTURER], |t| t.to_ascii_lowercase().contains("adobe")) != Some(false) {
        return Err("Adobe-authored font assets are excluded by the project's asset policy".into());
    }
    let licences = ["SIL OPEN FONT", "SIL Open Font", "Apache License", "Ubuntu Font Licence", "Bitstream Vera", "MIT License"];
    if any_name_record(f, &[StringId::LICENSE_DESCRIPTION], |t| licences.iter().any(|l| t.contains(l))) != Some(true) {
        return Err("select an openly licensed font whose name table declares its licence (OFL, Apache, Ubuntu, Bitstream Vera or MIT)".into());
    }
    Ok(())
}

/// Name-table text one policy check reads in all.
const MAX_NAME_BYTES: usize = 1 << 20;

/// Whether `found` holds for any record of the name-table strings `ids`, read one at a time
/// within [`MAX_NAME_BYTES`] in all: `Some(true)` for a match, `Some(false)` when every record was
/// read and none matched, `None` when the allowance ran out first (a record that does not fit what
/// is left is not searched). The table can point many records at one large string, so the records
/// are never joined.
fn any_name_record(font: &FontRef, ids: &[skrifa::string::StringId], found: impl Fn(&str) -> bool) -> Option<bool> {
    let mut left = MAX_NAME_BYTES;
    for s in ids.iter().flat_map(|id| font.localized_strings(*id)) {
        let text = s.to_string();
        if text.len() > left {
            return None;
        }
        left -= text.len().max(1).min(left);
        if found(&text) {
            return Some(true);
        }
    }
    Some(false)
}

/// Validate before allocating any PDF objects, so an invalid font cannot partly mutate a doc.
pub fn validate(bytes: &[u8]) -> Result<(), String> {
    if bytes.len() > 16 << 20 {
        return Err("the selected font exceeds 16 MiB".into());
    }
    let font = FontRef::new(bytes).map_err(|_| "select a standalone TrueType/OpenType font (not a font collection)")?;
    validate_face(&font)
}

fn validate_face(font: &FontRef<'_>) -> Result<(), String> {
    if font.fvar().is_ok() {
        return Err("select a static instance of this variable font before embedding".into());
    }
    if (font.glyf().is_err() && font.cff().is_err()) || font.head().is_err() || font.hhea().is_err() || font.hmtx().is_err() {
        return Err("the selected font has no usable TrueType/CFF outlines and metrics".into());
    }
    if font.os2().is_ok_and(|t| t.fs_type() & (0x0002 | 0x0004 | 0x0200) != 0) {
        return Err("this font's embedding permissions prohibit editable outline embedding".into());
    }
    let m = font.metrics(Size::unscaled(), LocationRef::default());
    if m.units_per_em == 0 || font.charmap().mappings().next().is_none() {
        return Err("the selected font has no Unicode mapping or valid units per em".into());
    }
    if font.charmap().mappings().take(65_535).count() >= 65_535 {
        return Err("the selected font has too many Unicode mappings for one PDF font".into());
    }
    if font.charmap().mappings().any(|(_, g)| g.to_u32() >= u32::from(m.glyph_count)) {
        return Err("the selected font maps Unicode to a glyph outside its program".into());
    }
    Ok(())
}

/// Embed the complete font once and provide separate CIDs for Unicode aliases. /ToUnicode,
/// /CIDToGIDMap and widths agree, including supplementary characters, so reopening needs no
/// installed font. Keeping the full program also lets subsequent edits add other glyphs.
pub fn embed(doc: &mut Document, fonts: &mut Dict, bytes: &[u8], family: &str) -> Result<String, String> {
    validate(bytes)?;
    let font = FontRef::new(bytes).map_err(|e| e.to_string())?;
    let mappings: Vec<_> = font
        .charmap()
        .mappings()
        .filter(|(_, gid)| gid.to_u32() > 0)
        .filter_map(|(cp, gid)| char::from_u32(cp).map(|c| (c.to_string(), gid)))
        .collect();
    embed_mapping(doc, fonts, bytes, family, &mappings)
}

/// Explicit glyph mapping for shaped text. A CID can represent a contextual glyph or a
/// ligature, while its ToUnicode retains the logical cluster. The full source program stays exact.
pub fn embed_mapping(
    doc: &mut Document,
    fonts: &mut Dict,
    bytes: &[u8],
    family: &str,
    mappings: &[(String, skrifa::GlyphId)],
) -> Result<String, String> {
    validate(bytes)?;
    if mappings.len() > 65_534 || mappings.iter().any(|(s, _)| s.len() > 4096) {
        return Err("shaped font mapping exceeds its budget".into());
    }
    let font = FontRef::new(bytes).map_err(|e| e.to_string())?;
    let cff = font.glyf().is_err();
    let loc = LocationRef::default();
    let m = font.metrics(Size::unscaled(), loc);
    let scale = 1000.0 / f64::from(m.units_per_em);
    let advances = font.glyph_metrics(Size::unscaled(), loc);
    let mut cff_widths = std::collections::BTreeMap::new();
    let mut gids = vec![0, 0];
    let mut widths = Vec::with_capacity(mappings.len());
    let mut cid_mappings = Vec::with_capacity(mappings.len());
    for (index, (unicode, gid)) in mappings.iter().enumerate() {
        let cid = u16::try_from(index.saturating_add(1)).map_err(|_| "font mapping limit")?;
        let glyph = u16::try_from(gid.to_u32()).map_err(|_| "font glyph limit")?;
        if glyph == 0 || u32::from(glyph) >= u32::from(m.glyph_count) {
            return Err("font contains an unavailable shaped glyph".into());
        }
        let width = advances.advance_width(*gid).unwrap_or(0.0) as f64 * scale;
        if !width.is_finite() || !(0.0..=16_000.0).contains(&width) {
            return Err("font contains an invalid advance".into());
        }
        gids.extend_from_slice(&glyph.to_be_bytes());
        widths.push(Object::Real(width));
        cff_widths.insert(glyph, width);
        cid_mappings.push(CidMapping { cid, glyph, width, unicode });
    }
    let cid_encoding = build_cid_encoding_cmap(&cid_mappings);
    let cmap = build_to_unicode_cmap(&cid_mappings);
    let name = family.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_')).collect::<String>();
    if name.is_empty() || name.len() > 128 {
        return Err("the font family name is invalid".into());
    }
    let glyph_map = if cff { cid_encoding.as_bytes() } else { gids.as_slice() };
    let mut programs = ProgramMatch::new(bytes);
    if let Some(resource) = same_resource(doc, fonts, &name, &mut programs, glyph_map, cmap.as_bytes()) {
        return Ok(resource);
    }
    let bounds = m.bounds.ok_or("font has no outline bounds")?;
    // Reuse the program another resource of this page already has, else one registered on another
    // page; source subset programs stay untouched.
    let program = existing_program(doc, fonts, &mut programs).unwrap_or_else(|| font_program_stream(doc, bytes, cff));
    let descriptor = build_font_descriptor(&FontDescriptorSpec {
        font_name: &name,
        family,
        italic_angle: f64::from(m.italic_angle),
        ascent: f64::from(m.ascent) * scale,
        descent: f64::from(m.descent) * scale,
        cap_height: f64::from(m.cap_height.unwrap_or(m.ascent)) * scale,
        bbox: [bounds.x_min, bounds.y_min, bounds.x_max, bounds.y_max].map(|v| f64::from(v) * scale),
        cff,
        program,
    });
    let desc = Object::Ref(doc.add(Object::Dict(descriptor)));
    let cid = build_cid_font_dict(doc, &name, desc, cff, cff_widths, gids, widths);
    let descendant = Object::Ref(doc.add(Object::Dict(cid)));
    let top = build_type0_font_dict(doc, &name, descendant, cff.then(|| cid_encoding.into_bytes()), cmap.into_bytes());
    let mut index = 0usize;
    while fonts.contains(format!("PCFull{index}").as_bytes()) {
        index = index.saturating_add(1);
    }
    let resource = format!("PCFull{index}");
    fonts.set(resource.as_bytes().to_vec(), Object::Ref(doc.add(Object::Dict(top))));
    Ok(resource)
}

/// Whether the stream `object` refers to decodes to `expected`.
fn stream_is(doc: &Document, object: Option<&Object>, expected: &[u8]) -> bool {
    object.is_some_and(|o| match doc.resolve(o).as_ref() {
        Object::Stream(s) => s.decoded_within(expected.len()).is_ok_and(|d| d == expected),
        _ => false,
    })
}

/// Full programs one embedding decodes at most while looking for its own. A real match passes
/// the cheap checks and decodes once; many candidates that pass them come from a hostile file.
const MAX_PROGRAM_DECODES: usize = 16;

/// Recognises the full program written for `bytes`: each candidate stream decodes at most once
/// and within `bytes.len()`, and at most [`MAX_PROGRAM_DECODES`] decode in all.
struct ProgramMatch<'a> {
    bytes: &'a [u8],
    known: HashMap<ObjRef, bool>,
    decodes: usize,
}

impl<'a> ProgramMatch<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, known: HashMap::new(), decodes: 0 }
    }

    /// Whether `r`, holding `object`, is the program.
    fn is(&mut self, doc: &Document, r: ObjRef, object: &Object) -> bool {
        if let Some(&known) = self.known.get(&r) {
            return known;
        }
        let found = match object {
            Object::Stream(s)
                if s.dict.get(b"PCFullFont").is_some()
                    && s.dict.get(b"Length1").and_then(|l| doc.resolve(l).as_int()) == i64::try_from(self.bytes.len()).ok()
                    && self.decodes < MAX_PROGRAM_DECODES =>
            {
                self.decodes = self.decodes.saturating_add(1);
                s.decoded_within(self.bytes.len()).is_ok_and(|data| data == self.bytes)
            }
            _ => false,
        };
        self.known.insert(r, found);
        found
    }

    /// Whether the resource's program is this one.
    fn written(&mut self, doc: &Document, w: &Written) -> bool {
        w.program.as_ref().is_some_and(|r| self.is(doc, r, &doc.get(r)))
    }
}

/// The program for `programs.bytes` that this page's `PCFull` resources, or else the first
/// objects of the document, already have.
fn existing_program(doc: &Document, fonts: &Dict, programs: &mut ProgramMatch) -> Option<Object> {
    written(doc, fonts)
        .find(|w| programs.written(doc, w))
        .map(|w| w.program)
        .or_else(|| doc.scan_objects().take(8192).find_map(|(r, o)| programs.is(doc, r, &o).then_some(Object::Ref(r))))
}

/// A `PCFull` resource that [`embed_mapping`] wrote.
struct Written {
    resource: String,
    top: Dict,
    cid: Dict,
    /// The reference to its full program.
    program: Object,
}

/// The `PCFull` resources in `fonts`, in order.
fn written<'a>(doc: &'a Document, fonts: &'a Dict) -> impl Iterator<Item = Written> + 'a {
    fonts.iter().filter(|(key, _)| key.starts_with(b"PCFull")).take(256).filter_map(move |(key, value)| {
        let top = doc.dict(value)?;
        let cid = doc.dict(doc.resolve(top.get(b"DescendantFonts")?).as_array()?.first()?)?;
        let descriptor = doc.dict(cid.get(b"FontDescriptor")?)?;
        let program = descriptor.get(b"FontFile2").or_else(|| descriptor.get(b"FontFile3")).filter(|p| matches!(p, Object::Ref(_)))?.clone();
        Some(Written { resource: String::from_utf8_lossy(key).into_owned(), top, cid, program })
    })
}

/// A resource in `fonts` that already is this embedding: the same font name, program, glyph map
/// and Unicode map. Editing the same text again reuses it instead of adding another copy.
fn same_resource(doc: &Document, fonts: &Dict, name: &str, programs: &mut ProgramMatch, glyph_map: &[u8], unicode: &[u8]) -> Option<String> {
    written(doc, fonts)
        .find(|w| {
            let glyphs = w.cid.get(b"CIDToGIDMap").or_else(|| w.top.get(b"Encoding"));
            w.top.name(b"BaseFont") == Some(name.as_bytes())
                && stream_is(doc, w.top.get(b"ToUnicode"), unicode)
                && stream_is(doc, glyphs, glyph_map)
                && programs.written(doc, w)
        })
        .map(|w| w.resource)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::Metrics;
    use pdfcraft_cos::Stream;

    #[test]
    fn full_unicode_embedding_keeps_outlines_advances_and_roundtrip_codes() {
        let bytes = include_bytes!("../../../assets/fonts/Inter-Regular.ttf");
        let mut doc = Document::new_empty();
        let mut fonts = Dict::new();
        let name = embed(&mut doc, &mut fonts, bytes, "Inter").unwrap();
        let font = doc.dict(fonts.get(name.as_bytes()).unwrap()).unwrap();
        let m = Metrics::from_dict(&doc, &font);
        let text = "Aé ΩЖ €";
        let codes = m.encode(text).unwrap();
        assert_eq!(m.decode(&codes), text);
        assert_eq!(crate::pdf::embedded_sfnt(&doc, &font).unwrap(), bytes);
        let sfnt = FontRef::new(bytes).unwrap();
        for (ch, (code, _)) in text.chars().zip(m.codes(&codes)) {
            let gid = sfnt.charmap().map(ch).unwrap();
            let w = sfnt.glyph_metrics(Size::unscaled(), LocationRef::default()).advance_width(gid).unwrap() as f64
                / f64::from(sfnt.metrics(Size::unscaled(), LocationRef::default()).units_per_em);
            assert!((m.width(code) - w).abs() < 1e-8);
        }
        let before = doc.object_numbers().len();
        assert!(embed(&mut doc, &mut fonts, b"broken font", "Broken").is_err());
        assert!(embed_mapping(&mut doc, &mut fonts, bytes, "Inter", &[("X".into(), skrifa::GlyphId::new(65_535))]).is_err());
        assert_eq!(doc.object_numbers().len(), before);
        let programs = || doc.scan_objects().filter(|(_, o)| matches!(o.as_ref(), Object::Stream(s) if s.dict.get(b"PCFullFont").is_some())).count();
        assert_eq!(programs(), 1);
        let mut second = Dict::new();
        embed(&mut doc, &mut second, bytes, "Inter").unwrap();
        assert_eq!(doc.scan_objects().filter(|(_, o)| matches!(o.as_ref(), Object::Stream(s) if s.dict.get(b"PCFullFont").is_some())).count(), 1);
        assert_eq!(face_key("ABCDEF+Droid-Sans-Fallback"), face_key("Droid Sans Fallback"));
    }

    #[test]
    fn licence_policy_reads_name_records_one_at_a_time() {
        use skrifa::string::StringId;
        let bytes = include_bytes!("../../../assets/fonts/Inter-Regular.ttf");
        assert!(validate_document_input(bytes).is_ok(), "an OFL font passes");
        let face = FontRef::new(bytes).unwrap();
        assert_eq!(any_name_record(&face, &[StringId::LICENSE_DESCRIPTION], |t| t.contains("SIL Open Font")), Some(true));
        assert_eq!(any_name_record(&face, &[StringId::LICENSE_DESCRIPTION], |_| false), Some(false));
        assert_eq!(any_name_record(&face, &[], |_| true), Some(false), "no ids, no records");
    }

    #[test]
    fn embedding_the_same_text_again_reuses_its_resource_and_program() {
        let bytes = include_bytes!("../../../assets/fonts/Inter-Regular.ttf");
        let face = FontRef::new(bytes).unwrap();
        let glyph = |c| face.charmap().map(c).unwrap();
        let programs = |doc: &Document| {
            doc.scan_objects().filter(|(_, o)| matches!(o.as_ref(), Object::Stream(s) if s.dict.get(b"PCFullFont").is_some())).count()
        };
        // Objects before the program push it past the first 8192 that the document-wide search reads.
        let mut doc = Document::new_empty();
        for _ in 0..9000 {
            doc.add(Object::Null);
        }
        let mut fonts = Dict::new();
        let full = embed(&mut doc, &mut fonts, bytes, "Inter").unwrap();
        let objects = doc.scan_objects().count();
        assert_eq!(embed(&mut doc, &mut fonts, bytes, "Inter").unwrap(), full, "the same embedding is the same resource");
        let shaped = [("fi".to_string(), glyph('f')), ("é".to_string(), glyph('é'))];
        let first = embed_mapping(&mut doc, &mut fonts, bytes, "Inter", &shaped).unwrap();
        assert_ne!(first, full, "another mapping is another resource");
        assert_eq!(embed_mapping(&mut doc, &mut fonts, bytes, "Inter", &shaped).unwrap(), first);
        assert_eq!(embed_mapping(&mut doc, &mut fonts, bytes, "Inter", &shaped[..1]).map(|r| r != first), Ok(true), "a different mapping");
        assert_eq!(programs(&doc), 1, "one program for every mapping");
        assert_eq!(fonts.iter().count(), 3);
        assert!(doc.scan_objects().count() > objects);
        // Another writer may have compressed the streams since; the page still recognises them.
        for (r, o) in doc.scan_objects().collect::<Vec<_>>() {
            if let Object::Stream(st) = o.as_ref()
                && st.dict.get(b"Filter").is_none()
            {
                let data = st.decoded().unwrap();
                doc.set(r, Object::Stream(Stream::flate(st.dict.clone(), &data)));
            }
        }
        let objects = doc.scan_objects().count();
        assert_eq!(embed(&mut doc, &mut fonts, bytes, "Inter").unwrap(), full);
        assert_eq!(embed_mapping(&mut doc, &mut fonts, bytes, "Inter", &shaped).unwrap(), first);
        assert_eq!(doc.scan_objects().count(), objects, "nothing new is written");
        // A different program under the same name is not taken for this one.
        let other = include_bytes!("../../../assets/fonts/Inter-Medium.ttf");
        let mut more = fonts.clone();
        let medium = embed(&mut doc, &mut more, other, "Inter").unwrap();
        assert!(!fonts.contains(medium.as_bytes()), "{medium} is new");
        assert_eq!(programs(&doc), 2);
    }

    #[test]
    fn the_program_search_decodes_each_candidate_once_and_few_in_all() {
        let bytes = b"the program".as_slice();
        let decoy = |doc: &mut Document| {
            let mut d = Dict::new();
            d.set(b"PCFullFont".to_vec(), Object::Bool(true));
            d.set(b"Length1".to_vec(), Object::Int(bytes.len() as i64));
            doc.add(Object::Stream(Stream::flate(d, &[0; 4096])))
        };
        let resource = |doc: &mut Document, program: ObjRef| {
            let mut descriptor = Dict::new();
            descriptor.set(b"FontFile2".to_vec(), Object::Ref(program));
            let mut cid = Dict::new();
            cid.set(b"FontDescriptor".to_vec(), Object::Dict(descriptor));
            let mut top = Dict::new();
            top.set(b"DescendantFonts".to_vec(), Object::Array(vec![Object::Dict(cid)]));
            doc.add(Object::Dict(top))
        };
        // 200 resources sharing one decoy program decode it once.
        let mut doc = Document::new_empty();
        let shared = decoy(&mut doc);
        let mut fonts = Dict::new();
        for i in 0..200 {
            let r = resource(&mut doc, shared);
            fonts.set(format!("PCFull{i}").into_bytes(), Object::Ref(r));
        }
        let mut programs = ProgramMatch::new(bytes);
        assert_eq!(existing_program(&doc, &fonts, &mut programs), None);
        assert_eq!(programs.decodes, 1);
        // 100 distinct decoys decode a bounded number of times.
        let mut doc = Document::new_empty();
        let mut fonts = Dict::new();
        for i in 0..100 {
            let program = decoy(&mut doc);
            let r = resource(&mut doc, program);
            fonts.set(format!("PCFull{i}").into_bytes(), Object::Ref(r));
        }
        let mut programs = ProgramMatch::new(bytes);
        assert_eq!(existing_program(&doc, &fonts, &mut programs), None);
        assert_eq!(programs.decodes, MAX_PROGRAM_DECODES);
    }

    #[test]
    fn the_full_program_is_written_compressed_and_another_page_reuses_it() {
        let bytes = include_bytes!("../../../assets/fonts/Inter-Regular.ttf");
        let mut doc = Document::new_empty();
        let mut first = Dict::new();
        embed(&mut doc, &mut first, bytes, "Inter").unwrap();
        let program = || {
            doc.scan_objects()
                .find_map(|(_, o)| match o.as_ref() {
                    Object::Stream(s) if s.dict.get(b"PCFullFont").is_some() => Some(s.clone()),
                    _ => None,
                })
                .unwrap()
        };
        let stream = program();
        assert_eq!(stream.dict.name(b"Filter"), Some(b"FlateDecode".as_slice()));
        assert!(stream.raw.len() < bytes.len(), "{} of {} bytes", stream.raw.len(), bytes.len());
        assert_eq!(stream.decoded().unwrap(), bytes);
        // Another page's resources don't have it, so it is found among the document's objects.
        let objects = doc.scan_objects().count();
        let mut second = Dict::new();
        embed(&mut doc, &mut second, bytes, "Inter").unwrap();
        let programs = doc.scan_objects().filter(|(_, o)| matches!(o.as_ref(), Object::Stream(s) if s.dict.get(b"PCFullFont").is_some())).count();
        assert_eq!(programs, 1, "one program for both pages");
        assert!(doc.scan_objects().count() > objects, "the second page gets its own resource");
    }

    #[test]
    fn custom_cmap_wire_builders_round_trip_through_metrics_across_chunks_and_surrogates() {
        let mut doc = Document::new_empty();
        let mut cff_widths = std::collections::BTreeMap::new();
        let mut entries: Vec<(u16, u16, f64, String)> =
            vec![(1, 301, 512.0, "A".into()), (2, 302, 625.0, "é".into()), (3, 303, 750.0, "fi".into()), (4, 304, 880.0, "𠀋".into())];
        // Exceed the 100-entry CMap chunk size so multi-block begincidchar/beginbfchar output is exercised.
        for i in 5..=125u16 {
            let ch = char::from_u32(0x4E00 + u32::from(i)).unwrap();
            entries.push((i, 300 + i, 500.0 + f64::from(i), ch.to_string()));
        }
        for &(_, glyph, width, _) in &entries {
            cff_widths.insert(glyph, width);
        }
        let cid_mappings: Vec<_> = entries
            .iter()
            .map(|(cid, glyph, width, unicode)| CidMapping { cid: *cid, glyph: *glyph, width: *width, unicode: unicode.as_str() })
            .collect();
        let cid_encoding = build_cid_encoding_cmap(&cid_mappings);
        let to_unicode = build_to_unicode_cmap(&cid_mappings);
        let program = font_program_stream(&mut doc, b"OTTO", true);
        let descriptor = build_font_descriptor(&FontDescriptorSpec {
            font_name: "CustomCffTest",
            family: "CustomCffTest",
            italic_angle: 0.0,
            ascent: 800.0,
            descent: -200.0,
            cap_height: 700.0,
            bbox: [0.0, -200.0, 1000.0, 800.0],
            cff: true,
            program,
        });
        let desc = Object::Ref(doc.add(Object::Dict(descriptor)));
        let cid = build_cid_font_dict(&mut doc, "CustomCffTest", desc, true, cff_widths, Vec::new(), Vec::new());
        let descendant = Object::Ref(doc.add(Object::Dict(cid)));
        let top = build_type0_font_dict(&mut doc, "CustomCffTest", descendant, Some(cid_encoding.into_bytes()), to_unicode.into_bytes());

        let m = Metrics::from_dict(&doc, &top);
        for &(cid_code, glyph, width, ref expected) in &entries {
            let code = u32::from(cid_code);
            let bytes = cid_code.to_be_bytes();
            assert_eq!(m.decode(&bytes), *expected, "cid {cid_code} ToUnicode");
            assert_eq!(m.cid(code), u32::from(glyph), "cid {cid_code} PCGlyphs mapping");
            assert!((m.width(code) - width / 1000.0).abs() < 1e-8, "cid {cid_code} width");
        }
    }
}

/// A user-chosen embeddable program. Never holds a path: reopening a PDF resolves only the
/// program attached to its own font resource, and never consults a path from the document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExplicitFace {
    bytes: std::sync::Arc<Vec<u8>>,
    postscript: String,
    program_id: u64,
}

impl ExplicitFace {
    pub fn new(bytes: Vec<u8>) -> Result<Self, String> {
        validate_document_input(&bytes)?;
        let (_, postscript) = names(&bytes)?;
        use std::hash::{DefaultHasher, Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        bytes.hash(&mut hasher);
        let program_id = hasher.finish();
        Ok(Self { bytes: std::sync::Arc::new(bytes), postscript, program_id })
    }

    pub fn name(&self) -> &str {
        &self.postscript
    }

    pub fn program_len(&self) -> usize {
        self.bytes.len()
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn program_id(&self) -> u64 {
        self.program_id
    }

    pub fn first_missing(&self, text: &str) -> Option<char> {
        let face = FontRef::new(&self.bytes).ok()?;
        let map = face.charmap();
        text.chars().map(|c| if c == '\t' { ' ' } else { c }).find(|&c| !matches!(c, '\r' | '\n') && map.map(c).is_none_or(|gid| gid.to_u32() == 0))
    }

    pub fn layout(&self, line: &str) -> Result<crate::source_shaping::ShapedRun, String> {
        crate::source_shaping::shape(&self.bytes, &line.replace('\t', " "))
    }

    /// A bounded outline of a shaped glyph belonging to this selected face.
    pub fn outline(&self, glyph: u32) -> Result<crate::GlyphOutline, crate::GlyphError> {
        crate::face_glyph(&self.bytes, glyph)
    }

    pub fn width(&self, line: &str, size: f64) -> Option<f64> {
        self.layout(line).ok().map(|row| row.advance * size)
    }

    pub fn nominal_width(&self, text: &str, size: f64) -> Option<f64> {
        let f = FontRef::new(&self.bytes).ok()?;
        let cmap = f.charmap();
        let loc = LocationRef::default();
        let metrics = f.metrics(Size::unscaled(), loc);
        let advances = f.glyph_metrics(Size::unscaled(), loc);
        let width: f64 =
            text.chars().filter_map(|c| cmap.map(if c == '\t' { ' ' } else { c })).filter_map(|gid| advances.advance_width(gid)).map(f64::from).sum();
        Some(width * size / f64::from(metrics.units_per_em))
    }

    /// Reuse the complete program attached to an explicitly selected resource we wrote. Limits
    /// and policy are checked again before this program can be used to lay out any newly edited content.
    pub fn program_ref(doc: &Document, resource: &Object) -> Option<ObjRef> {
        let top = doc.resolve(resource);
        let top = top.as_dict()?;
        let descendants = doc.resolve(top.get(b"DescendantFonts")?);
        let cid = doc.resolve(descendants.as_array()?.first()?);
        let descriptor = doc.resolve(cid.as_dict()?.get(b"FontDescriptor")?);
        let descriptor = descriptor.as_dict()?;
        descriptor.get(b"FontFile2").or_else(|| descriptor.get(b"FontFile3"))?.as_ref()
    }

    pub fn from_pdf_program(doc: &Document, reference: ObjRef) -> Option<Self> {
        let program = doc.get(reference);
        let Object::Stream(program) = program.as_ref() else { return None };
        if !matches!(program.dict.get(b"PCFullFont"), Some(Object::Bool(true))) {
            return None;
        }
        Self::new(program.decoded_within(16 << 20).ok()?).ok()
    }

    /// A selected program embedded once for all shaped rows. Every glyph (including contextual
    /// glyphs) gets the correct position, mapping, and original logical text.
    pub fn embed_rows(
        &self,
        doc: &mut Document,
        fonts: &mut Dict,
        rows: &[crate::source_shaping::ShapedRun],
    ) -> Result<(String, Vec<Vec<u16>>), String> {
        use std::collections::hash_map::Entry;
        let mut index = HashMap::new();
        let mut mappings = Vec::new();
        let mut all = Vec::with_capacity(rows.len());
        for row in rows {
            let mut encoded = Vec::with_capacity(row.glyphs.len());
            for glyph in &row.glyphs {
                let key = (glyph.text.clone(), glyph.id);
                let code = match index.entry(key.clone()) {
                    Entry::Occupied(e) => *e.get(),
                    Entry::Vacant(e) => {
                        if mappings.len() >= 65_534 {
                            return Err("shaped font mapping exceeds its budget".into());
                        }
                        mappings.push(key);
                        *e.insert(mappings.len() as u16)
                    }
                };
                encoded.push(code);
            }
            all.push(encoded);
        }
        let resource = embed_mapping(doc, fonts, &self.bytes, &self.postscript, &mappings)?;
        Ok((resource, all))
    }
}
