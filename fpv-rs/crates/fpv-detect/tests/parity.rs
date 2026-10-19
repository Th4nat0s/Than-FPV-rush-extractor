//! Parity with the Python detector on real frames. The thresholds were measured
//! against these numbers, so a metric that drifts would silently retune the
//! whole detector — the contract every port of it is held to.

use fpv_detect::{DisarmedTemplate, FrameAnalyzer};

fn fixture(name: &str) -> Vec<u8> {
    let path = format!("{}/tests/fixtures/{name}.gray", env!("CARGO_MANIFEST_DIR"));
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    assert_eq!(bytes.len(), 640 * 480);
    bytes
}

/// What spike/fpv_spike.py computes for these exact frames:
/// (luma, laplacian variance, block variation, vertical correlation, flat fraction).
const REFERENCES: [(&str, [f64; 5]); 3] = [
    ("snow", [47.6715, 1153.700, 0.101414, 0.645934, 0.0]),
    ("picture", [156.9389, 222.151, 1.638780, 0.974649, 0.0]),
    ("menu", [86.6264, 1286.795, 1.518726, 0.839572, 0.125]),
];

#[test]
fn matches_the_python_detector_on_real_frames() {
    for (name, [luma, laplacian, variation, vertical, flat]) in REFERENCES {
        let m = FrameAnalyzer::default().metrics(&fixture(name));
        assert!((m.luma - luma).abs() < 0.001, "{name} luma {}", m.luma);
        // The Python side computes the Laplacian in float32, so a few parts per
        // million of drift are expected and harmless.
        assert!((m.laplacian_variance - laplacian).abs() / laplacian < 1e-4,
                "{name} laplacian {}", m.laplacian_variance);
        assert!((m.block_variation - variation).abs() < 1e-4, "{name} blkcv {}", m.block_variation);
        assert!((m.vertical_correlation - vertical).abs() < 1e-4,
                "{name} corrv {}", m.vertical_correlation);
        assert!((m.flat_fraction - flat).abs() < 1e-9, "{name} flat {}", m.flat_fraction);
    }
}

#[test]
fn separates_snow_from_picture_and_finds_the_goggle_screen_flat() {
    let analyzer = FrameAnalyzer::default();
    let (snow, picture, menu) = (analyzer.metrics(&fixture("snow")),
                                 analyzer.metrics(&fixture("picture")),
                                 analyzer.metrics(&fixture("menu")));
    // The whole boundary detector rests on these gaps.
    assert!(snow.block_variation < 0.2 && picture.block_variation > 1.5);
    assert!(snow.vertical_correlation < 0.7 && picture.vertical_correlation > 0.95);
    assert!(menu.flat_fraction > 0.10 && picture.flat_fraction == 0.0);
    assert!(menu.vertical_correlation < 0.90);
}

#[test]
fn matches_the_python_disarmed_numbers_on_the_fixture_frames() {
    let template = DisarmedTemplate::bundled().unwrap();
    // The picture fixture is a flying frame: 0.486 is what a busy background
    // reproduces by accident, and it sits below the 0.55 threshold.
    for (name, expected) in [("snow", 0.0), ("picture", 0.4861538462), ("menu", 0.0)] {
        let score = template.match_frame(&fixture(name), 640, 480);
        assert!((score - expected).abs() < 1e-9, "{name}: {score}");
    }
}
