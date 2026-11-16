//! Paints one pill frame (`model::Frame`) into an RGBA8 premultiplied buffer.
//!
//! The capsule from Capsule.dc.html with honeycomb v2 (Honeycomb2.dc.html): an ink (#16130F)
//! capsule, 40 pt tall, with 37 soft pointy-top hexagons on its left (rings of 1, 6, 12, 18; each
//! cell bevelled light top-left to shade bottom-right, an orange core with a soft glow) and the
//! live words (drawn by the overlay) to the right. Looks: rest, voice (rings lift from the centre),
//! room noise (slate rim), working (amber sweep on the rim), typed (a lime tick), low memory
//! (amber), problem (coral). Everything is a signed-distance shape evaluated on the CPU, so the
//! same code runs on every OS and in tests. Sizes are points (y down).

use super::model::{cell_pos, ring, tick, Frame, CELLS};

pub const CAPSULE_H: f32 = 40.0;
/// Room above the capsule for the voice ring; the shadow takes the room below.
pub const TOP: f32 = 6.0;

/// The image in points (the capsule plus room for its ring and shadow) and its backing scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Target {
    pub width: f32,
    pub height: f32,
    pub scale: f32,
    /// Only deepens the shadow on a dark desktop; the capsule itself is ink on both.
    pub dark: bool,
}

impl Target {
    pub fn pixels(&self) -> (usize, usize) {
        ((self.width * self.scale).round().max(1.0) as usize, (self.height * self.scale).round().max(1.0) as usize)
    }
}

type Rgb = [f32; 3];

const fn hex(v: u32) -> Rgb {
    [((v >> 16) & 255) as f32 / 255.0, ((v >> 8) & 255) as f32 / 255.0, (v & 255) as f32 / 255.0]
}

const INK: Rgb = hex(0x16130F);
const ORANGE: Rgb = hex(0xFF7A45);
const RING_ORANGE: Rgb = hex(0xF2541B);
const NOISE: Rgb = hex(0x9FB0C2);
const AMBER: Rgb = hex(0xFFC24B);
const LIME: Rgb = hex(0xB8F06A);
const CORAL: Rgb = hex(0xFF6B5B);

fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

