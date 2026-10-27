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
        remembered_volume: 1.0,
        playback_muted: false,
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
        remembered_volume: 1.0,
        playback_muted: false,
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

fn texture_rect(
    shapes: &[egui::epaint::ClippedShape],
    texture: egui::TextureId,
) -> Option<egui::Rect> {
    fn find(shape: &egui::Shape, texture: egui::TextureId) -> Option<egui::Rect> {
        match shape {
            egui::Shape::Rect(r)
                if r.brush
                    .as_ref()
                    .is_some_and(|b| b.fill_texture_id == texture) =>
            {
                Some(r.rect)
            }
            egui::Shape::Mesh(m) if m.texture_id == texture => Some(egui::Rect::from_points(
                &m.vertices.iter().map(|v| v.pos).collect::<Vec<_>>(),
            )),
            egui::Shape::Vec(shapes) => shapes.iter().find_map(|s| find(s, texture)),
            _ => None,
        }
    }
    shapes.iter().find_map(|s| find(&s.shape, texture))
}

#[test]
fn splash_moves_from_the_first_frames_without_a_hold() {
    let ctx = egui::Context::default();
    let mut app = app(&ctx);
    let mut widths = vec![];
    for elapsed in [0.05, 0.3] {
        let mut output = None;
        for _ in 0..4 {
            app.splash_started = Some(std::time::Instant::now() - Duration::from_secs_f32(elapsed));
            output = Some(ctx.run(input(egui::vec2(1280.0, 900.0)), |ctx| {
                app.splash_window(ctx)
            }));
        }
        widths.push(
            texture_rect(&output.unwrap().shapes, app.splash_texture.id())
                .unwrap()
                .width(),
        );
    }
    assert!(
        widths[0] < 100.0,
        "splash should start in the distance: {widths:?}"
    );
    assert!(
        widths[1] > widths[0] * 1.2,
        "splash must move immediately: {widths:?}"
    );
}

#[test]
fn thumbnail_hearts_have_no_background_box() {
    fn boxes(shape: &egui::Shape, point: egui::Pos2) -> bool {
        match shape {
            egui::Shape::Rect(r) => {
                r.rect.contains(point)
                    && r.rect.width() < 45.0
                    && r.rect.height() < 45.0
                    && (r.fill.a() > 0 || r.stroke.width > 0.0)
            }
            egui::Shape::Vec(shapes) => shapes.iter().any(|s| boxes(s, point)),
            _ => false,
        }
    }
    let ctx = egui::Context::default();
    let mut app = app(&ctx);
    let session = SessionView {
        id: "s".into(),
        name: "Test".into(),
        date: None,
        files: vec![],
        flights: vec![],
        segmentable: false,
        reducible: false,
        detection_profile: "standard".into(),
        location: None,
        notes: None,
        labels: vec![],
    };
    let mut flight = flight();
    for liked in [false, true] {
        flight.liked = liked;
        let mut output = None;
        for _ in 0..3 {
            output = Some(ctx.run(input(egui::vec2(600.0, 500.0)), |ctx| {
                egui::CentralPanel::default()
                    .show(ctx, |ui| app.flight_card(ui, &session, &flight, None));
            }));
        }
        let output = output.unwrap();
        let heart = text_positions(&output.shapes)
            .into_iter()
            .find(|(s, _)| s == if liked { "♥" } else { "♡" })
            .unwrap()
            .1
            + egui::vec2(4.0, 4.0);
        assert!(
            !output.shapes.iter().any(|s| boxes(&s.shape, heart)),
            "heart must not have a painted button box"
        );
    }
}

fn test_viewer(ctx: &egui::Context) -> Viewer {
    Viewer {
        flight: flight(),
        player: Player::new(vec![], None, 0.0, ctx.clone()),
        texture: None,
        mark: None,
        figure: String::new(),
        label: String::new(),
        volume: 0.6,
        remembered_volume: 0.6,
        playback_muted: false,
        muted: false,
        fullscreen: false,
        window: Default::default(),
        seek_time: None,
    }
}

