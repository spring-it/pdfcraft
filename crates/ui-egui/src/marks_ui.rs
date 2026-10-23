//! Add Header and Footer, Add Watermark and Add Background dialogs (Acrobat: Edit a PDF ▸ …;
//! dialog snapshots d13, d14, d12d). Each has the shared Page Range Options and a schematic
//! preview of the current page. "Update" opens the same dialog in replace mode.

use egui::{Align, Color32, CornerRadius, Layout, Rect, Stroke, pos2, vec2};
use pdfcraft_engine::{Background, Edit, HeaderFooter, MarkKind, Watermark};

use crate::theme::{self, Tokens};
use crate::{PdfCraftApp, widgets};

/// How far a watermark may be moved from the page centre, in points.
const MAX_OFFSET: f64 = 20_000.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Subset {
    All,
    Even,
    Odd,
}

/// Page Range Options (shared by the three dialogs). Pages are 1-based.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageRange {
    pub all: bool,
    pub from: usize,
    pub to: usize,
    pub subset: Subset,
}

impl Default for PageRange {
    /// From the first page to the last: `to` starts past any page count, and the range field
    /// shows (and clamps it to) the document's last page.
    fn default() -> Self {
        Self { all: true, from: 1, to: usize::MAX, subset: Subset::All }
    }
}

impl PageRange {
    /// The pages (0-based) the range selects in a document of `count` pages.
    pub fn pages(&self, count: usize) -> Vec<usize> {
        let (a, b) = if self.all { (1, count) } else { (self.from.max(1), self.to.min(count)) };
        (a..=b)
            .filter(|p| match self.subset {
                Subset::All => true,
                Subset::Even => p % 2 == 0,
                Subset::Odd => p % 2 == 1,
            })
            .map(|p| p - 1)
            .collect()
    }

