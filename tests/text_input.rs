//! Regression tests for "cannot paste / type the TMDB key in Tools > Options"
//! (3.0.0 on Windows).
//!
//! Root cause: eframe was built without its `clipboard` feature, so egui-winit
//! only had an in-app fallback clipboard and Ctrl+V never produced a paste of
//! text copied in another program. The first test guards the build setting;
//! the others drive a headless egui context the way the Options dialog is built
//! (modal window, a password field, a text field behind the modal that had
//! focus before) and check that typed and pasted text reaches the field.

use egui::{Event, Modifiers, PointerButton, Pos2, RawInput, Rect, Vec2};

#[test]
fn eframe_is_built_with_the_system_clipboard() {
    let manifest = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml")).unwrap();
    let line = manifest
        .lines()
        .find(|l| l.trim_start().starts_with("eframe ") || l.trim_start().starts_with("eframe="))
        .expect("eframe dependency line");
    assert!(
        line.contains("\"clipboard\""),
        "eframe must enable the \"clipboard\" feature, otherwise Ctrl+V cannot paste text from other programs: {line}"
    );
}

struct Form {
    behind: String,
    key: String,
    language: String,
    key_rect: Rect,
    language_rect: Rect,
    behind_id: Option<egui::Id>,
}

fn input(events: Vec<Event>) -> RawInput {
    RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1000.0, 800.0))),
        events,
        ..Default::default()
    }
}

fn frame(ctx: &egui::Context, f: &mut Form, password: bool, events: Vec<Event>) {
    let mut out = ctx.run_ui(input(events), |ui| {
        // main window field (e.g. the folder box) drawn first, like the app does
        let r = ui.add(egui::TextEdit::singleline(&mut f.behind));
        f.behind_id = Some(r.id);
        let ctx = ui.ctx().clone();
        egui::Modal::new(egui::Id::unique("organimizer-options")).show(&ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Your TMDB key");
                    let r = ui.add(egui::TextEdit::singleline(&mut f.key).password(password));
                    f.key_rect = r.rect;
                });
                ui.horizontal(|ui| {
                    ui.label("Language");
                    let r = ui.add(egui::TextEdit::singleline(&mut f.language));
                    f.language_rect = r.rect;
                });
            });
        });
    });
    out.textures_delta.clear(); // headless: no renderer to hand the font texture to
}

fn click(at: Pos2) -> Vec<Event> {
    vec![
        Event::PointerMoved(at),
        Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE },
        Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE },
    ]
}

fn type_and_paste(password: bool) -> Form {
    let ctx = egui::Context::default();
    let mut f = Form {
        behind: String::new(),
        key: String::new(),
        language: "en-US".into(),
        key_rect: Rect::NOTHING,
        language_rect: Rect::NOTHING,
        behind_id: None,
    };
    frame(&ctx, &mut f, password, vec![]);
    // the main-window field had keyboard focus before Options was opened
    let behind = f.behind_id.unwrap();
    ctx.memory_mut(|m| m.request_focus(behind));
    for _ in 0..3 {
        frame(&ctx, &mut f, password, vec![]);
    }
    let at = f.key_rect.center();
    frame(&ctx, &mut f, password, click(at));
    frame(&ctx, &mut f, password, vec![]);
    frame(&ctx, &mut f, password, vec![Event::Text("ab".into()), Event::Text("c".into())]);
    frame(&ctx, &mut f, password, vec![Event::Paste("0123456789abcdef0123456789abcdef".into())]);
    let at = f.language_rect.center();
    frame(&ctx, &mut f, password, click(at));
    frame(&ctx, &mut f, password, vec![]);
    frame(&ctx, &mut f, password, vec![Event::Paste("-x".into())]);
    frame(&ctx, &mut f, password, vec![]);
    f
}

#[test]
fn password_key_field_in_modal_accepts_typing_and_paste() {
    let f = type_and_paste(true);
    assert_eq!(f.key, "abc0123456789abcdef0123456789abcdef");
    assert_eq!(f.language, "en-US-x");
    assert_eq!(f.behind, "", "the field behind the modal must not receive the keystrokes");
}

#[test]
fn shown_key_field_in_modal_accepts_typing_and_paste() {
    let f = type_and_paste(false);
    assert_eq!(f.key, "abc0123456789abcdef0123456789abcdef");
    assert_eq!(f.behind, "");
}