#[test]
fn player_speaker_click_mutes_and_restores_the_audible_gain() {
    let ctx = egui::Context::default();
    let mut viewer = test_viewer(&ctx);
    viewer.player.set_volume(viewer.volume);
    let mut rect = egui::Rect::NOTHING;
    let render = |ctx: &egui::Context, viewer: &mut Viewer, rect: &mut egui::Rect, events| {
        let mut raw = input(egui::vec2(300.0, 150.0));
        raw.events = events;
        let _ = ctx.run(raw, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                *rect = player_speaker(ui, viewer).rect;
            });
        });
    };
    for _ in 0..3 {
        render(&ctx, &mut viewer, &mut rect, vec![]);
    }
    for expected in [0.0, 0.6] {
        for pressed in [true, false] {
            let pos = rect.center();
            render(
                &ctx,
                &mut viewer,
                &mut rect,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
            );
        }
        assert!(
            (viewer.player.volume() - expected).abs() < 0.001,
            "speaker click did not update gain"
        );
    }
    viewer.volume = 0.0;
    viewer.player.set_volume(0.0);
    for pressed in [true, false] {
        let pos = rect.center();
        render(
            &ctx,
            &mut viewer,
            &mut rect,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                },
            ],
        );
    }
    assert!(
        (viewer.player.volume() - 0.6).abs() < 0.001,
        "zero-volume unmute restores the last audible level"
    );
    viewer.player.seek(0.0);
    assert!(
        (viewer.player.volume() - 0.6).abs() < 0.001,
        "seeking preserves gain"
    );
    viewer.muted = true;
    viewer.player.set_volume(0.0);
    for pressed in [true, false] {
        let pos = rect.center();
        render(
            &ctx,
            &mut viewer,
            &mut rect,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                },
            ],
        );
    }
    assert_eq!(
        viewer.player.volume(),
        0.0,
        "speaker must not undo forced no-audio policy"
    );
}

#[test]
fn volume_is_vertical_on_the_right_and_increases_upwards() {
    let ctx = egui::Context::default();
    let mut viewer = test_viewer(&ctx);
    let mut slider = egui::Rect::NOTHING;
    let video = egui::Rect::from_min_size(egui::pos2(20.0, 20.0), egui::vec2(500.0, 320.0));
    let render = |viewer: &mut Viewer, slider: &mut egui::Rect, events| {
        let mut raw = input(egui::vec2(600.0, 400.0));
        raw.events = events;
        let _ = ctx.run(raw, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                if let Some(response) = player_volume_overlay(ui, viewer, video) {
                    *slider = response.rect;
                }
            });
        });
    };
    for _ in 0..3 {
        render(&mut viewer, &mut slider, vec![]);
    }
    assert!(
        slider.height() > slider.width() * 3.0,
        "volume should be vertical: {slider:?}"
    );
    assert!(video.contains_rect(slider) && slider.right() > video.right() - 45.0);
    let mut gains = vec![];
    for y in [slider.bottom() - 8.0, slider.top() + 8.0] {
        for pressed in [true, false] {
            let pos = egui::pos2(slider.center().x, y);
            render(
                &mut viewer,
                &mut slider,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
            );
        }
        gains.push(viewer.player.volume());
    }
    assert!(
        gains[1] > gains[0] + 0.5,
        "volume increases upwards: {gains:?}"
    );
    viewer.muted = true;
    let _ = ctx.run(input(egui::vec2(600.0, 400.0)), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            assert!(player_volume_overlay(ui, &mut viewer, video).is_none())
        });
    });
}

#[test]
fn video_controls_do_not_paint_a_background_strip() {
    fn has_strip(shape: &egui::Shape, image: egui::Rect) -> bool {
        match shape {
            egui::Shape::Rect(r) => {
                image.expand(1.0).contains_rect(r.rect)
                    && r.rect.width() > image.width() * 0.7
                    && r.rect.height() > 40.0
                    && r.rect.height() < image.height() * 0.8
                    && r.fill.a() > 0
            }
            egui::Shape::Vec(shapes) => shapes.iter().any(|s| has_strip(s, image)),
            _ => false,
        }
    }
    let ctx = egui::Context::default();
    let mut app = app(&ctx);
    let mut viewer = test_viewer(&ctx);
    let texture = ctx.load_texture(
        "transparent-controls",
        egui::ColorImage::filled([640, 360], Color32::WHITE),
        Default::default(),
    );
    let id = texture.id();
    viewer.texture = Some(texture);
    app.viewer = Some(viewer);
    let mut output = None;
    for _ in 0..4 {
        output = Some(ctx.run(input(egui::vec2(1280.0, 820.0)), |ctx| {
            app.player_window(ctx)
        }));
    }
    let shapes = output.unwrap().shapes;
    let image = texture_rect(&shapes, id).unwrap();
    assert!(
        !shapes.iter().any(|s| has_strip(&s.shape, image)),
        "controls must be transparent over the video"
    );
}