    pub(crate) fn ui(&mut self, ui: &mut egui::Ui, count: usize) {
        ui.horizontal_wrapped(|ui| {
            widgets::radio_value(ui, &mut self.all, true, tl!("All pages"));
            widgets::radio_value(ui, &mut self.all, false, tl!("Pages from"));
            ui.add_enabled(!self.all, egui::DragValue::new(&mut self.from).range(1..=count.max(1)));
            ui.label(tl!("to"));
            ui.add_enabled(!self.all, egui::DragValue::new(&mut self.to).range(1..=count.max(1)));
            ui.label(tl!("Subset"));
            let label = match self.subset {
                Subset::All => tl!("All pages in range"),
                Subset::Even => tl!("Even pages only"),
                Subset::Odd => tl!("Odd pages only"),
            };
            egui::ComboBox::from_id_salt("mark-subset").selected_text(label).show_ui(ui, |ui| {
                ui.selectable_value(&mut self.subset, Subset::All, tl!("All pages in range"));
                ui.selectable_value(&mut self.subset, Subset::Even, tl!("Even pages only"));
                ui.selectable_value(&mut self.subset, Subset::Odd, tl!("Odd pages only"));
            });
        });
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MarksDraft {
    pub hf: HeaderFooter,
    pub wm: Watermark,
    pub bg: Background,
    pub range: PageRange,
    /// Replace existing marks of this kind (Update).
    pub replace: bool,
    /// Which header/footer box Insert Page Number / Insert Date type into.
    pub focused_box: usize,
    pub page_format: usize,
    pub date_format: usize,
    /// Watermark: size the text to the page.
    pub fit: bool,
    /// Source ▸ File (watermark or background): the file picked, and the page of a PDF (1-based).
    pub use_file: bool,
    pub file: Option<(String, std::sync::Arc<Vec<u8>>)>,
    pub file_page: usize,
    pub font_family: usize,
    pub custom_enabled: bool,
    pub custom_pages: String,
}

impl Default for MarksDraft {
    fn default() -> Self {
        Self {
            hf: HeaderFooter::default(),
            wm: Watermark {
                text: "内部资料".into(),
                font_size: 15.0,
                opacity: 0.10,
                rotation: -45.0,
                color: [0.5, 0.5, 0.5],
                tile: true,
                tile_gap: [80.0, 120.0],
                offset: [0.0, 0.0],
                behind: false,
                ..Watermark::default()
            },
            bg: Background::default(),
            range: PageRange::default(),
            replace: false,
            focused_box: 1,
            page_format: 0,
            date_format: 0,
            fit: false,
            use_file: false,
            file: None,
            file_page: 1,
            font_family: 0,
            custom_enabled: false,
            custom_pages: String::new(),
        }
    }
}

const PAGE_FORMATS: [&str; 5] = ["<<1>>", "<<1 of n>>", "<<1/n>>", "<<Page 1>>", "<<Page 1 of n>>"];
const DATE_FORMATS: [&str; 6] = ["<<m/d/yyyy>>", "<<mm/dd/yyyy>>", "<<d/m/yyyy>>", "<<yyyy-mm-dd>>", "<<m/d/yy>>", "<<mmmm d, yyyy>>"];
const BOX_NAMES: [&str; 6] =
    ["Left Header Text", "Center Header Text", "Right Header Text", "Left Footer Text", "Center Footer Text", "Right Footer Text"];

fn color_button(ui: &mut egui::Ui, c: &mut [f64; 3]) {
    let mut f = c.map(|v| v as f32);
    if egui::color_picker::color_edit_button_rgb(ui, &mut f).changed() {
        *c = f.map(f64::from);
    }
}

/// A schematic of the current page with the marks laid out (no rendering needed).
fn preview(ui: &mut egui::Ui, t: &Tokens, size: (f64, f64), draw: impl FnOnce(&egui::Painter, Rect, f32)) {
    let (w, h) = (size.0.max(1.0), size.1.max(1.0));
    let s = (190.0 / w.max(h)) as f32;
    let (r, _) = ui.allocate_exact_size(vec2(210.0, 210.0), egui::Sense::hover());
    let page = Rect::from_center_size(r.center(), vec2(w as f32 * s, h as f32 * s));
    let painter = ui.painter_at(r);
    painter.rect_filled(page, CornerRadius::ZERO, Color32::WHITE);
    painter.rect_stroke(page, CornerRadius::ZERO, Stroke::new(1.0, t.border), egui::StrokeKind::Outside);
    draw(&painter, page, s);
}

fn rgb32(c: [f64; 3]) -> Color32 {
    let b = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgb(b(c[0]), b(c[1]), b(c[2]))
}

/// Target live-preview canvas height on standard-height windows.
const WATERMARK_PREVIEW_HEIGHT: f32 = 450.0;
/// Preserve room for the title, controls and action buttons on shorter windows.
const WATERMARK_PREVIEW_MIN_HEIGHT: f32 = 220.0;

fn watermark_preview_height(viewport_height: f32) -> f32 {
    (viewport_height - 500.0).clamp(WATERMARK_PREVIEW_MIN_HEIGHT, WATERMARK_PREVIEW_HEIGHT)
}

/// A responsive two-column watermark dialog: settings on the left, live preview on the right.
fn watermark_body(ui: &mut egui::Ui, app: &mut PdfCraftApp, t: &Tokens) -> (bool, bool) {
    let Some((i, id)) = app.active_ids() else { return (false, true) };
    let current = app.views[i].current;
    let Some(doc) = app.session.get(id) else { return (false, true) };
    let count = doc.info.pages.len();
    let page = doc.info.pages.get(current).map(|p| (p.width as f64, p.height as f64)).unwrap_or((612.0, 792.0));
    let d = &mut app.marks_draft;
    let mut browse = false;
    let title = if d.replace { "更新水印" } else { "添加水印" };
    ui.label(egui::RichText::new(title).font(theme::semibold(18.0)));
    ui.add_space(8.0);

    // Keep the modal content-sized: no expanding ScrollArea and no max height.
    // The live preview grows on standard windows and scales down on short viewports.
    let preview_height = watermark_preview_height(ui.ctx().content_rect().height());
    let available_width = ui.available_width();
    let right_width = (available_width - 660.0).clamp(140.0, 450.0);
    let left_width = (available_width - right_width - 20.0).clamp(180.0, 640.0);

    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_min_width(left_width);
            ui.set_max_width(left_width);
                ui.horizontal(|ui| {
                    ui.label("来源");
                    widgets::radio_value(ui, &mut d.use_file, false, "文本");
                    widgets::radio_value(ui, &mut d.use_file, true, "文件");
                });
                if d.use_file {
                    browse |= file_source(ui, d, MarkKind::Watermark);
                } else {
                    ui.label("文本");
                    ui.add(crate::widgets::block(&mut d.wm.text)
                        .desired_rows(2).desired_width((left_width - 50.0).max(180.0))
                        .hint_text("内部资料").id_salt("wm-text-wide"));
                    ui.horizontal_wrapped(|ui| {
                        ui.label("常用模板");
                        for (label, value) in [
                            ("内部资料", "内部资料"),
                            ("机密文件", "机密文件"),
                            ("仅供审阅", "仅供审阅"),
                            ("CONFIDENTIAL", "CONFIDENTIAL"),
                        ] {
                            if ui.small_button(label).clicked() { d.wm.text = value.to_string(); }
                        }
                    });
                }

                ui.horizontal(|ui| {
                    ui.label("排列方式");
                    widgets::radio_value(ui, &mut d.wm.tile, true, "平铺水印");
                    widgets::radio_value(ui, &mut d.wm.tile, false, "单个水印");
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label("字体");
                    egui::ComboBox::from_id_salt("wofei-watermark-font")
                        .selected_text(["微软雅黑", "黑体", "Arial", "Georgia", "等宽字体"][d.font_family.min(4)])
                        .show_ui(ui, |ui| {
                            for (i, label) in ["微软雅黑", "黑体", "Arial", "Georgia", "等宽字体"].iter().enumerate() {
                                ui.selectable_value(&mut d.font_family, i, *label);
                            }
                        });
                    ui.label("字号");
                    ui.add(egui::Slider::new(&mut d.wm.font_size, 8.0..=100.0).suffix(" pt"));
                    ui.label("颜色");
                    color_button(ui, &mut d.wm.color);
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label("旋转");
                    for angle in [-45.0, 0.0, 45.0] {
                        widgets::radio_value(ui, &mut d.wm.rotation, angle, if angle == 0.0 { "无".to_owned() } else { format!("{angle:.0}°") });
                    }
                    ui.add(egui::DragValue::new(&mut d.wm.rotation).range(-180.0..=180.0).suffix("°"));
                });
                ui.horizontal(|ui| {
                    ui.label("不透明度");
                    let mut pct = d.wm.opacity * 100.0;
                    if ui.add(egui::Slider::new(&mut pct, 0.0..=100.0).suffix("%")).changed() {
                        d.wm.opacity = pct / 100.0;
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label("位置");
                    widgets::radio_value(ui, &mut d.wm.behind, true, "显示在页面后面");
                    widgets::radio_value(ui, &mut d.wm.behind, false, "显示在页面上层");
                });
                if d.wm.tile {
                    ui.horizontal_wrapped(|ui| {
                        ui.label("横向间距");
                        ui.add(egui::Slider::new(&mut d.wm.tile_gap[0], 20.0..=260.0).suffix(" pt"));
                        ui.label("纵向间距");
                        ui.add(egui::Slider::new(&mut d.wm.tile_gap[1], 20.0..=260.0).suffix(" pt"));
                    });
                } else {
                    ui.horizontal(|ui| {
                        ui.label("水印位置");
                        egui::Grid::new("wofei-watermark-grid-wide").num_columns(3).spacing([6.0, 4.0]).show(ui, |ui| {
                            for (i, label) in ["左上", "顶部", "右上", "左侧", "居中", "右侧", "左下", "底部", "右下"].into_iter().enumerate() {
                                ui.selectable_value(&mut d.wm.position, i as u8, label);
                                if i % 3 == 2 { ui.end_row(); }
                            }
                        });
                    });
                }
                ui.horizontal_wrapped(|ui| {
                    ui.label("与中心的偏移");
                    ui.label("水平");
                    ui.add(egui::DragValue::new(&mut d.wm.offset[0]).range(-MAX_OFFSET..=MAX_OFFSET).suffix(" pt"));
                    ui.label("垂直");
                    ui.add(egui::DragValue::new(&mut d.wm.offset[1]).range(-MAX_OFFSET..=MAX_OFFSET).suffix(" pt"));
                });

                ui.add_space(8.0);
                ui.label(egui::RichText::new("页面范围选项").font(theme::semibold(12.5)));
                d.range.ui(ui, count);
                ui.checkbox(&mut d.custom_enabled, "自定义页码");
                if d.custom_enabled {
                    ui.add(egui::TextEdit::singleline(&mut d.custom_pages)
                        .desired_width((left_width - 40.0).max(180.0)).hint_text("例如 1-3, 5, 8-10"));
                    if selected_pages(d, count).is_err() {
                        ui.colored_label(Color32::RED, "页码范围无效，请检查起止页是否在文档范围内。");
                    }
                }
                ui.label(egui::RichText::new("支持中文水印：使用本机字体绘制透明 PNG，再写入 PDF；不上传文件或字体。")
                    .small().color(t.text_muted));
        });

        // Do not use ui.separator() in this horizontal layout: egui sizes a vertical
        // separator from the parent's available height, which can make Modal treat
        // the whole viewport as content and re-center the controls out of view.
        let divider_height = preview_height.min(ui.available_height());
        let (divider, _) = ui.allocate_exact_size(vec2(1.0, divider_height), egui::Sense::hover());
        ui.painter().line_segment(
            [divider.center_top(), divider.center_bottom()],
            Stroke::new(1.0, t.border),
        );

        ui.vertical(|right| {
            right.set_min_width(right_width);
            right.set_max_width(right_width);
            right.label(egui::RichText::new("实时预览").font(theme::semibold(15.0)));
            right.add_space(6.0);
            let file_name = d.file.as_ref().filter(|_| d.use_file).map(|f| f.0.as_str());
            watermark_preview(right, t, page, &d.wm, file_name, d.use_file, right_width, preview_height);
            right.add_space(6.0);
            right.label(egui::RichText::new(format!("预览：第 {} 页，共 {} 页", current + 1, count)).small().color(t.text_faint));
            let selected = selected_pages(d, count).map(|p| p.len()).unwrap_or(0);
            right.label(egui::RichText::new(format!("应用于 {selected} 页")).small().color(t.text_faint));
            if d.replace {
                right.label(egui::RichText::new("将替换这些页面上现有的水印。").small().color(t.text_faint));
            }
            right.label(egui::RichText::new("预览随设置实时更新；最终效果以保存后的 PDF 为准。")
                .small().color(t.text_muted));
        });
    });

    ui.add_space(8.0);
    let has_source = if d.use_file { d.file.is_some() } else { !d.wm.text.trim().is_empty() };
    let ready = has_source && selected_pages(d, count).is_ok_and(|pages| !pages.is_empty());
    let mut apply = false;
    let mut cancel = false;
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        if ui.add_enabled_ui(ready, |ui| widgets::pill_button(ui, tl!("OK"), true)).inner.clicked() {
            apply = true;
        }
        if widgets::pill_button(ui, tl!("Cancel"), false).clicked() {
            cancel = true;
        }
    });

    #[cfg(not(target_arch = "wasm32"))]
    if browse {
        let dialog = rfd::AsyncFileDialog::new()
            .add_filter(tl!("PDF or image"), &["pdf", "png", "jpg", "jpeg", "tif", "tiff", "gif", "bmp", "jp2", "j2k", "jpx"]);
        let epoch = app.dialog_epoch();
        app.ask_one(crate::pickers::Ask::File(dialog), None, move |app, p| {
            if app.dialog_epoch() != epoch { return; }
            app.marks_draft.file = std::fs::read(&p).ok().map(|b| (
                p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                std::sync::Arc::new(b),
            ));
        });
    }
    #[cfg(target_arch = "wasm32")]
    let _ = browse;

    (apply, cancel)
}

