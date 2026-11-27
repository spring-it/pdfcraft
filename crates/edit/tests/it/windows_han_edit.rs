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
    let replacement = "萨达，這頁的數據已更新。";
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

const EDITABLE_CN: &str = "萨达：沃飞翼览 PDF 數據已更新（第12頁）";

#[test]
fn new_page_text_and_header_round_trip_as_unicode() {
    if pdfcraft_fonts::ui_chinese_fonts().is_empty() {
        return;
    }
    let mut doc = styled_text_page("Helvetica");
    let text = pdfcraft_edit::AddedText { rect: [30.0, 100.0, 560.0, 145.0], text: EDITABLE_CN.into(), ..Default::default() };
    pdfcraft_edit::add_content(&mut doc, 0, &pdfcraft_edit::Content::Text(text)).unwrap();
    let mut header = pdfcraft_edit::HeaderFooter::default();
    header.text[0] = "核准：薩達，第 <<1>> 頁".into();
    pdfcraft_edit::add_header_footer(&mut doc, &[0], &header, false, &pdfcraft_edit::Context { date: (2026, 11, 26) }).unwrap();
    let saved = reopen(&doc);
    let lines = text::text_lines(&saved, 0).unwrap();
    let all = lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join("\n");
    assert!(all.contains(EDITABLE_CN), "{all:?}");
    assert!(all.contains("核准：薩達，第 1 頁"), "{all:?}");
    assert!(!all.contains('?'));
}

#[test]
fn typewriter_and_text_box_write_real_unicode_appearances() {
    use pdfcraft_annot::{Meta, NewAnnotation, Shape, Style, add_annotation, set_contents};
    use pdfcraft_cos::Object;
    if pdfcraft_fonts::ui_chinese_fonts().is_empty() {
        return;
    }
    let mut doc = styled_text_page("Helvetica");
    for shape in [
        Shape::Typewriter { rect: [30.0, 80.0, 560.0, 124.0], font_size: 12.0 },
        Shape::TextBox { rect: [30.0, 140.0, 560.0, 190.0], font_size: 12.0 },
    ] {
        let style = Style::default_for(&shape);
        let meta = Meta::default();
        let index =
            add_annotation(&mut doc, &NewAnnotation { page: 0, shape, style, contents: "待更新".into(), author: String::new() }, &meta).unwrap();
        set_contents(&mut doc, 0, index, EDITABLE_CN, &meta).unwrap();
    }
    let saved = reopen(&doc);
    let p = &pdfcraft_model::pages(&saved)[0].dict;
    let annotations = saved.resolve(p.get(b"Annots").unwrap());
    for annotation in annotations.as_array().unwrap() {
        let d = saved.dict(annotation).unwrap();
        assert_eq!(d.get(b"Contents").and_then(Object::as_string).unwrap().to_text(), EDITABLE_CN);
        assert!(pdfcraft_fonts::undrawn_mark(&d).is_none());
        let ap = saved.dict(d.get(b"AP").unwrap()).unwrap();
        let normal = saved.resolve(ap.get(b"N").unwrap());
        let Object::Stream(normal) = &*normal else {
            panic!("saved annotation must contain an appearance stream");
        };
        let operators = normal.decoded().unwrap();
        assert!(!operators.contains(&b'?'));
        let res = saved.dict(normal.dict.get(b"Resources").unwrap()).unwrap();
        let fonts = saved.dict(res.get(b"Font").unwrap()).unwrap();
        let subset = fonts.iter().find_map(|(_, font)| saved.dict(font).filter(|font| font.name(b"Subtype") == Some(b"Type3"))).unwrap();
        let cmap = saved.resolve(subset.get(b"ToUnicode").unwrap());
        let Object::Stream(cmap) = &*cmap else {
            panic!("the subset must map the original text");
        };
        let map = String::from_utf8(cmap.decoded().unwrap()).unwrap();
        for ch in EDITABLE_CN.chars().filter(|c| !c.is_ascii()) {
            assert!(map.contains(&format!("<{:04X}>", u32::from(ch))), "{ch}: {map}");
        }
    }
}

