//! Use physical pointer events, since popup click-away handling does not use
//! AccessKit's synthetic action click on the TextEdit.

use std::sync::Arc;

use egui::accesskit::Role;
use egui::{Event, Key, Modifiers, PointerButton, Pos2};
use egui_kittest::{Harness, kittest::Queryable};

use super::{DisplayFace, InstalledFace, combo, list_with};

#[derive(Default)]
struct State {
    selected: Option<String>,
    reloads: usize,
}

fn face(postscript: &str) -> InstalledFace {
    InstalledFace {
        path: format!("{postscript}.ttf").into(),
        names: DisplayFace {
            family: postscript.into(),
            postscript: postscript.into(),
            chinese_simplified: None,
            chinese_traditional: None,
            face_index: 0,
            can_embed: true,
            can_rasterize: true,
        },
    }
}

fn click(h: &mut Harness<'_, State>, at: Pos2) {
    h.hover_at(at);
    h.run_steps(1);
    h.drag_at(at);
    h.run_steps(1);
    h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(3);
}

fn click_label(h: &mut Harness<'_, State>, label: &str) {
    let pos = h.get_by_label(label).rect().center();
    click(h, pos);
}

fn open_font_menu(h: &mut Harness<'_, State>) {
    let pos = h.get_by_role_and_label(Role::ComboBox, "Font").rect().center();
    click(h, pos);
}

#[test]
fn font_menu_stays_open_for_search_and_reload_then_closes_on_choice_or_dismissal() {
    for raster_only in [false, true] {
        let fonts = Arc::new(vec![face("AmberSans-Regular"), face("SapphireSerif-Regular")]);
        let mut h = Harness::builder().with_size(egui::vec2(600.0, 420.0)).build_ui_state(
            move |ui, state: &mut State| {
                ui.horizontal(|ui| {
                    let label = ui.label("Font");
                    let selected = state.selected.as_deref().unwrap_or("Choose font").to_owned();
                    let dropdown = combo("test-font").selected_text(selected).show_ui(ui, |ui| {
                        list_with(ui, raster_only, |_, reload| {
                            state.reloads += usize::from(reload);
                            Some(Arc::clone(&fonts))
                        })
                    });
                    dropdown.response.labelled_by(label.id);
                    if let Some(Some(font)) = dropdown.inner {
                        state.selected = Some(font.names.postscript);
                    }
                    ui.add_space(200.0);
                    ui.label("Outside popup");
                });
            },
            State::default(),
        );
        h.run_steps(2);
        open_font_menu(&mut h);
        assert!(h.query_by_label("AmberSans-Regular").is_some());

        let search = h.get_by_role(Role::TextInput).rect().center();
        click(&mut h, search);
        assert!(h.query_by_role(Role::TextInput).is_some(), "clicking the text field keeps the font list open");
        h.event(Event::Text("Sapphire".into()));
        h.run_steps(3);
        assert_eq!(h.get_by_role(Role::TextInput).value().as_deref(), Some("Sapphire"));
        assert!(h.query_by_label("AmberSans-Regular").is_none(), "typing filters the actual list");
        assert!(h.query_by_label("SapphireSerif-Regular").is_some());

        click_label(&mut h, "Reload");
        assert_eq!(h.state().reloads, 1);
        assert!(h.query_by_role(Role::TextInput).is_some(), "reload keeps the popup open");
        assert!(h.query_by_label("AmberSans-Regular").is_none(), "reload retains the search");
        click_label(&mut h, "SapphireSerif-Regular");
        assert_eq!(h.state().selected.as_deref(), Some("SapphireSerif-Regular"));
        assert!(h.query_by_role(Role::TextInput).is_none(), "an actual choice closes the popup");

        open_font_menu(&mut h);
        assert!(h.query_by_role(Role::TextInput).is_some());
        h.key_press(Key::Escape);
        h.run_steps(3);
        assert!(h.query_by_role(Role::TextInput).is_none(), "Escape still dismisses the popup");

        open_font_menu(&mut h);
        assert!(h.query_by_role(Role::TextInput).is_some());
        click_label(&mut h, "Outside popup");
        assert!(h.query_by_role(Role::TextInput).is_none(), "clicking outside still dismisses the popup");
    }
}