/// Large page preview used by the two-column watermark dialog.
fn watermark_preview(
    ui: &mut egui::Ui,
    t: &Tokens,
    size: (f64, f64),
    wm: &Watermark,
    file_name: Option<&str>,
    use_file: bool,
    canvas_width: f32,
    canvas_height: f32,
) {
    let (w, h) = (size.0.max(1.0), size.1.max(1.0));
    let canvas_size = vec2(canvas_width.max(1.0), canvas_height.max(0.0));
    let (canvas, _) = ui.allocate_exact_size(canvas_size, egui::Sense::hover());
    let painter = ui.painter_at(canvas);
    painter.rect_filled(canvas, CornerRadius::same(6), t.field);
    let scale = (((canvas_size.x - 24.0).max(1.0) as f64) / w)
        .min(((canvas_size.y - 24.0).max(1.0) as f64) / h) as f32;
    let page = Rect::from_center_size(canvas.center(), vec2(w as f32 * scale, h as f32 * scale));
    painter.rect_filled(page, CornerRadius::same(2), Color32::WHITE);
    painter.rect_stroke(page, CornerRadius::same(2), Stroke::new(1.0, t.border), egui::StrokeKind::Outside);

    let draw_content = || {
        painter.text(page.min + vec2(16.0, 20.0), egui::Align2::LEFT_CENTER, "PDF 文档预览",
            egui::FontId::proportional(10.0), Color32::from_gray(190));
        for row in 0..12 {
            let y = page.top() + 42.0 + row as f32 * ((page.height() - 60.0) / 12.0);
            let right = if row % 4 == 3 { page.right() - 60.0 } else { page.right() - 18.0 };
            painter.line_segment([pos2(page.left() + 18.0, y), pos2(right, y)],
                Stroke::new(1.0, Color32::from_gray(235)));
        }
    };
    let draw_mark = || {
        if use_file {
            if let Some(name) = file_name {
                let side = page.width().min(page.height()) * wm.scale as f32;
                let centre = page.center() + vec2(wm.offset[0] as f32 * scale, -wm.offset[1] as f32 * scale);
                let rect = Rect::from_center_size(centre, vec2(side, side));
                painter.rect_stroke(rect, CornerRadius::ZERO, Stroke::new(1.0, Color32::GRAY), egui::StrokeKind::Inside);
                painter.text(centre, egui::Align2::CENTER_CENTER, name, egui::FontId::proportional(10.0), Color32::GRAY);
            } else {
                painter.text(page.center(), egui::Align2::CENTER_CENTER, "请选择水印文件",
                    egui::FontId::proportional(12.0), Color32::GRAY);
            }
            return;
        }
        let text = if wm.text.trim().is_empty() { "内部资料" } else { wm.text.trim() };
        let size = (wm.font_size as f32 * scale).max(5.0);
        let galley = painter.layout_no_wrap(text.to_string(), egui::FontId::proportional(size),
            rgb32(wm.color).gamma_multiply(wm.opacity as f32));
        let angle = -(wm.rotation as f32).to_radians();
        let half = galley.size() / 2.0;
        let (sn, cs) = angle.sin_cos();
        let text_offset = vec2(half.x * cs - half.y * sn, half.x * sn + half.y * cs);
        let draw_one = |centre: egui::Pos2| {
            painter.add(egui::epaint::TextShape::new(centre - text_offset, galley.clone(), Color32::BLACK).with_angle(angle));
        };
        let clipped = painter.with_clip_rect(page);
        if wm.tile {
            let step_x = (galley.size().x + wm.tile_gap[0] as f32 * scale).max(24.0);
            let step_y = (galley.size().y + wm.tile_gap[1] as f32 * scale).max(24.0);
            let mut y = page.top() - step_y;
            let mut row = 0;
            while y < page.bottom() + step_y && row < 30 {
                let mut x = page.left() - step_x + if row % 2 == 0 { 0.0 } else { step_x / 2.0 };
                let mut col = 0;
                while x < page.right() + step_x && col < 30 {
                    clipped.add(egui::epaint::TextShape::new(pos2(x, y) - text_offset, galley.clone(), Color32::BLACK).with_angle(angle));
                    x += step_x;
                    col += 1;
                }
                y += step_y;
                row += 1;
            }
        } else {
            let col = usize::from(wm.position.min(8)) % 3;
            let row = usize::from(wm.position.min(8)) / 3;
            let xs = [page.left() + 32.0, page.center().x, page.right() - 32.0];
            let ys = [page.top() + 32.0, page.center().y, page.bottom() - 32.0];
            draw_one(pos2(xs[col] + wm.offset[0] as f32 * scale, ys[row] - wm.offset[1] as f32 * scale));
        }
    };
    if wm.behind { draw_mark(); draw_content(); } else { draw_content(); draw_mark(); }
}

