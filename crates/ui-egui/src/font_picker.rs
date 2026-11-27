//! Shared desktop list for PDF font controls. Names are presentation only; a font is
//! selected by its actual installed file, never by matching the translated label.

mod names;
#[cfg(all(test, not(target_arch = "wasm32")))]
mod popup_tests;

use pdfcraft_engine::text_fonts::InstalledFace;
use pdfcraft_fonts::embedding::{DisplayFace, ExplicitFace, face_key};

/// Search, reload and scrolling are interactions inside a font menu, not selections.
/// The font rows (and the callers' built-in choices) close explicitly after selection.
pub(crate) fn combo(id: &'static str) -> craft_ui::ComboBox<'static> {
    craft_ui::ComboBox::from_id_salt(id).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
}

pub(crate) fn face_label(face: &DisplayFace, language: &str) -> String {
    let localizations = match language {
        "zh-hans" => [&face.chinese_simplified, &face.chinese_traditional],
        "zh-hant" => [&face.chinese_traditional, &face.chinese_simplified],
        _ => return face.postscript.clone(),
    };
    localizations
        .into_iter()
        .filter_map(Option::as_deref)
        .find(|name| names::han(name))
        .map(str::to_owned)
        .unwrap_or_else(|| names::fallback(&face.postscript, Some(&face.family), language == "zh-hant"))
}

pub(crate) fn source_label(name: &str, fonts: &[InstalledFace], language: &str) -> String {
    if !matches!(language, "zh-hans" | "zh-hant") || name.is_empty() {
        return name.into();
    }
    let key = face_key(name);
    if !key.is_empty()
        && let Some(font) = fonts.iter().find(|f| face_key(&f.names.postscript) == key)
    {
        return face_label(&font.names, language);
    }
    names::fallback(name, None, language == "zh-hant")
}

pub(crate) fn preset(name: &str) -> String {
    source_label(name, &[], crate::i18n::current().code())
}

/// A font from this app's own saved PDF resource may no longer be installed. Read its
/// official metadata once per face/language instead of needing a matching local file.
pub(crate) fn explicit_label(ctx: &egui::Context, face: &ExplicitFace) -> String {
    let language = crate::i18n::current().code();
    if !matches!(language, "zh-hans" | "zh-hant") {
        return face.name().into();
    }
    let id = egui::Id::new(("explicit-font-label", face.program_id(), language));
    if let Some(label) = ctx.data(|d| d.get_temp::<String>(id)) {
        return label;
    }
    let label = pdfcraft_fonts::embedding::display_faces(face.bytes(), 1)
        .first()
        .map_or_else(|| source_label(face.name(), &[], language), |names| face_label(names, language));
    ctx.data_mut(|d| d.insert_temp(id, label.clone()));
    label
}

pub(crate) fn installed_label(font: &DisplayFace) -> String {
    face_label(font, crate::i18n::current().code())
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
    list_with(ui, raster_only, crate::files::installed_font_files)
}

#[cfg(not(target_arch = "wasm32"))]
fn list_with(
    ui: &mut egui::Ui,
    raster_only: bool,
    installed: impl FnOnce(&egui::Context, bool) -> Option<std::sync::Arc<Vec<InstalledFace>>>,
) -> Option<InstalledFace> {
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
    let Some(fonts) = installed(ui.ctx(), reload) else {
        ui.label(tl!("Finding document fonts…"));
        return None;
    };
    let query = query.trim().to_lowercase();
    let mut shown = fonts
        .iter()
        .map(|face| (face, face_label(&face.names, language)))
        .filter(|(f, label)| {
            query.is_empty()
                || [f.names.family.as_str(), f.names.postscript.as_str(), label, f.label("zh-hans"), f.label("zh-hant")]
                    .iter()
                    .any(|name| name.to_lowercase().contains(&query))
        })
        .collect::<Vec<_>>();
    let allowed = |font: &InstalledFace| if raster_only { font.names.can_rasterize } else { font.names.can_embed };
    shown.sort_by(|(a, a_label), (b, b_label)| {
        allowed(b).cmp(&allowed(a)).then_with(|| a_label.cmp(b_label)).then(a.names.postscript.cmp(&b.names.postscript))
    });
    if shown.is_empty() {
        ui.label(tl!("No matching installed fonts."));
        return None;
    }
    let mut picked = None;
    egui::ScrollArea::vertical().max_height(230.0).show(ui, |ui| {
        for (font, label) in shown {
            let allowed = allowed(font);
            let r = ui.add_enabled(allowed, egui::Button::selectable(false, label));
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

    #[test]
    fn english_only_installed_face_uses_the_same_chinese_label_in_rows_and_selected_fields() {
        let f = InstalledFace {
            path: "fonts/dejavu.ttf".into(),
            names: DisplayFace {
                family: "DejaVu Sans".into(),
                postscript: "DejaVuSans-BoldOblique".into(),
                chinese_simplified: Some("DejaVu Sans".into()),
                chinese_traditional: None,
                face_index: 0,
                can_embed: true,
                can_rasterize: true,
            },
        };
        let shown = face_label(&f.names, "zh-hans");
        assert_eq!(shown, "德雅无衬线体 粗倾斜体");
        assert_eq!(source_label("DejaVuSans-BoldOblique", &[f], "zh-hans"), shown);
        assert_eq!(source_label("DejaVuSans-BoldOblique", &[], "en"), "DejaVuSans-BoldOblique");
    }
}
