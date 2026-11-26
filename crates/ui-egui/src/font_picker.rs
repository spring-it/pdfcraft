//! Shared desktop list for PDF font controls. Names are presentation only; a font is
//! selected by its actual installed file, never by matching the translated label.

use pdfcraft_engine::text_fonts::InstalledFace;

pub(crate) fn source_label(name: &str, fonts: &[InstalledFace], language: &str) -> String {
    let key = pdfcraft_fonts::embedding::face_key(name);
    let installed = fonts.iter().find(|f| pdfcraft_fonts::embedding::face_key(&f.names.postscript) == key);
    if let Some(font) = installed
        && matches!(language, "zh-hans" | "zh-hant")
        && font.label(language) != font.names.postscript
    {
        return font.label(language).to_owned();
    }
    // Familiar source PDF aliases can be localized even when that font isn't installed.
    if !matches!(language, "zh-hans" | "zh-hant") {
        return name.into();
    }
    match key.as_str() {
        "simsun" => "宋体",
        "nsimsun" => "新宋体",
        "simhei" => "黑体",
        "fangsong" | "fangsonggb2312" => "仿宋",
        "kaiti" | "kaitigb2312" => "楷体",
        "microsoftyahei" => "微软雅黑",
        "microsoftyaheibold" => "微软雅黑 粗体",
        "microsoftyaheilight" => "微软雅黑 细体",
        "mingliu" => "細明體",
        "pmingliu" => "新細明體",
        "microsoftjhenghei" => "微軟正黑體",
        _ => name,
    }
    .into()
}

pub(crate) fn label(ctx: &egui::Context, name: &str) -> String {
    let language = crate::i18n::current().code();
    #[cfg(not(target_arch = "wasm32"))]
    if matches!(language, "zh-hans" | "zh-hant")
        && let Some(fonts) = crate::files::installed_font_files(ctx, false)
    {
        return source_label(name, &fonts, language);
    }
    #[cfg(target_arch = "wasm32")]
    let _ = ctx;
    source_label(name, &[], language)
}

/// Use the chosen face inside the live text editor. egui installs changed definitions next
/// frame; show with the ordinary editor face until the named family actually exists.
pub(crate) fn preview_font(ctx: &egui::Context, face: Option<&pdfcraft_fonts::embedding::ExplicitFace>, fallback: egui::FontId) -> egui::FontId {
    let Some(face) = face else { return fallback };
    let name = format!("pdf-added-explicit-{:016x}", face.program_id());
    let family = egui::FontFamily::Name(name.clone().into());
    if ctx.fonts(|f| f.definitions().families.contains_key(&family)) {
        return egui::FontId::new(fallback.size, family);
    }
    let mut defs = ctx.fonts(|f| f.definitions().clone());
    defs.font_data.retain(|n, _| !n.starts_with("pdf-added-explicit-"));
    defs.families.retain(|f, _| !matches!(f, egui::FontFamily::Name(n) if n.starts_with("pdf-added-explicit-")));
    defs.font_data.insert(name.clone(), std::sync::Arc::new(egui::FontData::from_owned(face.bytes().to_vec())));
    let stack = std::iter::once(name).chain(defs.families.get(&fallback.family).into_iter().flatten().cloned()).collect();
    defs.families.insert(family, stack);
    ctx.set_fonts(defs);
    ctx.request_repaint();
    fallback
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn choices(ui: &mut egui::Ui) -> Option<InstalledFace> {
    list(ui, false)
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn raster_choices(ui: &mut egui::Ui) -> Option<InstalledFace> {
    list(ui, true)
}

#[cfg(not(target_arch = "wasm32"))]
fn list(ui: &mut egui::Ui, raster_only: bool) -> Option<InstalledFace> {
    let language = crate::i18n::current().code();
    let id = ui.scope_id().with("font-search");
    let mut query = ui.data(|d| d.get_temp::<String>(id)).unwrap_or_default();
    ui.separator();
    ui.horizontal(|ui| {
        let r = ui.add(egui::TextEdit::singleline(&mut query).id(id.with("field")).hint_text(tl!("Search fonts…")).desired_width(150.0));
        if r.changed() {
            ui.data_mut(|d| d.insert_temp(id, query.clone()));
        }
    });
    let reload = ui.small_button(tl!("Reload")).clicked();
    let Some(fonts) = crate::files::installed_font_files(ui.ctx(), reload) else {
        ui.label(tl!("Finding document fonts…"));
        return None;
    };
    let query = query.trim().to_lowercase();
    let mut shown = fonts
        .iter()
        .filter(|f| {
            query.is_empty()
                || [f.names.family.as_str(), f.names.postscript.as_str(), f.label("zh-hans"), f.label("zh-hant")]
                    .iter()
                    .any(|name| name.to_lowercase().contains(&query))
        })
        .collect::<Vec<_>>();
    shown.sort_by(|a, b| {
        b.names
            .can_embed
            .cmp(&a.names.can_embed)
            .then_with(|| a.label(language).cmp(b.label(language)))
            .then(a.names.postscript.cmp(&b.names.postscript))
    });
    if shown.is_empty() {
        ui.label(tl!("No matching installed fonts."));
        return None;
    }
    let mut picked = None;
    egui::ScrollArea::vertical().max_height(230.0).show(ui, |ui| {
        for font in shown {
            let allowed = if raster_only { font.names.can_rasterize } else { font.names.can_embed };
            let r = ui.add_enabled(allowed, egui::Button::selectable(false, font.label(language)));
            let r = if allowed {
                r.on_hover_text(&font.names.postscript)
            } else {
                r.on_disabled_hover_text(if raster_only {
                    tl!("This font cannot be used to draw the watermark.")
                } else {
                    tl!("This font cannot be embedded in a PDF.")
                })
            };
            if r.clicked() {
                picked = Some(font.clone());
                ui.close();
            }
        }
    });
    picked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installed_metadata_localizes_pdf_postscript_names_without_changing_the_family_key() {
        let face = InstalledFace {
            path: "face.ttc".into(),
            names: pdfcraft_fonts::embedding::DisplayFace {
                family: "Custom Text".into(),
                postscript: "CustomText-Regular".into(),
                chinese_simplified: Some("示例字体".into()),
                chinese_traditional: Some("範例字型".into()),
                face_index: 2,
                can_embed: true,
                can_rasterize: true,
            },
        };
        let fonts = [face];
        assert_eq!(source_label("ABCDEF+CustomText-Regular", &fonts, "zh-hans"), "示例字体");
        assert_eq!(source_label("ABCDEF+CustomText-Regular", &fonts, "zh-hant"), "範例字型");
        assert_eq!(source_label("ABCDEF+CustomText-Regular", &fonts, "en"), "ABCDEF+CustomText-Regular");
        assert_eq!(source_label("ABCDEF+SimSun", &fonts, "zh-hans"), "宋体");
        assert_eq!(fonts[0].names.face_index, 2);
    }
}