/// The dialog for `kind`; returns (apply, cancel).
pub(crate) fn body(ui: &mut egui::Ui, app: &mut PdfCraftApp, t: &Tokens, kind: MarkKind) -> (bool, bool) {
    if kind == MarkKind::Watermark {
        return watermark_body(ui, app, t);
    }
    let Some((i, id)) = app.active_ids() else { return (false, true) };
    let current = app.views[i].current;
    let Some(doc) = app.session.get(id) else { return (false, true) };
    let count = doc.info.pages.len();
    let page = doc.info.pages.get(current).map(|p| (p.width as f64, p.height as f64)).unwrap_or((612.0, 792.0));
    let units = app.units;
    let d = &mut app.marks_draft;
    let mut browse = false;
    let verb = if d.replace { tl!("Update") } else { tl!("Add") };
    let title = match kind {
        MarkKind::HeaderFooter => crate::i18n::fmt(tl!("{verb} Header and Footer"), &[("verb", verb)]),
        MarkKind::Watermark => crate::i18n::fmt(tl!("{verb} Watermark"), &[("verb", verb)]),
        MarkKind::Background => crate::i18n::fmt(tl!("{verb} Background"), &[("verb", verb)]),
    };
    ui.label(egui::RichText::new(title).font(theme::semibold(18.0)));
    ui.add_space(8.0);
    match kind {
        MarkKind::HeaderFooter => {
            ui.horizontal(|ui| {
                ui.label(tl!("Font: Helvetica"));
                ui.label(tl!("Size"));
                ui.add(egui::DragValue::new(&mut d.hf.font_size).range(4.0..=72.0).speed(0.5));
                ui.add(widgets::checkbox(&mut d.hf.underline, tl!("Underline")));
                color_button(ui, &mut d.hf.color);
                ui.add_space(16.0);
                ui.label(format!("{} ({})", tl!("Margins"), units.label()));
                let per = units.per_point();
                for (label, k) in [(tl!("Top"), 0), (tl!("Bottom"), 1), (tl!("Left"), 2), (tl!("Right"), 3)] {
                    ui.label(label);
                    let mut v = d.hf.margins[k] * per;
                    // Up to 10 inches, whatever the unit.
                    let max = 720.0 * per;
                    if ui.add(egui::DragValue::new(&mut v).range(0.0..=max).speed(units.speed()).max_decimals(units.decimals())).changed() {
                        d.hf.margins[k] = v / per;
                    }
                }
            });
            ui.add_space(8.0);
            egui::Grid::new("hf-boxes").num_columns(3).spacing([10.0, 4.0]).show(ui, |ui| {
                for row in 0..2 {
                    for col in 0..3 {
                        ui.label(egui::RichText::new(tl!(BOX_NAMES[row * 3 + col])).small());
                    }
                    ui.end_row();
                    for col in 0..3 {
                        let k = row * 3 + col;
                        let r = ui.add_sized([220.0, 40.0], crate::widgets::block(&mut d.hf.text[k]).desired_rows(2).id_salt(("hf-box", k)));
                        if r.gained_focus() || r.has_focus() {
                            d.focused_box = k;
                        }
                    }
                    ui.end_row();
                }
            });
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("hf-page-format").selected_text(PAGE_FORMATS[d.page_format].trim_matches(['<', '>'])).show_ui(
                    ui,
                    |ui| {
                        for (k, f) in PAGE_FORMATS.iter().enumerate() {
                            ui.selectable_value(&mut d.page_format, k, f.trim_matches(['<', '>']));
                        }
                    },
                );
                if ui.button(tl!("Insert Page Number")).clicked() {
                    d.hf.text[d.focused_box].push_str(PAGE_FORMATS[d.page_format]);
                }
                ui.add_space(12.0);
                egui::ComboBox::from_id_salt("hf-date-format").selected_text(DATE_FORMATS[d.date_format].trim_matches(['<', '>'])).show_ui(
                    ui,
                    |ui| {
                        for (k, f) in DATE_FORMATS.iter().enumerate() {
                            ui.selectable_value(&mut d.date_format, k, f.trim_matches(['<', '>']));
                        }
                    },
                );
                if ui.button(tl!("Insert Date")).clicked() {
                    d.hf.text[d.focused_box].push_str(DATE_FORMATS[d.date_format]);
                }
                ui.add_space(12.0);
                ui.label(tl!("Start page number"));
                ui.add(egui::DragValue::new(&mut d.hf.start_number).range(1..=999_999));
            });
        }
        MarkKind::Watermark => {
            ui.horizontal(|ui| {
                ui.label(tl!("Source"));
                widgets::radio_value(ui, &mut d.use_file, false, tl!("Text"));
                widgets::radio_value(ui, &mut d.use_file, true, tl!("File"));
            });
            if d.use_file {
                browse |= file_source(ui, d, MarkKind::Watermark);
            }
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.add_enabled_ui(!d.use_file, |ui| {
                        ui.label(tl!("Text"));
                        ui.add(
                            crate::widgets::block(&mut d.wm.text).desired_rows(2).desired_width(320.0).hint_text("CONFIDENTIAL").id_salt("wm-text"),
                        );
                    });
                    if !d.use_file {
                        ui.horizontal_wrapped(|ui| {
                            ui.label("常用模板");
                            for (label, value) in [
                                ("内部资料", "内部资料"),
                                ("机密文件", "机密文件"),
                                ("仅供审阅", "仅供审阅"),
                                ("CONFIDENTIAL", "CONFIDENTIAL"),
                            ] {
                                if ui.small_button(label).clicked() { d.wm.text = value.to_string(); }
                            }
                        });
                    }
                    ui.horizontal(|ui| {
                        ui.label("排列方式");
                        widgets::radio_value(ui, &mut d.wm.tile, true, "平铺水印");
                        widgets::radio_value(ui, &mut d.wm.tile, false, "单个水印");
                    });
                    ui.horizontal(|ui| {
                        ui.label("字体");
                        egui::ComboBox::from_id_salt("wofei-watermark-font")
                            .selected_text(["微软雅黑", "黑体", "Arial", "Georgia", "等宽字体"][d.font_family.min(4)])
                            .show_ui(ui, |ui| {
                                for (i, label) in ["微软雅黑", "黑体", "Arial", "Georgia", "等宽字体"].iter().enumerate() {
                                    ui.selectable_value(&mut d.font_family, i, *label);
                                }
                            });
                        ui.label("字号");
                        ui.add(egui::Slider::new(&mut d.wm.font_size, 12.0..=100.0).suffix(" pt"));
                        ui.label("颜色");
                        color_button(ui, &mut d.wm.color);
                    });
                    ui.horizontal(|ui| {
                        ui.label(tl!("Rotation"));
                        for r in [-45.0, 0.0, 45.0] {
                            widgets::radio_value(ui, &mut d.wm.rotation, r, if r == 0.0 { tl!("None").to_string() } else { format!("{r}°") });
                        }
                        ui.add(egui::DragValue::new(&mut d.wm.rotation).range(-180.0..=180.0).suffix("°"));
                    });
                    ui.horizontal(|ui| {
                        ui.label(tl!("Opacity"));
                        let mut pct = d.wm.opacity * 100.0;
                        if ui.add(egui::Slider::new(&mut pct, 0.0..=100.0).suffix("%")).changed() {
                            d.wm.opacity = pct / 100.0;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label(tl!("Location"));
                        widgets::radio_value(ui, &mut d.wm.behind, true, tl!("Appear behind page"));
                        widgets::radio_value(ui, &mut d.wm.behind, false, tl!("Appear on top of page"));
                    });
                    if d.wm.tile {
                        ui.horizontal(|ui| {
                            ui.label("横向间距");
                            ui.add(egui::Slider::new(&mut d.wm.tile_gap[0], 20.0..=260.0).suffix(" pt"));
                            ui.label("纵向间距");
                            ui.add(egui::Slider::new(&mut d.wm.tile_gap[1], 20.0..=260.0).suffix(" pt"));
                        });
                    } else {
                        ui.horizontal(|ui| {
                            ui.label("水印位置");
                            egui::Grid::new("wofei-watermark-grid").num_columns(3).show(ui, |ui| {
                                for (i, label) in ["左上", "顶部", "右上", "左侧", "居中", "右侧", "左下", "底部", "右下"]
                                    .into_iter().enumerate()
                                {
                                    ui.selectable_value(&mut d.wm.position, i as u8, label);
                                    if i % 3 == 2 { ui.end_row(); }
                                }
                            });
                        });
                    }
                    ui.horizontal(|ui| {
                        ui.label(tl!("Offset from centre"));
                        ui.label(tl!("Horizontal"));
                        ui.add(egui::DragValue::new(&mut d.wm.offset[0]).range(-MAX_OFFSET..=MAX_OFFSET).suffix(" pt"));
                        ui.label(tl!("Vertical"));
                        ui.add(egui::DragValue::new(&mut d.wm.offset[1]).range(-MAX_OFFSET..=MAX_OFFSET).suffix(" pt"));
                    });
                });
            });
        }
        MarkKind::Background => {
            ui.horizontal(|ui| {
                widgets::radio_value(ui, &mut d.use_file, false, tl!("From colour"));
                ui.add_enabled_ui(!d.use_file, |ui| color_button(ui, &mut d.bg.color));
                ui.add_space(12.0);
                widgets::radio_value(ui, &mut d.use_file, true, tl!("File"));
            });
            if d.use_file {
                browse |= file_source(ui, d, MarkKind::Background);
            }
            ui.horizontal(|ui| {
                ui.label(tl!("Opacity"));
                let mut pct = d.bg.opacity * 100.0;
                if ui.add(egui::Slider::new(&mut pct, 0.0..=100.0).suffix("%")).changed() {
                    d.bg.opacity = pct / 100.0;
                }
            });
        }
    }
    ui.add_space(8.0);
    ui.label(egui::RichText::new(tl!("Page Range Options")).font(theme::semibold(12.5)));
    d.range.ui(ui, count);
    if kind == MarkKind::Watermark {
        ui.horizontal(|ui| {
            ui.checkbox(&mut d.custom_enabled, "自定义页码");
            if d.custom_enabled {
                ui.add(egui::TextEdit::singleline(&mut d.custom_pages).desired_width(240.0)
                    .hint_text("例如 1-3, 5, 8-10"));
            }
        });
        if d.custom_enabled && selected_pages(d, count).is_err() {
            ui.colored_label(egui::Color32::RED, "页码范围无效，请输入 1-3, 5, 8-10 格式。");
        }
        ui.label(egui::RichText::new("支持中文水印：使用本机字体绘制透明 PNG，再写入 PDF；不上传文件或字体。")
            .small().color(t.text_muted));
    }
    ui.add_space(8.0);
    // Preview of the current page.
    let hf = d.hf.clone();
    let (wm, bg, fit) = (d.wm.clone(), d.bg.clone(), d.fit);
    let file_name = d.use_file.then(|| d.file.as_ref().map(|f| f.0.clone())).flatten();
    ui.horizontal(|ui| {
        preview(ui, t, page, |p, r, s| match kind {
            MarkKind::HeaderFooter => {
                for (k, text) in hf.text.iter().enumerate() {
                    if text.trim().is_empty() {
                        continue;
                    }
                    let shown = text.replace("<<", "").replace(">>", "");
                    let y = if k < 3 { r.top() + hf.margins[0] as f32 * s } else { r.bottom() - hf.margins[1] as f32 * s };
                    let (x, align) = match k % 3 {
                        0 => (r.left() + hf.margins[2] as f32 * s, egui::Align2::LEFT_CENTER),
                        1 => (r.center().x, egui::Align2::CENTER_CENTER),
                        _ => (r.right() - hf.margins[3] as f32 * s, egui::Align2::RIGHT_CENTER),
                    };
                    p.text(pos2(x, y), align, shown, egui::FontId::proportional((hf.font_size as f32 * s).max(5.0)), rgb32(hf.color));
                }
            }
            MarkKind::Watermark if file_name.is_some() => {
                let side = r.width().min(r.height()) * wm.scale as f32;
                let centre = r.center() + vec2(wm.offset[0] as f32 * s, -wm.offset[1] as f32 * s);
                p.rect_stroke(
                    Rect::from_center_size(centre, vec2(side, side)),
                    CornerRadius::ZERO,
                    egui::Stroke::new(1.0, Color32::GRAY),
                    egui::StrokeKind::Inside,
                );
                p.text(centre, egui::Align2::CENTER_CENTER, file_name.clone().unwrap_or_default(), egui::FontId::proportional(10.0), Color32::GRAY);
            }
            MarkKind::Watermark => {
                let text = if wm.text.trim().is_empty() { "内部资料" } else { wm.text.trim() };
                let size = if fit {
                    (r.width().hypot(r.height()) * 0.5 / (text.chars().count().max(1) as f32 * 0.55)).clamp(6.0, 80.0)
                } else { (wm.font_size as f32 * s).max(5.0) };
                let color = rgb32(wm.color).gamma_multiply(wm.opacity as f32);
                let galley = p.layout_no_wrap(text.to_string(), egui::FontId::proportional(size), color);
                let angle = -(wm.rotation as f32).to_radians();
                let half = galley.size() / 2.0;
                let (sn, cs) = angle.sin_cos();
                let offset = vec2(half.x * cs - half.y * sn, half.x * sn + half.y * cs);
                let painter = p.with_clip_rect(r);
                let draw = |centre: egui::Pos2| {
                    painter.add(egui::epaint::TextShape::new(centre - offset, galley.clone(), Color32::BLACK).with_angle(angle));
                };
                if wm.tile {
                    let step_x = (galley.size().x + wm.tile_gap[0] as f32 * s).max(30.0);
                    let step_y = (galley.size().y + wm.tile_gap[1] as f32 * s).max(30.0);
                    let mut y = r.top() - step_y / 2.0;
                    let mut row = 0;
                    while y < r.bottom() + step_y && row < 40 {
                        let mut x = r.left() - step_x / 2.0 + if row % 2 == 0 { 0.0 } else { step_x / 2.0 };
                        let mut col = 0;
                        while x < r.right() + step_x && col < 40 {
                            draw(pos2(x, y));
                            x += step_x;
                            col += 1;
                        }
                        y += step_y;
                        row += 1;
                    }
                } else {
                    let col = usize::from(wm.position.min(8)) % 3;
                    let row = usize::from(wm.position.min(8)) / 3;
                    let xs = [r.left() + 20.0, r.center().x, r.right() - 20.0];
                    let ys = [r.top() + 20.0, r.center().y, r.bottom() - 20.0];
                    draw(pos2(xs[col] + wm.offset[0] as f32 * s, ys[row] - wm.offset[1] as f32 * s));
                }
            }
            MarkKind::Background if file_name.is_some() => {
                p.rect_stroke(r.shrink(6.0), CornerRadius::ZERO, egui::Stroke::new(1.0, Color32::GRAY), egui::StrokeKind::Inside);
                p.text(
                    r.center(),
                    egui::Align2::CENTER_CENTER,
                    file_name.clone().unwrap_or_default(),
                    egui::FontId::proportional(10.0),
                    Color32::GRAY,
                );
            }
            MarkKind::Background => {
                p.rect_filled(r, CornerRadius::ZERO, rgb32(bg.color).gamma_multiply(bg.opacity as f32));
            }
        });
        ui.vertical(|ui| {
            ui.label(
                egui::RichText::new(crate::i18n::fmt(
                    tl!("Preview: page {p} of {n}"),
                    &[("p", &(current + 1).to_string()), ("n", &count.to_string())],
                ))
                .small()
                .color(t.text_faint),
            );
            let n = selected_pages(d, count).map(|p| p.len()).unwrap_or(0);
            let applies =
                if n == 1 { tl!("Applies to 1 page").to_string() } else { crate::i18n::fmt(tl!("Applies to {n} pages"), &[("n", &n.to_string())]) };
            ui.label(egui::RichText::new(applies).small().color(t.text_faint));
            if d.replace {
                ui.label(egui::RichText::new(tl!("Replaces the existing one on those pages.")).small().color(t.text_faint));
            }
        });
    });
    // Text the standard fonts can't draw is refused on apply (#125), so it's refused here, with the
    // preview (drawn in the interface font, which can draw it) no longer the only answer (#2013).
    let undrawable = undrawable(kind, d);
    if let Some(c) = undrawable {
        ui.add_space(6.0);
        ui.label(egui::RichText::new(crate::content_ui::undrawable_message(c)).small().color(crate::content_ui::UNDRAWABLE));
    }
    ui.add_space(10.0);
    let (mut apply, mut cancel) = (false, false);
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        let ready = undrawable.is_none()
            && match kind {
                MarkKind::HeaderFooter => d.hf.text.iter().any(|x| !x.trim().is_empty()),
                MarkKind::Watermark => {
                    if d.use_file {
                        d.file.is_some()
                    } else {
                        !d.wm.text.trim().is_empty()
                    }
                }
                MarkKind::Background => !d.use_file || d.file.is_some(),
            }
            && selected_pages(d, count).is_ok_and(|pages| !pages.is_empty());
        if ui.add_enabled_ui(ready, |ui| widgets::pill_button(ui, tl!("OK"), true)).inner.clicked() {
            apply = true;
        }
        if widgets::pill_button(ui, tl!("Cancel"), false).clicked() {
            cancel = true;
        }
    });
    // Browse…: the picker answers on a later frame, into the draft.
    #[cfg(not(target_arch = "wasm32"))]
    if browse {
        let dialog = rfd::AsyncFileDialog::new()
            .add_filter(tl!("PDF or image"), &["pdf", "png", "jpg", "jpeg", "tif", "tiff", "gif", "bmp", "jp2", "j2k", "jpx"]);
        let epoch = app.dialog_epoch();
        app.ask_one(crate::pickers::Ask::File(dialog), None, move |app, p| {
            // Only into the dialog that asked: not one closed meanwhile, or opened again since.
            if app.dialog_epoch() != epoch {
                return;
            }
            app.marks_draft.file =
                std::fs::read(&p).ok().map(|b| (p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(), std::sync::Arc::new(b)));
        });
    }
    #[cfg(target_arch = "wasm32")]
    let _ = browse;
    (apply, cancel)
}

