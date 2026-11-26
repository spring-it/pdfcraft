//! Company links and support details are available without restoring upstream promotion.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pdfcraft_engine::links;
use pdfcraft_ui_egui::{Dialog, PdfCraftApp};

fn harness(setup: impl FnOnce(&mut PdfCraftApp) + 'static) -> Harness<'static, PdfCraftApp> {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        setup(&mut app);
        app
    });
    h.run_steps(4);
    h
}

#[test]
fn company_home_omits_the_upstream_discord_button() {
    let h = harness(|_| {});
    h.get_by_label("沃飞翼览 PDF");
    assert!(h.query_by_label("Discord").is_none());
}

#[test]
fn company_home_omits_upstream_external_promotion_links() {
    let h = harness(|_| {});
    h.get_by_label("沃飞翼览 PDF");
    for label in ["Join the ArtCraft community", "Join our Discord", "PdfCraft web page", "PdfCraft on GitHub", "ArtCraft website"] {
        assert!(h.query_by_label(label).is_none(), "{label} should not be rendered on the company home page");
    }
    assert_eq!(h.state().last_opened_url, None);
}

#[test]
fn company_home_omits_the_upstream_community_card() {
    let h = harness(|_| {});
    assert!(h.query_by_label("Join the ArtCraft community").is_none());
    assert!(h.query_by_label("Join our Discord").is_none());
    assert!(h.query_by_label("沃飞翼览 PDF").is_some());
}

#[test]
fn home_restores_company_intro_version_and_online_browser_link() {
    let mut h = harness(|_| {});
    h.get_by_label("欢迎使用");
    h.get_by_label(&format!("(v{})", pdfcraft_engine::build_info::version()));
    h.get_by_label("沃飞长空内部 PDF 阅读与处理工具 · 本地、安全、高效");
    h.get_by_label("沃飞翼览 PDF 支持本地离线处理，默认不上传文档，不需要账号。");
    assert!(h.state().last_opened_url.is_none(), "opening the desktop home must not launch the online app");
    settled(&mut h, "立即访问在线版").click();
    h.run_steps(3);
    assert_eq!(h.state().last_opened_url.as_deref(), Some(links::ONLINE));
}

#[test]
fn about_dialog_shows_the_company_brand() {
    let pdf = b"%PDF-1.7\n1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 >> endobj\n3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >> endobj\ntrailer << /Root 1 0 R >>\n%%EOF";
    // With a document open, so the home screen's own brand and links are not on screen.
    let h = harness(move |app| {
        app.open_bytes("one.pdf", None, pdf.to_vec()).unwrap();
        app.dialog = Some(Dialog::About);
    });
    assert!(h.query_all_by_label("沃飞翼览 PDF").count() >= 1, "the company wordmark");
    assert!(h.query_by_label("ArtCraft").is_none(), "the upstream brand is not shown");
    assert!(h.query_by_label("Join our Discord").is_none(), "the upstream Discord link is not shown");
    h.get_by_label("沃飞长空内部 PDF 阅读与处理工具");
    h.get_by_label("内部定制开发者信息");
    h.get_by_label("数字发展室");
}

#[test]
fn help_commands_open_each_link() {
    for l in links::LINKS {
        let mut h = harness(|_| {});
        assert!(h.state_mut().execute(l.command), "{}", l.command);
        assert_eq!(h.state().last_opened_url.as_deref(), Some(l.url));
        assert_eq!(pdfcraft_engine::commands::command(l.command).unwrap().menu, Some("Help"));
    }
}

/// Runs frames until the node labelled `label` exists and has stopped moving, then returns it.
///
/// The About dialog is a modal centred on its own size, and switching tabs changes that size, so
/// for a few frames afterwards the modal grows and recentres and every control in it moves. A
/// fixed number of frames is a guess at how long that takes: it was too few after the 0.5.0
/// contributors refresh (78 names instead of 5) on some machines, the click on Table landed where
/// the button had been, and the table never opened (#671).
fn settled<'a>(h: &'a mut Harness<'static, PdfCraftApp>, label: &'a str) -> egui_kittest::Node<'a> {
    let mut last = None;
    for _ in 0..60 {
        h.run_steps(1);
        let now = h.query_by_label(label).map(|n| n.rect());
        if now.is_some() && now == last {
            return h.get_by_label(label);
        }
        last = now;
    }
    panic!("{label:?} never appeared or never stopped moving");
}

#[test]
fn about_dialog_has_contributors_and_models_tabs() {
    let mut h = harness(|app| app.dialog = Some(Dialog::About));
    settled(&mut h, "Contributors").click();
    // The owner is always in the compiled-in credits (contributors/contributors.json), shown by username.
    settled(&mut h, "@echelon");
    settled(&mut h, "Table").click();
    settled(&mut h, "PRs");
    settled(&mut h, "Display name").click();
    settled(&mut h, "Brandon Thomas");
    settled(&mut h, "Models").click();
    h.run_steps(4);
    assert!(h.query_all_by_label("Anthropic").count() >= 1);
}
