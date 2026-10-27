use super::*;
use crate::{browse::Browse, images::Images, player::Player, worker::Worker};
use std::collections::{HashMap, HashSet};
use std::sync::mpsc;

fn app(ctx: &egui::Context) -> App {
    crate::configure_style(ctx);
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
        selecting: false,
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
fn player_fits_viewport_when_opened_and_when_the_display_shrinks() {
    let ctx = egui::Context::default();
    let mut app = app(&ctx);
    let new_viewer = || Viewer {
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
        window: Default::default(),
        seek_time: None,
    };
    app.viewer = Some(new_viewer());
    for _ in 0..4 {
        let _ = ctx.run(input(egui::vec2(1280.0, 820.0)), |ctx| {
            app.player_window(ctx)
        });
    }
    let initial = ctx
        .memory(|m| m.area_rect(egui::Id::new("player")))
        .unwrap();
    assert!(
        initial.width() <= 1050.0 && initial.height() <= 680.0,
        "player should open compact, not fill the viewport: {initial:?}"
    );
    assert!(!app.viewer.as_ref().unwrap().fullscreen);
    fn click_control(ctx: &egui::Context, app: &mut App, label: &str) {
        let size = egui::vec2(1280.0, 820.0);
        let output = ctx.run(input(size), |ctx| app.player_window(ctx));
        let position = text_positions(&output.shapes)
            .into_iter()
            .find(|(text, _)| text == label)
            .expect("visible player control")
            .1
            + egui::vec2(5.0, 5.0);
        for pressed in [true, false] {
            let mut raw = input(size);
            raw.events = vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                },
            ];
            let _ = ctx.run(raw, |ctx| app.player_window(ctx));
        }
        for _ in 0..4 {
            let _ = ctx.run(input(size), |ctx| app.player_window(ctx));
        }
    }
    click_control(&ctx, &mut app, "Maximize");
    let maximized = ctx
        .memory(|m| m.area_rect(egui::Id::new("player")))
        .unwrap();
    assert!(maximized.width() > initial.width() + 100.0);
    assert!(maximized.height() > initial.height() + 100.0);
    assert!(
        !app.viewer.as_ref().unwrap().fullscreen,
        "maximize must keep OS windowed"
    );
    click_control(&ctx, &mut app, "Restore");
    let restored = ctx
        .memory(|m| m.area_rect(egui::Id::new("player")))
        .unwrap();
    assert!(
        (restored.size() - initial.size()).length() < 2.0
            && restored.min.distance(initial.min) < 2.0,
        "restore should return to {initial:?}, got {restored:?}"
    );
    click_control(&ctx, &mut app, "Maximize");
    click_control(&ctx, &mut app, "×");
    assert!(
        app.viewer.is_none(),
        "close must dismiss the maximized player"
    );
    app.viewer = Some(new_viewer());
    for _ in 0..4 {
        let _ = ctx.run(input(egui::vec2(1280.0, 820.0)), |ctx| {
            app.player_window(ctx)
        });
    }
    let reopened_rect = ctx
        .memory(|m| m.area_rect(egui::Id::new("player")))
        .unwrap();
    assert!(
        (reopened_rect.size() - initial.size()).length() < 2.0,
        "opening another video must reset maximized geometry: {reopened_rect:?}"
    );
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
#[test]
fn pagination_is_rendered_below_the_flight_cards() {
    let ctx = egui::Context::default();
    let mut app = app(&ctx);
    app.browse.view = View::Flights;
    app.sessions.push(SessionView {
        id: "session".into(),
        name: "Test session".into(),
        date: None,
        files: vec![],
        flights: vec![flight()],
        segmentable: false,
        reducible: false,
        detection_profile: "standard".into(),
        location: None,
        notes: None,
        labels: vec![],
    });
    let mut positions = vec![];
    for _ in 0..3 {
        let output = ctx.run(input(egui::vec2(900.0, 600.0)), |ctx| app.browser(ctx));
        positions = text_positions(&output.shapes);
    }
    let showing = positions
        .iter()
        .find(|(s, _)| s.starts_with("Showing "))
        .unwrap()
        .1;
    let play = positions
        .iter()
        .find(|(s, _)| s == "Layout test flight")
        .unwrap()
        .1;
    assert!(
        showing.y > play.y,
        "pagination at {showing:?} precedes the cards at {play:?}"
    );
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
    let mut previous = 0.0;
    for expected in [
        "Coded by",
        "• Claude",
        "• Codex",
        "Driven by",
        "• Defane",
        "• Thanat0s",
    ] {
        let y = positions
            .iter()
            .find(|(text, _)| text == expected)
            .unwrap_or_else(|| panic!("missing contributor line {expected}"))
            .1
            .y;
        assert!(
            y > previous,
            "credits must appear in role order on separate lines"
        );
        previous = y;
    }
    assert!(positions.iter().any(|(s, _)| s.contains("letzkwad.lu")));
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
        window: Default::default(),
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
fn import_speaker_button_toggles_the_same_state_as_the_no_sound_checkbox() {
    let ctx = egui::Context::default();
    let mut muted = false;
    let mut icon = egui::Rect::NOTHING;
    for _ in 0..2 {
        let _ = ctx.run(input(egui::vec2(400.0, 150.0)), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                icon = no_sound_choice(ui, &mut muted).rect;
            });
        });
    }
    for expected in [true, false] {
        for pressed in [true, false] {
            let mut input = input(egui::vec2(400.0, 150.0));
            input.events = vec![
                egui::Event::PointerMoved(icon.center()),
                egui::Event::PointerButton {
                    pos: icon.center(),
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                },
            ];
            let _ = ctx.run(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let _ = no_sound_choice(ui, &mut muted);
                });
            });
        }
        assert_eq!(muted, expected);
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
fn screenshot_reference_navigation_shows_tab_counts() {
    let ctx = egui::Context::default();
    let mut app = app(&ctx);
    let mut flight = flight();
    flight.figures.push(fpv_library::TagView {
        id: "tag".into(),
        label: "split-S".into(),
        note: None,
        start: 1.0,
        end: 2.0,
        marker: false,
    });
    app.sessions.push(SessionView {
        id: "s".into(),
        name: "Reference session".into(),
        date: None,
        files: vec![],
        flights: vec![flight],
        segmentable: false,
        reducible: false,
        detection_profile: "standard".into(),
        location: None,
        notes: None,
        labels: vec![],
    });
    let mut positions = vec![];
    for _ in 0..3 {
        positions = text_positions(
            &ctx.run(input(egui::vec2(1280.0, 820.0)), |ctx| app.sidebar(ctx))
                .shapes,
        );
    }
    for expected in ["Sessions\n1", "All flights\n1", "Figures\n1"] {
        assert!(
            positions.iter().any(|(text, _)| text == expected),
            "missing tab {expected:?}"
        );
    }
}