fn player_input(ctx: &egui::Context, app: &mut App, events: Vec<egui::Event>) -> egui::FullOutput {
    let mut raw = input(egui::vec2(1280.0, 820.0));
    raw.events = events;
    ctx.run(raw, |ctx| app.player_window(ctx))
}
fn click_player(ctx: &egui::Context, app: &mut App, pos: egui::Pos2) {
    for pressed in [true, false] {
        player_input(
            ctx,
            app,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                },
            ],
        );
    }
}
#[test]
fn video_click_buttons_and_space_share_playback_state() {
    let ctx = egui::Context::default();
    let mut app = app(&ctx);
    let mut viewer = test_viewer(&ctx);
    let texture = ctx.load_texture(
        "clickable-video",
        egui::ColorImage::filled([640, 360], Color32::GRAY),
        Default::default(),
    );
    let id = texture.id();
    viewer.texture = Some(texture);
    app.viewer = Some(viewer);
    let mut output = None;
    for _ in 0..4 {
        output = Some(player_input(&ctx, &mut app, vec![]));
    }
    let image = texture_rect(&output.unwrap().shapes, id).unwrap();
    click_player(&ctx, &mut app, image.center());
    assert!(
        app.viewer.as_ref().unwrap().player.paused(),
        "video click must pause playback"
    );
    let output = player_input(&ctx, &mut app, vec![]);
    let play = text_positions(&output.shapes)
        .into_iter()
        .find(|(s, _)| s == "Play")
        .expect("retained Play button")
        .1;
    assert!(
        play.y > image.bottom(),
        "playback buttons belong below the video"
    );
    click_player(&ctx, &mut app, play + egui::vec2(6.0, 6.0));
    assert!(
        !app.viewer.as_ref().unwrap().player.paused(),
        "Play button must resume once"
    );
    click_player(
        &ctx,
        &mut app,
        egui::pos2(image.right() - 31.0, image.bottom() - 180.0),
    );
    assert!(
        !app.viewer.as_ref().unwrap().player.paused(),
        "volume clicks must not toggle video playback"
    );
    // Give focus back to the video before testing the global shortcut.
    click_player(&ctx, &mut app, image.center());
    assert!(app.viewer.as_ref().unwrap().player.paused());
    ctx.memory_mut(|m| m.surrender_focus(m.focused().unwrap_or(egui::Id::NULL)));
    player_input(
        &ctx,
        &mut app,
        vec![egui::Event::Key {
            key: egui::Key::Space,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Default::default(),
        }],
    );
    assert!(
        !app.viewer.as_ref().unwrap().player.paused(),
        "Space must resume"
    );
    let output = player_input(&ctx, &mut app, vec![]);
    let field = text_positions(&output.shapes)
        .into_iter()
        .find(|(s, _)| s == "Add a tag…")
        .unwrap()
        .1;
    click_player(&ctx, &mut app, field + egui::vec2(8.0, 6.0));
    player_input(
        &ctx,
        &mut app,
        vec![
            egui::Event::Key {
                key: egui::Key::Space,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            },
            egui::Event::Text(" ".into()),
        ],
    );
    assert!(
        !app.viewer.as_ref().unwrap().player.paused(),
        "typing spaces must not pause playback"
    );
}

#[test]
fn boundary_editor_places_start_left_and_end_right() {
    let ctx = egui::Context::default();
    let mut app = app(&ctx);
    let mut flight = flight();
    flight.clips.push(fpv_library::ClipView {
        id: "clip".into(),
        file_id: "source".into(),
        file_name: "DVR.avi".into(),
        start_frame: 10,
        end_frame: 80,
        last_frame: 99,
        fps: 25.0,
        file_duration: 4.0,
        entry: "".into(),
        exit: "".into(),
        thumbnail: None,
    });
    app.dialog = Some(Dialog::editor(&flight));
    let mut output = None;
    for _ in 0..4 {
        output = Some(ctx.run(input(egui::vec2(1280.0, 900.0)), |ctx| {
            app.dialog_window(ctx)
        }));
    }
    let texts = text_positions(&output.unwrap().shapes);
    let start = texts
        .iter()
        .find(|(s, _)| s == "Start")
        .unwrap_or_else(|| panic!("start handle label missing: {texts:?}"))
        .1;
    let end = texts
        .iter()
        .find(|(s, _)| s == "End (exclusive)")
        .expect("end handle label")
        .1;
    assert!(
        end.x > start.x + 200.0 && (end.y - start.y).abs() < 10.0,
        "start should be aligned left and end right on the same trim rail: {start:?}, {end:?}"
    );
    for size in [egui::vec2(800.0, 600.0), egui::vec2(640.0, 480.0)] {
        for _ in 0..4 {
            let _ = ctx.run(input(size), |ctx| app.dialog_window(ctx));
        }
        let rect = ctx.memory(|m| m.area_rect(egui::Id::new("form"))).unwrap();
        assert!(
            egui::Rect::from_min_size(egui::Pos2::ZERO, size)
                .expand(1.0)
                .contains_rect(rect),
            "boundary editor must fit smaller displays: {rect:?} in {size:?}"
        );
    }
}