#[test]
fn chosen_local_face_is_saved_for_add_text_and_header_and_can_be_reedited() {
    use pdfcraft_edit::{AddedText, Content, Context, HeaderFooter};
    let Some(choice) =
        pdfcraft_fonts::ui_chinese_fonts().into_iter().find_map(|f| pdfcraft_fonts::embedding::ExplicitFace::new(f.bytes.to_vec()).ok())
    else {
        if std::env::var("CRAFT_FONTS_REQUIRED").as_deref() == Ok("1") {
            panic!("required build must ship an approved Chinese font fixture");
        }
        return;
    };
    let mut doc = styled_text_page("Helvetica");
    let t = AddedText { text: "核准文件 ABC".into(), rect: [30.0, 110.0, 555.0, 155.0], local_face: Some(choice.clone()), ..Default::default() };
    pdfcraft_edit::add_content(&mut doc, 0, &Content::Text(t)).unwrap();
    let mut header = HeaderFooter { local_face: Some(choice.clone()), ..Default::default() };
    header.text[0] = "第 <<1>> 頁 HDR".into();
    pdfcraft_edit::add_header_footer(&mut doc, &[0], &header, false, &Context { date: (2026, 11, 26) }).unwrap();
    let mut saved = reopen(&doc);
    let mut found = pdfcraft_edit::list_added(&saved).into_iter().next().unwrap();
    let Content::Text(t) = &mut found.content else { panic!("the saved item is text") };
    assert_eq!(t.local_face.as_ref().map(|f| f.name()), Some(choice.name()));
    let lines = text::text_lines(&saved, 0).unwrap();
    for phrase in ["核准文件 ABC", "第 1 頁 HDR"] {
        assert!(lines.iter().any(|l| l.text.contains(phrase) && l.font.starts_with("PCFull")), "{phrase}: {lines:?}");
    }
    t.text = "這是重新編輯".into();
    pdfcraft_edit::update_content(&mut saved, 0, 0, &found.content).unwrap();
    let changed = reopen(&saved);
    let lines = text::text_lines(&changed, 0).unwrap();
    assert!(lines.iter().any(|l| l.text == "這是重新編輯" && l.font.starts_with("PCFull")), "{lines:?}");
}