#[test]
fn screenshot_reference_figure_cards_keep_the_flight_title_and_overlay_the_figure_name() {
    let ctx = egui::Context::default();
    let mut app = app(&ctx);
    app.browse.view = View::Figures;
    let mut flight = flight();
    flight.figures.push(fpv_library::TagView {
        id: "tag".into(),
        label: "split-S".into(),
        note: None,
        start: 1.0,
        end: 2.0,
        marker: false,
    });
    app.sessions.push(SessionView {
        id: "s".into(),
        name: "Reference session".into(),
        date: None,
        files: vec![],
        flights: vec![flight],
        segmentable: false,
        reducible: false,
        detection_profile: "standard".into(),
        location: None,
        notes: None,
        labels: vec![],
    });
    let mut positions = vec![];
    for _ in 0..3 {
        positions = text_positions(
            &ctx.run(input(egui::vec2(1280.0, 820.0)), |ctx| app.browser(ctx))
                .shapes,
        );
    }
    let figure = positions
        .iter()
        .find(|(text, _)| text == "split-S")
        .expect("figure badge")
        .1;
    let flight = positions
        .iter()
        .find(|(text, _)| text == "Layout test flight")
        .expect("parent flight title")
        .1;
    assert!(
        figure.y < flight.y,
        "figure badge at {figure:?} must precede the flight caption at {flight:?}"
    );
}

#[test]
fn pagination_stays_inside_a_narrow_view_with_many_pages() {
    let ctx = egui::Context::default();
    let mut app = app(&ctx);
    app.browse.view = View::Flights;
    app.browse.page = 10;
    app.sessions.push(SessionView {
        id: "s".into(),
        name: "Large session".into(),
        date: None,
        files: vec![],
        flights: vec![flight(); 500],
        segmentable: false,
        reducible: false,
        detection_profile: "standard".into(),
        location: None,
        notes: None,
        labels: vec![],
    });
    let mut positions = vec![];
    for _ in 0..4 {
        positions = text_positions(
            &ctx.run(input(egui::vec2(900.0, 600.0)), |ctx| {
                app.sidebar(ctx);
                app.browser(ctx);
            })
            .shapes,
        );
    }
    for label in ["Showing ", "Per page"] {
        let position = positions
            .iter()
            .find(|(text, _)| text.starts_with(label))
            .unwrap()
            .1;
        assert!(
            position.x >= 0.0
                && position.x < 900.0
                && position.y > 500.0
                && position.y + 24.0 <= 600.0,
            "footer {label} lies outside its bottom row: {position:?}"
        );
    }
}