/// The first character of the dialog's text that applying would refuse (the standard fonts can't
/// draw it), as the engine decides it. Pictures draw no text.
fn undrawable(kind: MarkKind, d: &MarksDraft) -> Option<char> {
    match kind {
        MarkKind::HeaderFooter => d.hf.first_undrawable(),
        MarkKind::Watermark if !d.use_file => None, // Unicode watermark text becomes a PNG.
        MarkKind::Watermark | MarkKind::Background => None,
    }
}

fn mark_file(d: &MarksDraft) -> Option<pdfcraft_engine::MarkFile> {
    let (name, bytes) = d.file.clone().filter(|_| d.use_file)?;
    Some(pdfcraft_engine::MarkFile { name, bytes, page: d.file_page.max(1) - 1 })
}

/// Source ▸ File: Browse…, the page of a PDF, and the size relative to the page. Returns true
/// when Browse… was clicked (the caller shows the picker once it no longer borrows the draft).
fn file_source(ui: &mut egui::Ui, d: &mut MarksDraft, kind: MarkKind) -> bool {
    // Browse… is desktop-only.
    #[cfg_attr(target_arch = "wasm32", allow(unused_mut))]
    let mut browse = false;
    ui.horizontal(|ui| {
        #[cfg(not(target_arch = "wasm32"))]
        if ui.button(tl!("Browse…")).clicked() {
            browse = true;
        }
        ui.label(d.file.as_ref().map(|f| f.0.clone()).unwrap_or_else(|| tl!("No file chosen").to_string()));
        if d.file.as_ref().is_some_and(|f| f.1.starts_with(b"%PDF")) {
            ui.label(tl!("Page number"));
            ui.add(egui::DragValue::new(&mut d.file_page).range(1..=9999));
        }
    });
    ui.horizontal(|ui| {
        ui.label(tl!("Scale relative to target page"));
        let scale = if kind == MarkKind::Background { &mut d.bg.scale } else { &mut d.wm.scale };
        let mut pct = *scale * 100.0;
        if ui.add(egui::Slider::new(&mut pct, 5.0..=100.0).suffix("%")).changed() {
            *scale = pct / 100.0;
        }
    });
    browse
}

