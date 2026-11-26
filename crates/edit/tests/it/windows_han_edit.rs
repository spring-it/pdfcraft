//! Round-trip coverage of Chinese edits on heterogeneous Windows PDFs.

use pdfcraft_cos::{Document, SaveOptions, write_incremental};
use pdfcraft_edit::text;
use std::sync::Arc;

fn styled_text_page(base: &str) -> Document {
    let content = "BT /F1 12 Tf 72 700 Td (Original) Tj ET";
    let mut out = b"%PDF-1.7\n".to_vec();
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 600 800] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_owned(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
        format!("<< /Type /Font /Subtype /Type1 /BaseFont /{base} /Encoding /WinAnsiEncoding >>"),
    ];
    let mut offsets = Vec::new();
    for (i, object) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", i + 1).as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(b"xref\n0 6\n0000000000 65535 f \n");
    for offset in offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n").as_bytes());
    Document::open(Arc::new(out)).unwrap()
}

fn reopen(doc: &Document) -> Document {
    let bytes = write_incremental(doc, &SaveOptions::default()).unwrap();
    hayro_syntax::Pdf::new(bytes.clone()).expect("parses");
    Document::open(Arc::new(bytes)).unwrap()
}

/// A mixed or unidentified PDF font did not opt into the Chinese fallback before. Reopen the
/// saved output to check that the glyphs are real document text, not just present in the editor.
#[test]
fn unidentified_subset_can_save_the_reported_simplified_chinese_glyphs() {
    let replacement = "丁水岸东方艾丝袜姐";
    if pdfcraft_fonts::ui_chinese_fonts().is_empty() {
        return;
    }
    for route in 0..3 {
        let mut doc = styled_text_page("ABCDEF+UnknownSubset");
        match route {
            0 => {
                text::replace_line(&mut doc, 0, 0, replacement).unwrap();
            }
            1 => {
                text::replace_block(&mut doc, 0, 0, replacement).unwrap();
            }
            _ => {
                let style = text::BlockStyle {
                    ranges: vec![text::RangeStyle {
                        range: 0..replacement.len(),
                        family: None,
                        resource: None,
                        size: Some(12.0),
                        color: None,
                        underline: None,
                        char_spacing: None,
                        scale: None,
                    }],
                    ..Default::default()
                };
                text::rewrite_block(&mut doc, 0, 0, Some(replacement), &style).unwrap();
            }
        }
        let saved = reopen(&doc);
        let lines = text::text_lines(&saved, 0).unwrap();
        assert_eq!(lines.iter().map(|line| line.text.as_str()).collect::<String>(), replacement, "route {route}");
    }
}

#[test]
fn desktop_approved_font_supplies_missing_han_without_replacing_source_letters() {
    let Some(face) = pdfcraft_fonts::ui_chinese_fonts()
        .into_iter()
        .find(|f| pdfcraft_fonts::embedding::covers(f.bytes, "东方") && pdfcraft_fonts::embedding::validate_document_input(f.bytes).is_ok())
    else {
        return;
    };
    let mut doc = styled_text_page("Helvetica");
    let replacement = "Original 东方";
    let style = text::BlockStyle { local_han_font: Some(std::sync::Arc::new(face.bytes.to_vec())), ..Default::default() };
    text::rewrite_block(&mut doc, 0, 0, Some(replacement), &style).unwrap();
    let reopened = reopen(&doc);
    let lines = text::text_lines(&reopened, 0).unwrap();
    assert_eq!(lines.iter().map(|line| line.text.as_str()).collect::<String>(), replacement);
    assert_eq!(lines[0].base_font, "Helvetica");
    assert!(lines.iter().any(|line| line.font.starts_with("PCFull")), "the licensed desktop face should be used, not the bundled Type 3 fallback");
}