/// An installed Latin face need not contain Han for Add Text to save real Chinese glyphs.
#[test]
fn add_text_keeps_chosen_latin_font_and_falls_back_for_the_reported_chinese() {
    use pdfcraft_cos::Object;
    use pdfcraft_edit::{AddedText, Content};
    const CHINESE: &str = "我来测试个撒地方";
    if pdfcraft_fonts::ui_chinese_fonts().is_empty() {
        return;
    }
    let choice = pdfcraft_fonts::document_latin_fonts(true)
        .into_iter()
        .find_map(|f| pdfcraft_fonts::embedding::ExplicitFace::new(f.bytes.to_vec()).ok())
        .expect("an embedded Latin-only face");
    assert!(choice.first_missing("Testing ABC 123").is_none());
    assert_eq!(choice.first_missing(CHINESE), Some('我'), "same coverage distinction as Noto Sans Bold");
    let mut doc = styled_text_page("Helvetica");
    let make =
        |text: &str| AddedText { text: text.into(), rect: [30.0, 110.0, 555.0, 155.0], local_face: Some(choice.clone()), ..Default::default() };
    let t = make(CHINESE);
    assert_eq!(pdfcraft_edit::added::first_undrawable(&t), None, "the draft must permit Commit");
    assert_eq!(pdfcraft_edit::added::first_undrawable(&make("我🦄")), Some('🦄'), "a genuinely missing non-Han glyph is not hidden");
    pdfcraft_edit::add_content(&mut doc, 0, &Content::Text(t)).unwrap();

    let mut saved = reopen(&doc);
    let phrase = format!("Testing {CHINESE} ABC 123");
    let assert_roundtrip = |d: &Document, expected: &str, uses_selected_face: bool| {
        let item = pdfcraft_edit::list_added(d).pop().expect("the text item remains editable");
        let Content::Text(t) = item.content else { panic!("text item") };
        assert_eq!(t.local_face.as_ref(), Some(&choice), "the selected program survives even with no visible Latin");
        assert_eq!(t.text, expected);
        let lines = text::text_lines(d, 0).unwrap();
        assert!(lines.iter().any(|line| line.text == expected), "saved text extraction must retain {expected}: {lines:?}");
        let stream = d.get(item.obj);
        let Object::Stream(stream) = stream.as_ref() else { panic!("expected added text stream") };
        let content = String::from_utf8(stream.decoded().unwrap()).expect("ASCII PDF operators");
        assert!(!content.contains('?'));
        assert!(content.contains("/PCAr"), "fallback outlines are actually painted, not only stored as text: {content}");
        assert_eq!(content.contains("/PCFull"), uses_selected_face, "the selected font owns the visible Latin portion: {content}");
        let marker = d.dict(stream.dict.get(b"PCAdded").unwrap()).unwrap();
        let selected = d.dict(marker.get(b"PCTypeface").unwrap()).unwrap();
        assert_eq!(selected.name(b"BaseFont"), Some(choice.name().as_bytes()), "the marker must never point to the fallback");
        let page = &pdfcraft_model::pages(d)[0].dict;
        let resources = d.dict(page.get(b"Resources").unwrap()).unwrap();
        let fonts = d.dict(resources.get(b"Font").unwrap()).unwrap();
        let fallback = fonts.iter().find_map(|(key, f)| key.starts_with(b"PCAr").then(|| d.dict(f)).flatten()).expect("fallback font");
        let cmap = d.resolve(fallback.get(b"ToUnicode").unwrap());
        let Object::Stream(cmap) = cmap.as_ref() else { panic!("ToUnicode stream") };
        let map = String::from_utf8(cmap.decoded().unwrap()).unwrap();
        for c in CHINESE.chars() {
            assert!(map.contains(&format!("<{:04X}>", u32::from(c))), "fallback glyph {c} has a Unicode mapping");
        }
    };
    assert_roundtrip(&saved, CHINESE, false);
    pdfcraft_edit::update_content(&mut saved, 0, 0, &Content::Text(make(&phrase))).unwrap();
    let changed = reopen(&saved);
    assert_roundtrip(&changed, &phrase, true);
}

#[test]
fn existing_paragraph_keeps_selected_latin_face_and_saves_missing_chinese() {
    let Some(fallback) = pdfcraft_fonts::ui_chinese_fonts().into_iter().find(|f| {
        pdfcraft_fonts::embedding::covers(f.bytes, "ABC 我来测试个撒地方 123") && pdfcraft_fonts::embedding::validate_document_input(f.bytes).is_ok()
    }) else {
        return;
    };
    let latin = include_bytes!("../../../../assets/fonts/Inter-Regular.ttf");
    let mut doc = styled_text_page("Helvetica");
    let resource = text::register_text_font(&mut doc, 0, latin).unwrap();
    let style = text::BlockStyle { resource: Some(resource.clone()), local_han_font: Some(Arc::new(fallback.bytes.to_vec())), ..Default::default() };
    let phrase = "ABC 我来测试个撒地方 123";
    text::rewrite_block(&mut doc, 0, 0, Some(phrase), &style).unwrap();
    let after = reopen(&doc);
    let lines = text::text_lines(&after, 0).unwrap();
    assert_eq!(lines.iter().map(|l| l.text.as_str()).collect::<String>(), phrase);
    assert!(lines.iter().any(|l| l.font == resource && l.text.contains("ABC")), "{lines:?}");
    assert!(lines.iter().any(|l| l.font != resource && l.text.contains("我来")), "{lines:?}");
}