fn smooth(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// 1 inside the shape, 0 outside, soft over `aa` points.
fn cover(distance: f32, aa: f32) -> f32 {
    smooth(aa, -aa, distance)
}

fn rounded_box(x: f32, y: f32, hw: f32, hh: f32, r: f32) -> f32 {
    let (qx, qy) = (x.abs() - hw + r, y.abs() - hh + r);
    let (ox, oy) = (qx.max(0.0), qy.max(0.0));
    (ox * ox + oy * oy).sqrt() + qx.max(qy).min(0.0) - r
}

/// "Over" in premultiplied space.
fn over(dest: &mut [f32; 4], color: Rgb, alpha: f32) {
    let a = alpha.clamp(0.0, 1.0);
    for i in 0..3 {
        dest[i] = color[i] * a + dest[i] * (1.0 - a);
    }
    dest[3] = a + dest[3] * (1.0 - a);
}

/// The comb is 32 pt square (a 92-unit comb in a 100-unit box): the design's spacing unit `s` in
/// points, the hexagon radius (0.82 s: clear gaps between cells), and where the comb's middle sits
/// from the capsule's left edge (10 pt padding + half the comb).
pub const PITCH: f32 = 92.0 / 11.6 * 0.32;
/// 0.82 s in Honeycomb2; the 32 pt small-size variant fills the cells a little more so they read.
const HEX: f32 = 0.9;
pub const LATTICE_X: f32 = 26.0;

const CREAM_STOPS: [Rgb; 3] = [hex(0xFFFFFF), hex(0xF1EADB), hex(0xCDBFA7)];
const CORE_STOPS: [Rgb; 3] = [hex(0xFFC9AA), hex(0xFF7A45), hex(0xB8380B)];

/// At 32 pt a strong bevel turns to mud: keep it, but gentle.
fn flat(stops: [Rgb; 3]) -> [Rgb; 3] {
    [mix(stops[1], stops[0], 0.5), stops[1], mix(stops[1], stops[2], 0.45)]
}

/// A flat colour as a bevel: lighter top-left, the colour itself, shade bottom-right.
fn bevel(c: Rgb) -> [Rgb; 3] {
    [mix(c, [1.0, 1.0, 1.0], 0.4), c, mix(c, [0.0, 0.0, 0.0], 0.25)]
}

/// One cell's look: scale of the hexagon, opacity and its bevel stops.
#[derive(Clone, Copy)]
struct Look {
    k: f32,
    alpha: f32,
    stops: [Rgb; 3],
}

fn blend(a: Look, b: Look, t: f32) -> Look {
    if t <= 0.0 {
        return a;
    }
    Look { k: a.k + (b.k - a.k) * t, alpha: a.alpha + (b.alpha - a.alpha) * t, stops: [mix(a.stops[0], b.stops[0], t), mix(a.stops[1], b.stops[1], t), mix(a.stops[2], b.stops[2], t)] }
}

/// The design's look of cell `i` for this frame, blended between states.
fn look(frame: &Frame, i: usize) -> Look {
    let g = ring(i);
    let (x, y) = cell_pos(i);
    let v = frame.rings[g];
    // The small-size variant of Honeycomb2 (a 32 pt comb): brighter rest opacities, a core that
    // reads as orange at a glance, and inner rings that visibly swell (and warm) with the voice.
    // A slow breath keeps it alive while quiet.
    const REST: [f32; 4] = [1.0, 0.92, 0.78, 0.62];
    const VOICE: [f32; 4] = [1.0, 1.0, 0.9, 0.7];
    const REST_K: [f32; 4] = [1.15, 0.96, 0.92, 0.9];
    const VOICE_K: [f32; 4] = [1.22, 1.02, 0.96, 0.92];
    let breath = 0.5 + 0.5 * (std::f32::consts::TAU * frame.time / 2.4 - g as f32 * 0.5).sin();
    let warm = if g == 1 { 0.3 + 0.55 * v } else { 0.0 }; // ring 1 carries the core's colour, more on voice
    let mut l = Look {
        k: (REST_K[g] + (VOICE_K[g] - REST_K[g]) * v) * (0.985 + 0.03 * breath * (1.0 - v)),
        alpha: REST[g] + (VOICE[g] - REST[g]) * v,
        stops: if g == 0 { CORE_STOPS } else { flat([0, 1, 2].map(|k| mix(CREAM_STOPS[k], CORE_STOPS[k], warm))) },
    };
    // Room noise: the rim turns slate, heard and ignored.
    if g == 3 {
        let hush = (smooth(0.45, 0.9, frame.noise) * (1.0 - v.max(frame.rings[0])).powi(2)).clamp(0.0, 1.0);
        l = blend(l, Look { k: 0.92, alpha: 0.8, stops: bevel(NOISE) }, hush);
    }
    // Models loading: the comb fills in from the middle.
    if frame.warming > 0.001 {
        let lit = ((frame.warming * 1.3 - g as f32 / 3.0 * 0.45) * 3.0).clamp(0.0, 1.0);
        l.alpha *= 0.25 + 0.75 * lit;
    }
    // Working: a warm amber sweep circles the rim; the inside rests low.
    let w = ((y.atan2(x) + 0.9 - frame.time * 4.0).cos() + 1.0) / 2.0;
    let working = match g {
        0 => Look { k: 0.92, alpha: 0.9, stops: bevel(AMBER) },
        3 => Look { k: 0.84 + 0.16 * w, alpha: 0.15 + 0.85 * w, stops: bevel(AMBER) },
        _ => Look { k: 0.9, alpha: 0.45, stops: bevel(AMBER) },
    };
    l = blend(l, working, frame.working);
    // Typed: seven cells form a tick, the rest fade back.
    let typed = if tick(i) { Look { k: 1.04, alpha: 1.0, stops: bevel(LIME) } } else { Look { k: 0.88, alpha: 0.28, stops: flat(CREAM_STOPS) } };
    l = blend(l, typed, frame.done);
    // Low memory / adjust something: even amber, still.
    l = blend(l, Look { k: 0.9, alpha: 0.6, stops: bevel(AMBER) }, frame.amber);
    // Problem: calm coral.
    blend(l, Look { k: 0.92, alpha: if g >= 2 { 0.6 } else { 0.95 }, stops: bevel(CORAL) }, frame.fail)
}

/// Signed distance to a pointy-top hexagon of circumradius `r`, corners rounded by `round`.
fn hexagon(x: f32, y: f32, r: f32, round: f32) -> f32 {
    // iq's flat-top hexagon with x and y swapped (so a corner points up), inradius r·√3/2.
    let (k0, k1, k2) = (-0.866_025_4f32, 0.5f32, 0.577_350_3f32);
    let inner = r * 0.866_025_4 - round;
    let (mut px, mut py) = (y.abs(), x.abs());
    let d = 2.0 * (k0 * px + k1 * py).min(0.0);
    px -= d * k0;
    py -= d * k1;
    let qx = px - px.clamp(-k2 * inner, k2 * inner);
    let qy = py - inner;
    (qx * qx + qy * qy).sqrt() * qy.signum() - round
}

pub fn paint(frame: &Frame, target: &Target, out: &mut Vec<u8>) {
    let (w, h) = target.pixels();
    out.clear();
    out.resize(w * h * 4, 0);
    if frame.opacity <= 0.002 || frame.reveal <= 0.01 {
        return;
    }
    // Eases up from 0.88: a gentle arrival, no bounce to speak of.
    let zoom = (0.88 + 0.12 * frame.reveal).max(0.05);
    let scale = target.scale;
    let aa = 0.75 / (scale * zoom);
    let (hw, hh) = (frame.width / 2.0, CAPSULE_H / 2.0);
    let cy0 = TOP + hh; // the capsule's middle, from the image's top
    let shadow_alpha = if target.dark { 0.30 } else { 0.14 };
    // A faint halo: warm with the voice while listening, lime once typed.
    let voice_ring = 3.0 * frame.level * (1.0 - frame.working) * (1.0 - frame.done);
    let done_ring = 2.0 * frame.done * (1.0 - frame.fail);

    let mut buf = vec![[0.0f32; 4]; w * h];
    let col_x: Vec<f32> = (0..w).map(|x| ((x as f32 + 0.5) / scale - target.width / 2.0 - frame.shake) / zoom).collect();
    let row_y: Vec<f32> = (0..h).map(|y| ((y as f32 + 0.5) / scale - cy0) / zoom).collect();

    // Pass 1: shadow (0 8px 20px), the ring, the ink capsule.
    for y in 0..h {
        for x in 0..w {
            let (px, py) = (col_x[x], row_y[y]);
            let d = rounded_box(px, py, hw, hh, hh);
            if d > 22.0 {
                continue;
            }
            let c = &mut buf[y * w + x];
            let soft = rounded_box(px, py - 6.0, hw - 2.0, hh - 1.0, hh);
            over(c, INK, shadow_alpha * (1.0 - smooth(-12.0, 14.0, soft)));
            if d > 0.0 {
                if voice_ring > 0.05 {
                    over(c, RING_ORANGE, 0.12 * (1.0 - smooth(0.0, voice_ring + 1.5, d)));
                }
                if done_ring > 0.05 {
                    over(c, LIME, 0.22 * (1.0 - smooth(0.0, done_ring + 1.5, d)));
                }
            }
            over(c, INK, cover(d, aa));
        }
    }

    // Pass 2: a soft orange glow behind the core, then the comb. Each cell only touches the
    // pixels near it.
    let to_pixel = |cx: f32, cy: f32| (((cx * zoom) + target.width / 2.0 + frame.shake) * scale - 0.5, ((cy * zoom) + cy0) * scale - 0.5);
    let glow = 0.4 * (0.35 + 0.65 * frame.rings[0]) * (1.0 - frame.working) * (1.0 - frame.done) * (1.0 - frame.fail) * (1.0 - frame.amber);
    if glow > 0.01 {
        let (gx, gy) = to_pixel(-hw + LATTICE_X, 0.0);
        let reach = 3.2 * PITCH * zoom * scale;
        let (x0, x1) = ((gx - reach).max(0.0) as usize, ((gx + reach) as usize).min(w - 1));
        let (y0, y1) = ((gy - reach).max(0.0) as usize, ((gy + reach) as usize).min(h - 1));
        for y in y0..=y1 {
            for x in x0..=x1 {
                let d = ((x as f32 - gx).powi(2) + (y as f32 - gy).powi(2)).sqrt() / reach;
                if d < 1.0 {
                    over(&mut buf[y * w + x], ORANGE, glow * (1.0 - d).powi(2));
                }
            }
        }
    }
    for i in 0..CELLS {
        let (ux, uy) = cell_pos(i);
        let (cx, cy) = (-hw + LATTICE_X + ux * PITCH, uy * PITCH);
        let l = look(frame, i);
        let r = HEX * PITCH * l.k * zoom; // circumradius in points
        let round = 0.5 * r; // the design cuts each corner back 28 % of a side
        let (pxc, pyc) = to_pixel(cx, cy);
        let reach = (r * scale).ceil() as i32 + 2;
        let (x0, x1) = ((pxc as i32 - reach).max(0), (pxc as i32 + reach).min(w as i32 - 1));
        let (y0, y1) = ((pyc as i32 - reach).max(0), (pyc as i32 + reach).min(h as i32 - 1));
        for y in y0..=y1 {
            for x in x0..=x1 {
                let (dx, dy) = ((x as f32 - pxc) / scale, (y as f32 - pyc) / scale);
                let coverage = cover(hexagon(dx, dy, r, round), aa * zoom);
                if coverage <= 0.0 {
                    continue;
                }
                // the bevel runs from the top-left of the cell's box to its bottom-right
                let t = (((dx / (1.732 * r)) + 0.5 + (dy / (2.0 * r)) + 0.5) / 2.0).clamp(0.0, 1.0);
                let colour = if t < 0.55 { mix(l.stops[0], l.stops[1], t / 0.55) } else { mix(l.stops[1], l.stops[2], (t - 0.55) / 0.45) };
                over(&mut buf[y as usize * w + x as usize], colour, l.alpha * coverage);
            }
        }
    }

    // Pass 3: to bytes, with the capsule's overall opacity.
    for (i, c) in buf.iter().enumerate() {
        for k in 0..4 {
            out[i * 4 + k] = (c[k] * frame.opacity).clamp(0.0, 1.0).mul_add(255.0, 0.5) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::model::{Bands, Event, Model, NoticeKind, BASE_WIDTH};
    use super::*;
    use std::time::{Duration, Instant};

    fn frame_after(events: &[(Event, f32)], dark: bool) -> (Frame, Target) {
        let mut now = Instant::now();
        let mut model = Model::new(now);
        let mut frame = model.step(now, 1.0 / 60.0);
        for (event, seconds) in events {
            model.handle(*event, now);
            for k in 0..(seconds * 60.0) as usize {
                now += Duration::from_micros(16_667);
                if let Event::Started = event {
                    model.handle(Event::Meter(Bands::flat(0.02 + 0.08 * ((k as f32 * 0.4).sin().abs()))), now);
                }
                frame = model.step(now, 1.0 / 60.0);
            }
        }
        (frame, Target { width: 360.0, height: 64.0, scale: 2.0, dark })
    }

    #[test]
    fn a_listening_capsule_is_painted_in_the_middle_and_clear_at_the_corners() {
        let (frame, target) = frame_after(&[(Event::Started, 0.8)], true);
        let mut buffer = Vec::new();
        paint(&frame, &target, &mut buffer);
        let (w, h) = target.pixels();
        assert_eq!(buffer.len(), w * h * 4);
        let mid = (TOP + CAPSULE_H / 2.0) as usize * 2;
        assert!(buffer[(mid * w + w / 2 + 10) * 4 + 3] > 200, "the capsule is opaque");
        assert_eq!(buffer[3], 0, "the corner is transparent");
    }

    #[test]
    fn a_hidden_pill_paints_nothing() {
        let now = Instant::now();
        let mut model = Model::new(now);
        let frame = model.step(now, 1.0 / 60.0);
        let mut buffer = Vec::new();
        paint(&frame, &Target { width: 360.0, height: 64.0, scale: 1.0, dark: false }, &mut buffer);
        assert!(buffer.iter().all(|b| *b == 0));
    }

    #[test]
    fn every_look_paints_something_different() {
        let looks: [&[(Event, f32)]; 6] = [
            &[(Event::Started, 0.8)],
            &[(Event::Started, 0.5), (Event::Submitted, 0.7)],
            &[(Event::Started, 0.5), (Event::Submitted, 0.2), (Event::Completed, 0.7)],
            &[(Event::Notice(NoticeKind::Error), 0.4)],
            &[(Event::Notice(NoticeKind::Warn), 0.4)],
            &[(Event::Started, 0.3), (Event::Warming(true), 0.6)],
        ];
        let sums: Vec<u64> = looks
            .iter()
            .map(|events| {
                let (frame, target) = frame_after(events, false);
                let mut buffer = Vec::new();
                paint(&frame, &target, &mut buffer);
                buffer.iter().map(|b| *b as u64).sum()
            })
            .collect();
        for i in 0..sums.len() {
            for j in i + 1..sums.len() {
                assert_ne!(sums[i], sums[j], "looks {i} and {j} should differ");
            }
        }
    }

    #[test]
    fn the_lattice_sits_inside_the_capsule_and_clear_of_the_words() {
        // every hexagon, at its largest, lies within the capsule's padding and left of the words
        let r = HEX * PITCH * 1.12;
        for i in 0..CELLS {
            let (x, y) = cell_pos(i);
            let (cx, cy) = (LATTICE_X + x * PITCH, y * PITCH);
            assert!(cx - r >= 9.0 && cy.abs() + r < CAPSULE_H / 2.0, "cell {i} at ({cx}, {cy})");
            assert!(cx + r <= super::super::model::TEXT_LEFT - 8.0, "cell {i} reaches the words");
        }
        // at rest the capsule is just the comb (58 pt: 10 + 32 + 16)
        assert!(LATTICE_X + 16.0 + 16.0 <= BASE_WIDTH);
    }

    #[test]
    fn hexagons_point_up_and_round_their_corners() {
        assert!(hexagon(0.0, 0.0, 10.0, 5.0) < 0.0);
        assert!(hexagon(0.0, -9.5, 10.0, 0.0) < 0.0, "a corner points up");
        assert!(hexagon(9.0, 0.0, 10.0, 0.0) > 0.0, "the sides are flat (inradius 8.66)");
        assert!(hexagon(0.0, -9.5, 10.0, 5.0) > 0.0, "the rounded corner is cut back");
        assert!(hexagon(8.5, 0.0, 10.0, 5.0) < 0.0, "but the flat sides stay where they were");
    }
}

#[cfg(test)]
mod bench {
    use super::super::model::{Bands, Event, Model};
    use super::*;
    use std::time::Instant;

    #[test]
    #[ignore = "timing only: cargo test --release paint::bench -- --ignored --nocapture"]
    fn frame_cost() {
        let now = Instant::now();
        let mut model = Model::new(now);
        model.handle(Event::Started, now);
        model.handle(Event::Meter(Bands::flat(0.1)), now);
        let mut frame = model.step(now, 1.0 / 60.0);
        for _ in 0..60 {
            frame = model.step(now, 1.0 / 60.0);
        }
        for scale in [1.0f32, 2.0, 2.3] {
            let target = Target { width: 360.0, height: 64.0, scale, dark: true };
            let mut buffer = Vec::new();
            let start = Instant::now();
            for _ in 0..50 {
                paint(&frame, &target, &mut buffer);
            }
            println!("scale {scale}: {:.2} ms/frame", start.elapsed().as_secs_f64() * 1000.0 / 50.0);
        }
    }
}
