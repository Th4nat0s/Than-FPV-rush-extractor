use super::*;
use crate::{browse::Browse, images::Images, worker::Worker};
use std::collections::{HashMap, HashSet};
use std::sync::mpsc;

fn app(ctx: &egui::Context) -> App {
    ctx.style_mut(|s| { s.spacing.item_spacing = egui::vec2(10.0, 8.0); s.spacing.button_padding = egui::vec2(10.0, 6.0); });
    let (sender, _) = mpsc::channel();
    let (_, events) = mpsc::channel();
    App {
        worker: Worker { sender, events },
        sessions: vec![],
        cards: vec![],
        config: Default::default(),
        config_directory: PathBuf::new(),
        cache_directory: PathBuf::new(),
        tools: None,
        ffmpeg_ok: true,
        ready: true,
        browse: Browse::default(),
        tasks: vec![],
        next_id: 1,
        dialog: None,
        dialog_request: None,
        viewer: None,
        images: Images::new(ctx.clone()),
        thumbnails: HashMap::new(),
        requested: HashSet::new(),
        selected: HashSet::new(),
        search: String::new(),
        notices: vec![],
        show_tasks: false,
        exit_after: None,
        screenshot_requested: false,
        splash_started: None,
        splash_exit_started: None,
        splash_texture: ctx.load_texture(
            "test-logo",
            egui::ColorImage::filled([180, 194], Color32::BLACK),
            Default::default(),
        ),
        show_about: false,
    }
}
fn text_positions(shapes: &[egui::epaint::ClippedShape]) -> Vec<(String, egui::Pos2)> {
    fn walk(shape: &egui::Shape, out: &mut Vec<(String, egui::Pos2)>) {
        match shape {
            egui::Shape::Text(t) => out.push((t.galley.job.text.clone(), t.pos)),
            egui::Shape::Vec(shapes) => {
                for s in shapes {
                    walk(s, out);
                }
            }
            _ => {}
        }
    }
    let mut out = vec![];
    for s in shapes {
        walk(&s.shape, &mut out);
    }
    out
}
fn input(size: egui::Vec2) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
        ..Default::default()
    }
}

#[test]
fn splash_has_no_close_button_and_grows_while_fading() {
    let ctx = egui::Context::default();
    let mut app = app(&ctx);
    app.splash_started = Some(std::time::Instant::now());
    for _ in 0..3 {
        let output = ctx.run(input(egui::vec2(900.0, 700.0)), |ctx| {
            app.splash_window(ctx)
        });
        assert!(
            !text_positions(&output.shapes)
                .iter()
                .any(|(s, _)| s == "Close")
        );
    }
}

#[test]
fn splash_visibly_reaches_twice_its_initial_size_before_becoming_transparent() {
    fn mesh(shapes: &[egui::epaint::ClippedShape], texture: egui::TextureId) -> (egui::Rect, u8) {
        fn find(shape: &egui::Shape, texture: egui::TextureId) -> Option<(egui::Rect, u8)> {
            match shape {
                egui::Shape::Rect(r)
                    if r.brush
                        .as_ref()
                        .is_some_and(|b| b.fill_texture_id == texture) =>
                {
                    Some((r.rect, r.fill.a()))
                }
                egui::Shape::Mesh(m) if m.texture_id == texture => Some((
                    egui::Rect::from_points(&m.vertices.iter().map(|v| v.pos).collect::<Vec<_>>()),
                    m.vertices[0].color.a(),
                )),
                egui::Shape::Vec(shapes) => shapes.iter().find_map(|s| find(s, texture)),
                _ => None,
            }
        }
        shapes.iter().find_map(|s| find(&s.shape, texture)).unwrap()
    }
    let ctx = egui::Context::default();
    let app = app(&ctx);
    let mut shapes = vec![];
    for _ in 0..12 {
        shapes = ctx
            .run(input(egui::vec2(900.0, 700.0)), |ctx| {
                egui::CentralPanel::default()
                    .show(ctx, |ui| splash_logo(ui, &app.splash_texture, 0.0));
            })
            .shapes;
    }
    let initial = mesh(&shapes, app.splash_texture.id());
    shapes = ctx
        .run(input(egui::vec2(900.0, 700.0)), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| splash_logo(ui, &app.splash_texture, 0.7));
        })
        .shapes;
    let leaving = mesh(&shapes, app.splash_texture.id());
    assert!(leaving.0.width() >= initial.0.width() * 2.0);
    assert!(leaving.0.height() >= initial.0.height() * 2.0);
    assert!(leaving.1 > 0 && leaving.1 < initial.1);
}

