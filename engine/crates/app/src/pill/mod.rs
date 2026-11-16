//! The recording pill. `model` is the lifecycle and motion (pure, time-injected, unit-tested) and
//! `paint` draws a frame into pixels that the Slint overlay shows, on every OS. The look is
//! Voicio's own, from `design/voicio-handoff`; the lifecycle approach (a pure model, minimum
//! visible times, no frames while idle) follows Hex (https://github.com/anomalyco/hex, MIT).

pub mod bands;
pub mod model;
pub mod paint;
pub mod spring;

/// Developer tool (`voicio pill-snapshot out.ppm`): paints every look, dark and light, over a
/// backdrop and saves one picture, so the animation can be reviewed without a compositor.
pub fn snapshot(path: &std::path::Path) -> std::io::Result<()> {
    use model::{Bands, Event, Model, NoticeKind};
    use std::time::{Duration, Instant};

    // Each look: the events that lead to it, then how long to let it play.
    type Look = (&'static str, &'static [Event]);
    let looks: [Look; 9] = [
        ("rest", &[]),
        ("voice", &[Event::Meter(Bands { level: 0.09, low: 0.10, mid: 0.08, high: 0.04 })]),
        ("loud", &[Event::Meter(Bands { level: 0.35, low: 0.4, mid: 0.3, high: 0.2 })]),
        ("noisy room", &[Event::Meter(Bands { level: 0.03, low: 0.03, mid: 0.03, high: 0.03 })]),
        ("working", &[Event::Submitted]),
        ("typed", &[Event::Submitted, Event::Completed]),
        ("low memory", &[Event::Notice(NoticeKind::Warn)]),
        ("problem", &[Event::Notice(NoticeKind::Error)]),
        ("warming", &[Event::Warming(true)]),
    ];
    let (w, h, scale) = (96.0f32, 64.0f32, 3.0f32);
    let (tw, th) = ((w * scale) as usize, (h * scale) as usize);
    let mut tiles: Vec<(Vec<u8>, bool)> = Vec::new();
    for dark in [true, false] {
        for (name, events) in &looks {
            let mut now = Instant::now();
            let mut model = Model::new(now);
            model.handle(Event::Started, now);
            let mut frame = model.step(now, 1.0 / 60.0);
            // speak for a while so the lattice has ripples to show
            let speaking = events.iter().any(|e| matches!(e, Event::Meter(_)));
            let mut bands = Bands::default();
            if let Some(Event::Meter(b)) = events.iter().find(|e| matches!(e, Event::Meter(_))) {
                bands = *b;
            }
            for i in 0..90 {
                now += Duration::from_micros(16_667);
                if speaking {
                    let lo = if *name == "noisy room" { 0.8 } else { 0.12 };
                    let wobble = lo + (1.0 - lo) * ((i as f32 * 0.37).sin() * (i as f32 * 0.11).cos()).abs();
                    model.handle(Event::Meter(Bands { level: bands.level * wobble, low: bands.low * wobble, mid: bands.mid * wobble, high: bands.high * wobble }), now);
                }
                frame = model.step(now, 1.0 / 60.0);
            }
            for e in events.iter().filter(|e| !matches!(e, Event::Meter(_))) {
                model.handle(*e, now);
            }
            let after = if matches!(events.last(), Some(Event::Completed)) { 0.75 } else if *name == "working" { 0.5 } else { 0.4 };
            for i in 0..(after * 60.0) as usize {
                now += Duration::from_micros(16_667);
                if speaking {
                    let lo = if *name == "noisy room" { 0.8 } else { 0.12 };
                    let wobble = lo + (1.0 - lo) * (((90 + i) as f32 * 0.37).sin() * ((90 + i) as f32 * 0.11).cos()).abs();
                    model.handle(Event::Meter(Bands { level: bands.level * wobble, low: bands.low * wobble, mid: bands.mid * wobble, high: bands.high * wobble }), now);
                }
                frame = model.step(now, 1.0 / 60.0);
            }
            let mut buffer = Vec::new();
            paint::paint(&frame, &paint::Target { width: w, height: h, scale, dark }, &mut buffer);
            tiles.push((buffer, dark));
        }
    }
    let (cols, rows) = (looks.len(), 2);
    let mut out = format!("P6\n{} {}\n255\n", tw * cols, th * rows).into_bytes();
    for row in 0..rows {
        for y in 0..th {
            for col in 0..cols {
                let (tile, dark) = &tiles[row * cols + col];
                let backdrop: [f32; 3] = if *dark { [0.16, 0.18, 0.22] } else { [0.953, 0.937, 0.902] }; // #F3EFE6
                for x in 0..tw {
                    let p = &tile[(y * tw + x) * 4..][..4];
                    let a = p[3] as f32 / 255.0;
                    for c in 0..3 {
                        out.push((p[c] as f32 + backdrop[c] * 255.0 * (1.0 - a)).clamp(0.0, 255.0) as u8);
                    }
                }
            }
        }
    }
    std::fs::write(path, out)
}
