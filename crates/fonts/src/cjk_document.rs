//! Whole-run font selection for CJK text written into a PDF.

use std::sync::OnceLock;

use skrifa::{FontRef, MetadataProvider};

pub fn is_cjk(c: char) -> bool {
    matches!(c,
        '\u{3000}'..='\u{30ff}' | '\u{31f0}'..='\u{31ff}' | '\u{3400}'..='\u{4dbf}' |
        '\u{4e00}'..='\u{9fff}' | '\u{ac00}'..='\u{d7af}' | '\u{f900}'..='\u{faff}' |
        '\u{ff01}'..='\u{ff9f}' | '\u{20000}'..='\u{323af}')
}

#[derive(Debug)]
enum Bytes {
    Bundled(&'static [u8]),
    #[cfg(windows)]
    Local {
        bytes: std::sync::Arc<Vec<u8>>,
        index: u32,
    },
}

impl Bytes {
    fn read(&self) -> Option<FontRef<'_>> {
        match self {
            Self::Bundled(bytes) => FontRef::new(bytes).ok(),
            #[cfg(windows)]
            Self::Local { bytes, index } => FontRef::from_index(bytes, *index).ok(),
        }
    }
}

struct Face {
    bytes: Bytes,
    shaper: OnceLock<harfrust::ShaperData>,
}

impl From<Bytes> for Face {
    fn from(bytes: Bytes) -> Self {
        Self { bytes, shaper: OnceLock::new() }
    }
}

fn fonts() -> &'static [Face] {
    static FONTS: OnceLock<Vec<Face>> = OnceLock::new();
    FONTS.get_or_init(|| {
        let mut faces = Vec::new();
        #[cfg(windows)]
        if !std::env::var_os("PDFCRAFT_SYSTEM_FONTS").is_some_and(|v| v == "0") {
            faces.extend(local_faces().into_iter().map(Face::from));
        }
        // Prefer Chinese faces over Japanese for Han-only input, but keep both for mixed text.
        faces.extend(crate::ui_chinese_fonts().into_iter().chain(crate::ui_japanese_fonts()).map(|f| Face::from(Bytes::Bundled(f.bytes))));
        faces
    })
}

pub(crate) fn face(index: usize) -> Option<FontRef<'static>> {
    fonts().get(index)?.bytes.read()
}

pub(crate) fn shaper(index: usize, font: &FontRef<'_>) -> Option<&'static harfrust::ShaperData> {
    Some(fonts().get(index)?.shaper.get_or_init(|| harfrust::ShaperData::new(font)))
}

pub(crate) fn select(text: &str) -> Option<(usize, FontRef<'static>)> {
    if !text.chars().any(is_cjk) {
        return None;
    }
    fonts().iter().enumerate().find_map(|(index, face)| {
        let f = face.bytes.read()?;
        let map = f.charmap();
        text.chars().all(|c| matches!(c, '\n' | '\r' | '\t') || map.map(c).is_some_and(|g| g.to_u32() != 0)).then_some((index, f))
    })
}

pub fn covers(text: &str) -> bool {
    select(text).is_some()
}

/// One eligible local face for an existing paragraph fallback. The bundled outline path
/// remains the fallback when the machine has no eligible custom-installed face.
pub fn local_document_face(text: &str) -> Option<std::sync::Arc<Vec<u8>>> {
    #[cfg(windows)]
    {
        let (index, _) = select(text)?;
        if let Bytes::Local { bytes, index: 0 } = &fonts().get(index)?.bytes
            && FontRef::new(bytes).is_ok()
            && bytes.len() <= 16 << 20
        {
            return Some(std::sync::Arc::clone(bytes));
        }
    }
    let _ = text;
    None
}

pub fn refusal(text: &str) -> Option<String> {
    text.split(['\n', '\r'])
        .find(|line| line.chars().any(is_cjk) && !covers(line))
        .map(|_| "No available document font covers this Chinese, Japanese or Korean text. Install an embeddable font that supports it.".to_owned())
}

/// Only fonts approved by the existing document-input policy enter saved PDFs. The
/// installed viewer fonts may be proprietary and remain available for display only.
#[cfg(windows)]
fn local_faces() -> Vec<Bytes> {
    use std::io::Read as _;
    use std::path::PathBuf;
    use std::sync::Arc;
    const FILE_MAX: u64 = 32 << 20;
    let mut candidates = Vec::new();
    for (variable, folder) in [("LOCALAPPDATA", "Microsoft/Windows/Fonts"), ("WINDIR", "Fonts")] {
        if let Some(root) = std::env::var_os(variable).map(|p| PathBuf::from(p).join(folder))
            && let Ok(entries) = std::fs::read_dir(root)
        {
            let mut paths: Vec<_> = entries.take(2048).flatten().map(|e| e.path()).collect();
            paths.sort_unstable();
            candidates.extend(paths);
        }
    }
    let mut read_left = 192 << 20;
    let mut saved = 0;
    let mut faces = Vec::new();
    for path in candidates {
        if !path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|ext| ["ttf", "otf", "ttc", "otc"].iter().any(|candidate| ext.eq_ignore_ascii_case(candidate)))
        {
            continue;
        }
        let Ok(meta) = std::fs::symlink_metadata(&path) else { continue };
        if !meta.is_file() || meta.len() == 0 || meta.len() > FILE_MAX || meta.len() > read_left {
            continue;
        }
        read_left -= meta.len();
        let mut bytes = Vec::new();
        if std::fs::File::open(&path).and_then(|f| f.take(meta.len() + 1).read_to_end(&mut bytes)).is_err() || bytes.len() as u64 != meta.len() {
            continue;
        }
        // A small Latin UI font cannot cover this path. Identify installed CJK faces from their
        // actual cmap rather than an OS filename, vendor list, or a list of particular user glyphs.
        if saved + meta.len() <= 64 << 20 {
            let bytes = Arc::new(bytes);
            let mut accepted = false;
            for index in 0..16 {
                let Ok(font) = FontRef::from_index(&bytes, index) else { break };
                if font.charmap().mappings().take(65_535).any(|(c, g)| g.to_u32() != 0 && char::from_u32(c).is_some_and(is_cjk))
                    && crate::embedding::validate_document_face(&font).is_ok()
                {
                    accepted = true;
                    faces.push(Bytes::Local { bytes: Arc::clone(&bytes), index });
                    if faces.len() >= 32 {
                        break;
                    }
                }
            }
            if accepted {
                saved += meta.len();
            }
        }
        if faces.len() >= 32 {
            break;
        }
    }
    faces
}