/// Parse 1-based custom page ranges, rejecting invalid syntax and out-of-range pages.
fn selected_pages(d: &MarksDraft, count: usize) -> Result<Vec<usize>, String> {
    if !d.custom_enabled { return Ok(d.range.pages(count)); }
    let mut pages = std::collections::BTreeSet::new();
    let text = d.custom_pages.replace('，', ",").replace(' ', "");
    if text.is_empty() { return Err("请输入页码范围".into()); }
    for part in text.split(',') {
        let (a, b) = if let Some((a, b)) = part.split_once('-') {
            (a.parse::<usize>().map_err(|_| "无效起始页码")?, b.parse::<usize>().map_err(|_| "无效结束页码")?)
        } else {
            let p = part.parse::<usize>().map_err(|_| "无效页码")?;
            (p, p)
        };
        if a < 1 || b < a || b > count { return Err("页码超出文档范围".into()); }
        for p in a..=b { pages.insert(p - 1); }
    }
    if pages.is_empty() { return Err("没有选中任何页面".into()); }
    Ok(pages.into_iter().collect())
}

/// Render the text into a transparent PNG, as in the user's HTML watermark example.
fn watermark_file(d: &MarksDraft) -> Result<Option<pdfcraft_engine::MarkFile>, String> {
    if d.use_file { return Ok(mark_file(d)); }
    let png = crate::watermark_text::png(&d.wm.text, d.wm.font_size, d.wm.color, d.font_family)?;
    Ok(Some(pdfcraft_engine::MarkFile {
        name: "wofei-watermark.png".into(),
        bytes: std::sync::Arc::new(png),
        page: 0,
    }))
}

