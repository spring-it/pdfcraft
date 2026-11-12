//! Regression coverage for Fill & Sign settings without a window or GPU.

use pdfcraft_ui_egui::PdfCraftApp;
use serde_json::{Value, json};

#[test]
fn fill_text_colour_settings_preserve_other_preferences() {
    let mut app = PdfCraftApp::new();
    app.fill_text_color = [0.25, 0.5, 0.75];
    app.fill_text_size = 24.0;
    app.set_option("author", "Settings regression").unwrap();
    let settings = app.persist();
    let value: Value = serde_json::from_str(&settings).unwrap();
    assert_eq!(value["fill_text_color"], json!([0.25, 0.5, 0.75]));
    assert_eq!(value["fill_text_size"], json!(24.0));
    assert_eq!(value["author"], json!("Settings regression"));
    for key in ["docking", "comment_styles", "redact_prefs", "hide_empty_highlight_hover"] {
        assert!(value.get(key).is_some(), "missing existing setting: {key}");
    }
    let mut restored = PdfCraftApp::new();
    restored.restore(&settings);
    assert_eq!(restored.fill_text_color, app.fill_text_color);
    assert_eq!(restored.fill_text_size, app.fill_text_size);
    let value: Value = serde_json::from_str(&restored.persist()).unwrap();
    assert_eq!(value["author"], json!("Settings regression"));
}

#[test]
fn fill_text_colour_settings_round_trip_rgb_boundaries() {
    for color in [[0.0, 0.0, 0.0], [1.0, 1.0, 1.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]] {
        let mut app = PdfCraftApp::new();
        app.fill_text_color = color;
        let mut restored = PdfCraftApp::new();
        restored.restore(&app.persist());
        assert_eq!(restored.fill_text_color, color);
    }
}

#[test]
fn fill_text_colour_settings_keep_legacy_default() {
    let mut app = PdfCraftApp::new();
    let default_color = app.fill_text_color;
    app.restore(r#"{"fill_text_size":18.0}"#);
    assert_eq!(app.fill_text_size, 18.0);
    assert_eq!(app.fill_text_color, default_color);
}

#[test]
fn fill_text_colour_settings_ignore_malformed_rgb() {
    for invalid in [json!(null), json!("blue"), json!([1.0, 0.0]), json!([1.0, 0.0, "blue"])] {
        let mut app = PdfCraftApp::new();
        let color = [0.25, 0.5, 0.75];
        app.fill_text_color = color;
        app.restore(&json!({ "fill_text_color": invalid }).to_string());
        assert_eq!(app.fill_text_color, color);
    }
}
