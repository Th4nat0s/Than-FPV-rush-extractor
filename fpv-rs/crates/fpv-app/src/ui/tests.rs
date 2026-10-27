use super::*;
use crate::{browse::Browse, images::Images, worker::Worker};
use std::collections::{HashMap, HashSet};
use std::sync::mpsc;
use crate::player::Player;

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
fn flight() -> FlightView {
    FlightView {
        id: "test-flight".into(),
        order: 1,
        title: Some("Layout test flight".into()),
        liked: false,
        duration: 60.0,
        clips: vec![],
        recordings: 1,
        action_cam: true,
        source: "GoPro".into(),
        format: "1080p · H.264 · 30 fps".into(),
        reduced: false,
        cloud: false,
        freeable: false,
        cache_key: "test-flight".into(),
        labels: vec![],
        figures: vec![],
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

#[test]
fn about_shows_the_requested_roles_and_letzkwad_link() {
    let ctx = egui::Context::default();
    let mut app = app(&ctx);
    app.show_about = true;
    let mut positions = vec![];
    for _ in 0..3 {
        let output = ctx.run(input(egui::vec2(900.0, 700.0)), |ctx| app.about_window(ctx));
        positions = text_positions(&output.shapes);
    }
    assert!(
        positions
            .iter()
            .any(|(s, _)| s.contains("Coded by Claude, Codex, Driven by Defane, Thanat0s"))
    );
    assert!(positions.iter().any(|(s, _)| s.contains("letzkwad.lu")));
}

#[test]
fn video_progress_times_are_drawn_inside_the_video_image() {
    fn image_bounds(shape: &egui::Shape, texture: egui::TextureId) -> Option<egui::Rect> {
        match shape {
            egui::Shape::Mesh(mesh) if mesh.texture_id == texture => Some(egui::Rect::from_points(
                &mesh.vertices.iter().map(|v| v.pos).collect::<Vec<_>>(),
            )),
            egui::Shape::Vec(shapes) => shapes.iter().find_map(|s| image_bounds(s, texture)),
            _ => None,
        }
    }
    let ctx = egui::Context::default();
    let mut app = app(&ctx);
    let texture = ctx.load_texture(
        "overlay-test-video",
        egui::ColorImage::filled([640, 360], Color32::GRAY),
        Default::default(),
    );
    let texture_id = texture.id();
    app.viewer = Some(Viewer {
        flight: flight(),
        player: Player::new(vec![], None, 0.0, ctx.clone()),
        texture: Some(texture),
        mark: None,
        figure: String::new(),
        label: String::new(),
        volume: 1.0,
        muted: false,
        fullscreen: false,
        seek_time: None,
    });
    let mut shapes = vec![];
    for _ in 0..4 {
        shapes = ctx
            .run(input(egui::vec2(1100.0, 760.0)), |ctx| {
                app.player_window(ctx)
            })
            .shapes;
    }
    let image = shapes
        .iter()
        .find_map(|s| image_bounds(&s.shape, texture_id))
        .unwrap();
    let times: Vec<_> = text_positions(&shapes)
        .into_iter()
        .filter(|(text, _)| text == &clock(0.0))
        .collect();
    assert_eq!(times.len(), 2);
    for (_, position) in times {
        assert!(
            image.contains(position),
            "time at {position:?} lies outside the video {image:?}"
        );
    }
}

#[test]
fn player_fits_viewport_when_opened_and_when_the_display_shrinks() {
    let ctx = egui::Context::default();
    let mut app = app(&ctx);
    app.viewer = Some(Viewer {
        flight: flight(),
        player: Player::new(vec![], None, 0.0, ctx.clone()),
        texture: Some(ctx.load_texture(
            "test-video",
            egui::ColorImage::filled([640, 360], Color32::GRAY),
            Default::default(),
        )),
        mark: None,
        figure: String::new(),
        label: String::new(),
        volume: 1.0,
        muted: false,
        fullscreen: false,
        seek_time: None,
    });
    for size in [
        egui::vec2(1280.0, 820.0),
        egui::vec2(900.0, 600.0),
        egui::vec2(640.0, 480.0),
    ] {
        for _ in 0..4 {
            let _ = ctx.run(input(size), |ctx| app.player_window(ctx));
        }
        let rect = ctx
            .memory(|memory| memory.area_rect(egui::Id::new("player")))
            .unwrap();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
        assert!(
            screen.expand(1.0).contains_rect(rect),
            "player {rect:?} exceeds display {screen:?}"
        );
    }
    let size = egui::vec2(900.0, 600.0);
    for fullscreen in [true, false] {
        app.viewer.as_mut().unwrap().fullscreen = fullscreen;
        for _ in 0..4 {
            let _ = ctx.run(input(size), |ctx| app.player_window(ctx));
        }
        let rect = ctx
            .memory(|m| m.area_rect(egui::Id::new("player")))
            .unwrap();
        assert!(
            egui::Rect::from_min_size(egui::Pos2::ZERO, size)
                .expand(1.0)
                .contains_rect(rect),
            "fullscreen transition exceeds viewport: {rect:?}"
        );
    }
    app.viewer.as_mut().unwrap().fullscreen = true;
    let mut escape = input(size);
    escape.events.push(egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Default::default(),
    });
    let output = ctx.run(escape, |ctx| app.player_window(ctx));
    assert!(!app.viewer.as_ref().unwrap().fullscreen);
    assert!(
        output.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .iter()
            .any(|command| matches!(command, egui::ViewportCommand::Fullscreen(false)))
    );
}