/// The edit the dialog's OK makes. Fail before mutating any PDF on invalid font/pages.
pub fn edit(d: &MarksDraft, kind: MarkKind, count: usize) -> Result<Edit, String> {
    let pages = if kind == MarkKind::Watermark { selected_pages(d, count)? } else { d.range.pages(count) };
    match kind {
        MarkKind::HeaderFooter => Ok(Edit::AddHeaderFooter { pages, settings: d.hf.clone(), replace: d.replace }),
        MarkKind::Watermark => {
            let mut wm = d.wm.clone();
            if !d.use_file { wm.raster_dpi = crate::watermark_text::DPI; }
            Ok(Edit::AddWatermark { pages, settings: wm, replace: d.replace, file: watermark_file(d)? })
        }
        MarkKind::Background => Ok(Edit::AddBackground { pages, settings: d.bg.clone(), replace: d.replace, file: mark_file(d) }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_applying_would_refuse_is_caught_in_the_dialog() {
        let mut d = MarksDraft::default();
        d.hf.text[5] = "Страница <<1>> из <<n>> / Page <<1>> of <<n>>".into();
        assert_eq!(undrawable(MarkKind::HeaderFooter, &d), Some('С'), "#2013: refused before OK, not after");
        d.hf.text[5] = "Page <<1>> of <<n>> — café".into();
        assert_eq!(undrawable(MarkKind::HeaderFooter, &d), None);
        d.wm.text = "ΑΠΟΡΡΗΤΟ".into();
        assert_eq!(undrawable(MarkKind::Watermark, &d), None, "Unicode text is rasterized");
        d.use_file = true;
        assert_eq!(undrawable(MarkKind::Watermark, &d), None, "a picture watermark draws no text");
        assert_eq!(undrawable(MarkKind::Background, &d), None);
    }

    #[test]
    fn watermark_dialog_preview_scales_to_viewport_without_forcing_modal_height() {
        for viewport_height in [480.0_f32, 600.0, 720.0, 900.0, 1080.0] {
            let preview = watermark_preview_height(viewport_height);
            assert!(
                (WATERMARK_PREVIEW_MIN_HEIGHT..=WATERMARK_PREVIEW_HEIGHT).contains(&preview),
                "preview height {preview} must stay within bounds for viewport {viewport_height}"
            );
        }
        assert_eq!(watermark_preview_height(900.0), 400.0);
        assert_eq!(watermark_preview_height(1080.0), WATERMARK_PREVIEW_HEIGHT);
        assert_eq!(watermark_preview_height(480.0), WATERMARK_PREVIEW_MIN_HEIGHT);
    }

    #[test]
    fn watermark_dialog_defaults_match_reference() {
        let d = MarksDraft::default();
        assert_eq!(d.wm.text, "内部资料");
        assert_eq!(d.wm.font_size, 15.0);
        assert_eq!(d.wm.opacity, 0.10);
        assert_eq!(d.wm.rotation, -45.0);
        assert_eq!(d.wm.color, [0.5, 0.5, 0.5]);
        assert!(d.wm.tile);
        assert_eq!(d.wm.tile_gap, [80.0, 120.0]);
        assert_eq!(d.wm.offset, [0.0, 0.0]);
        assert!(!d.wm.behind);
        assert_eq!(d.font_family, 0);
    }

    #[test]
    fn page_ranges_with_subsets() {
        let r = PageRange::default();
        assert_eq!(r.pages(4), [0, 1, 2, 3]);
        let r = PageRange { all: false, from: 2, to: 9, subset: Subset::Even };
        assert_eq!(r.pages(6), [1, 3, 5]);
        let r = PageRange { all: true, subset: Subset::Odd, ..PageRange::default() };
        assert_eq!(r.pages(5), [0, 2, 4]);
        // Not all pages: by default up to the last one; a single page is a range too.
        let r = PageRange { all: false, ..PageRange::default() };
        assert_eq!(r.pages(4), [0, 1, 2, 3]);
        let r = PageRange { all: false, from: 1, to: 1, subset: Subset::All };
        assert_eq!(r.pages(38), [0]);
    }

    #[test]
    fn strict_custom_page_range_works_with_chinese_watermark() {
        let mut d = MarksDraft::default();
        d.custom_enabled = true;
        d.custom_pages = "1-3， 5, 8-10".to_owned();
        assert_eq!(selected_pages(&d, 10).unwrap(), [0, 1, 2, 4, 7, 8, 9]);
        d.custom_pages = "11".to_owned();
        assert!(selected_pages(&d, 10).is_err());
        d.custom_pages = "4-2".to_owned();
        assert!(selected_pages(&d, 10).is_err());
    }

    #[test]
    fn the_watermark_edit_carries_the_offset() {
        let mut d = MarksDraft::default();
        d.wm.text = "DRAFT".into();
        d.wm.offset = [-72.0, 36.0];
        match edit(&d, MarkKind::Watermark, 3).unwrap() {
            Edit::AddWatermark { settings, .. } => assert_eq!(settings.offset, [-72.0, 36.0]),
            other => panic!("expected AddWatermark, got {other:?}"),
        }
    }
}
