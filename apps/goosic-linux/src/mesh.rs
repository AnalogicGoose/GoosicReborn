/*
 * mesh.rs — the palette, seeded layout, and motion behind the full-screen and mini player's
 * procedural background, and the rule that asks YouTube's image servers for a larger cover.
 *
 * Ported from the previous Goosic application and apps/goosic-swift/Sources/GoosicSwift/Core/NowPlayingMesh.swift:
 * src/components/layout/now-playing-background.tsx (palette extraction, the seeded mesh grid) and
 * src/index.css (the drift and breathe keyframes), plus getHighResVariant from
 * src/components/shared/thumbnail.tsx. That implementation was written independently from the
 * technique described by frigopedro/Apple-Music-Background, which carries no license; no code
 * from that repository is used here.
 *
 * Copyright (C) Oscar Mantilla, George Shyshov, and Goosic contributors.
 *
 * This program is free software: you can redistribute it and/or modify it under the terms of
 * the GNU General Public License as published by the Free Software Foundation, version 3.
 *
 * This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
 * without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
 * See the GNU General Public License for more details. A copy is in LICENSE-GPL-3.0 at the
 * repository root.
 */

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, glib, graphene, gsk};

use crate::artwork::ArtworkCache;

pub const SAMPLE_SIDE: usize = 48;
pub const GRID_SIDE: usize = 6;
pub const PALETTE_SIZE: usize = 5;

/// One colour the cover really contains, and how common it is relative to the most common one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeshSample {
    pub red: f64,
    pub green: f64,
    pub blue: f64,
    pub weight: f64,
}

/// One blob of the mesh: which sampled colour it paints and where it sits in its grid slot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeshCell {
    pub color: MeshSample,
    pub x: f64,
    pub y: f64,
    pub scale: f64,
}

/// Where one breathing blob is at a moment in time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeshCellPose {
    pub opacity: f64,
    pub dx: f64,
    pub dy: f64,
    pub scale: f64,
}

/// Where the whole drifting field is at a moment in time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeshDriftPose {
    pub dx: f64,
    pub dy: f64,
    pub scale: f64,
    pub degrees: f64,
}

pub const RESTING_DRIFT: MeshDriftPose = MeshDriftPose {
    dx: -0.02,
    dy: -0.01,
    scale: 1.12,
    degrees: -1.0,
};

pub const RESTING_CELL: MeshCellPose = MeshCellPose {
    opacity: 1.0,
    dx: -0.03,
    dy: -0.02,
    scale: 1.0,
};

/// The cover's dominant colours, most common first, from `width x height` RGBA bytes.
pub fn palette(rgba: &[u8], width: usize, height: usize) -> Option<Vec<MeshSample>> {
    if width == 0 || height == 0 || rgba.len() < width * height * 4 {
        return None;
    }

    struct Bucket {
        red: f64,
        green: f64,
        blue: f64,
        count: usize,
    }

    let mut buckets: HashMap<u16, Bucket> = HashMap::new();
    let mut order: Vec<u16> = Vec::new();

    for y in (0..height).step_by(2) {
        for x in (0..width).step_by(2) {
            let offset = (y * width + x) * 4;
            if rgba[offset + 3] < 200 {
                continue;
            }
            let r = rgba[offset];
            let g = rgba[offset + 1];
            let b = rgba[offset + 2];
            let key = (u16::from(r >> 4) << 8) | (u16::from(g >> 4) << 4) | u16::from(b >> 4);
            let entry = buckets.entry(key).or_insert_with(|| {
                order.push(key);
                Bucket {
                    red: 0.0,
                    green: 0.0,
                    blue: 0.0,
                    count: 0,
                }
            });
            entry.red += f64::from(r);
            entry.green += f64::from(g);
            entry.blue += f64::from(b);
            entry.count += 1;
        }
    }

    let mut candidates: Vec<(usize, f64, f64, f64, usize)> = order
        .into_iter()
        .enumerate()
        .map(|(rank, key)| {
            let bucket = &buckets[&key];
            let count = bucket.count as f64;
            (
                rank,
                bucket.red / count,
                bucket.green / count,
                bucket.blue / count,
                bucket.count,
            )
        })
        .collect();

    candidates.sort_by(|a, b| b.4.cmp(&a.4).then_with(|| a.0.cmp(&b.0)));

    let mut selected: Vec<(f64, f64, f64, usize)> = Vec::new();
    for (_, red, green, blue, count) in candidates {
        let distinct = selected.iter().all(|(sr, sg, sb, _)| {
            let dr = red - sr;
            let dg = green - sg;
            let db = blue - sb;
            (dr * dr + dg * dg + db * db).sqrt() > 36.0
        });
        if distinct {
            selected.push((red, green, blue, count));
        }
        if selected.len() == PALETTE_SIZE {
            break;
        }
    }

    let largest = selected.first()?.3 as f64;
    let mut samples: Vec<MeshSample> = selected
        .into_iter()
        .map(|(red, green, blue, count)| MeshSample {
            red: red.round(),
            green: green.round(),
            blue: blue.round(),
            weight: count as f64 / largest,
        })
        .collect();
    let distinct_count = samples.len();
    while samples.len() < PALETTE_SIZE {
        samples.push(samples[samples.len() % distinct_count]);
    }
    Some(samples)
}

/// A 32-bit FNV-1a of the palette, so each cover gets its own arrangement and the same cover
/// always gets the same one.
pub fn seed(palette: &[MeshSample]) -> u32 {
    let mut hash: u32 = 2_166_136_261;
    for sample in palette {
        let text = format!(
            "rgb({} {} {}):{:.4}",
            sample.red as i32, sample.green as i32, sample.blue as i32, sample.weight
        );
        for unit in text.encode_utf16() {
            hash = (hash ^ u32::from(unit)).wrapping_mul(16_777_619);
        }
    }
    hash
}

/// The field's blobs, row by row, placed by a generator seeded from the palette.
pub fn cells(palette: &[MeshSample]) -> Vec<MeshCell> {
    if palette.is_empty() {
        return Vec::new();
    }
    let mut state = seed(palette);
    if state == 0 {
        state = 1;
    }
    let mut random = || {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        f64::from(state) / 4_294_967_296.0
    };
    let total_weight: f64 = palette.iter().map(|s| s.weight).sum();
    let mut result = Vec::with_capacity(GRID_SIDE * GRID_SIDE);
    for _ in 0..(GRID_SIDE * GRID_SIDE) {
        let mut target = random() * total_weight;
        let mut color = palette[0];
        for sample in palette {
            target -= sample.weight;
            if target <= 0.0 {
                color = *sample;
                break;
            }
        }
        let x = (20.0 + random() * 60.0) / 100.0;
        let y = (20.0 + random() * 60.0) / 100.0;
        let scale = 1.05 + random() * 0.45;
        result.push(MeshCell { color, x, y, scale });
    }
    result
}

pub fn alternating_progress(time: f64, duration: f64, delay: f64) -> f64 {
    if duration <= 0.0 {
        return 0.0;
    }
    let elapsed = (time - delay) / duration;
    let cycle = elapsed.floor();
    let fraction = elapsed - cycle;
    let odd = (cycle as i64).rem_euclid(2) != 0;
    if odd {
        1.0 - fraction
    } else {
        fraction
    }
}

pub fn ease(progress: f64) -> f64 {
    (1.0 - (std::f64::consts::PI * progress.clamp(0.0, 1.0)).cos()) / 2.0
}

/// The whole field's slow 28-second drift, through three keyframes.
pub fn drift(time: f64) -> MeshDriftPose {
    const FRAMES: [MeshDriftPose; 3] = [
        MeshDriftPose {
            dx: -0.03,
            dy: -0.02,
            scale: 1.12,
            degrees: -1.5,
        },
        MeshDriftPose {
            dx: 0.02,
            dy: 0.03,
            scale: 1.17,
            degrees: 1.0,
        },
        MeshDriftPose {
            dx: 0.04,
            dy: -0.01,
            scale: 1.13,
            degrees: 2.0,
        },
    ];
    let progress = alternating_progress(time, 28.0, -8.0);
    let (from, to, local) = if progress < 0.5 {
        (FRAMES[0], FRAMES[1], progress / 0.5)
    } else {
        (FRAMES[1], FRAMES[2], (progress - 0.5) / 0.5)
    };
    let eased = ease(local);
    let mix = |a: f64, b: f64| a + (b - a) * eased;
    MeshDriftPose {
        dx: mix(from.dx, to.dx),
        dy: mix(from.dy, to.dy),
        scale: mix(from.scale, to.scale),
        degrees: mix(from.degrees, to.degrees),
    }
}

/// One blob's breathing. Each cell has its own period and phase, and every third runs in reverse.
pub fn breathe(index: usize, time: f64) -> MeshCellPose {
    let duration = 18.0 + (index % 7) as f64 * 2.0;
    let delay = -((index as f64 * 1.37) % 19.0);
    let mut eased = ease(alternating_progress(time, duration, delay));
    if index % 3 == 1 {
        eased = 1.0 - eased;
    }
    MeshCellPose {
        opacity: 0.82 + 0.18 * eased,
        dx: -0.05 + 0.10 * eased,
        dy: -0.04 + 0.08 * eased,
        scale: 0.94 + 0.14 * eased,
    }
}

/// A larger render of a YouTube Music cover, or `None` when no upgrade applies.
pub fn high_resolution_variant(url: &str, size: u32) -> Option<String> {
    if let Some(pos) = url.find("=w") {
        let rest = &url[pos + 2..];
        let w_digits = rest.bytes().take_while(|b| b.is_ascii_digit()).count();
        if w_digits > 0 && rest[w_digits..].starts_with("-h") {
            let after_h = &rest[w_digits + 2..];
            let h_digits = after_h.bytes().take_while(|b| b.is_ascii_digit()).count();
            if h_digits > 0 {
                let end = pos + 2 + w_digits + 2 + h_digits;
                return Some(format!("{}=w{size}-h{size}{}", &url[..pos], &url[end..]));
            }
        }
    }
    if let Some(pos) = url.find("=s") {
        let rest = &url[pos + 2..];
        let digits = rest.bytes().take_while(|b| b.is_ascii_digit()).count();
        if digits > 0 {
            let end = pos + 2 + digits;
            return Some(format!("{}=s{size}{}", &url[..pos], &url[end..]));
        }
    }
    if let Some(vi) = url.find("/vi/") {
        let after_vi = &url[vi + 4..];
        if let Some(slash) = after_vi.find('/') {
            let file = &after_vi[slash + 1..];
            if let Some(dot_jpg) = file.find(".jpg") {
                if !file[..dot_jpg].contains('/') && !file[..dot_jpg].contains('.') {
                    let prefix = &url[..vi + 4 + slash + 1];
                    let suffix = &file[dot_jpg + 4..];
                    return Some(format!("{prefix}maxresdefault.jpg{suffix}"));
                }
            }
        }
    }
    if (url.contains("lh3.googleusercontent.com") || url.contains("yt3.ggpht.com"))
        && !url.contains('=')
    {
        return Some(format!("{url}=w{size}-h{size}-l90-rj"));
    }
    None
}

/// Samples a `gdk::Texture` down to a `SAMPLE_SIDE x SAMPLE_SIDE` RGBA buffer in pure Rust.
/// `gdk::Texture::download` writes BGRA premultiplied bytes (`B8G8R8A8` on little-endian);
/// swapping B and R recovers RGBA cleanly for palette extraction and artist hero tinting.
pub fn sample_texture_rgba(texture: &gdk::Texture) -> Option<(Vec<u8>, usize, usize)> {
    let w = usize::try_from(texture.width()).ok()?;
    let h = usize::try_from(texture.height()).ok()?;
    if w == 0 || h == 0 {
        return None;
    }
    let stride = w.checked_mul(4)?;
    let mut raw = vec![0u8; stride.checked_mul(h)?];
    texture.download(&mut raw, stride);

    let out_w = SAMPLE_SIDE.min(w);
    let out_h = SAMPLE_SIDE.min(h);
    let mut rgba = vec![0u8; out_w * out_h * 4];
    for oy in 0..out_h {
        let sy = (oy * h) / out_h;
        for ox in 0..out_w {
            let sx = (ox * w) / out_w;
            let src = sy * stride + sx * 4;
            let dst = (oy * out_w + ox) * 4;
            let b = raw[src];
            let g = raw[src + 1];
            let r = raw[src + 2];
            let a = raw[src + 3];
            rgba[dst] = r;
            rgba[dst + 1] = g;
            rgba[dst + 2] = b;
            rgba[dst + 3] = a;
        }
    }
    Some((rgba, out_w, out_h))
}

/// Samples the dominant artist page tint colour from a banner/portrait texture using
/// `goosic_shell_support::hero::tint`.
pub fn sample_hero_tint(texture: &gdk::Texture) -> Option<(f32, f32, f32)> {
    let (rgba, w, h) = sample_texture_rgba(texture)?;
    let [r, g, b] = goosic_shell_support::hero::tint(&rgba, w, h)?;
    Some((f32::from(r) / 255.0, f32::from(g) / 255.0, f32::from(b) / 255.0))
}

mod imp {
    use super::*;

    pub struct MeshWidget {
        pub cells: RefCell<Vec<MeshCell>>,
        pub elapsed: Cell<f64>,
        pub reduce_motion: Cell<bool>,
        pub high_contrast: Cell<bool>,
        pub started_at: Cell<Instant>,
        pub tick: RefCell<Option<glib::SourceId>>,
        pub on_tick: RefCell<Option<Rc<dyn Fn()>>>,
    }

    impl Default for MeshWidget {
        fn default() -> Self {
            Self {
                cells: RefCell::new(Vec::new()),
                elapsed: Cell::new(0.0),
                reduce_motion: Cell::new(false),
                high_contrast: Cell::new(false),
                started_at: Cell::new(Instant::now()),
                tick: RefCell::new(None),
                on_tick: RefCell::new(None),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for MeshWidget {
        const NAME: &'static str = "GoosicMeshBackdrop";
        type Type = super::MeshWidget;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for MeshWidget {
        fn dispose(&self) {
            if let Some(id) = self.tick.borrow_mut().take() {
                id.remove();
            }
        }
    }

    impl WidgetImpl for MeshWidget {
        fn map(&self) {
            self.parent_map();
            self.obj().sync_animation();
        }

        fn unmap(&self) {
            if let Some(id) = self.tick.borrow_mut().take() {
                id.remove();
            }
            self.parent_unmap();
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let obj = self.obj();
            let width = f64::from(obj.width());
            let height = f64::from(obj.height());
            if width <= 0.0 || height <= 0.0 {
                return;
            }
            let bounds = graphene::Rect::new(0.0, 0.0, width as f32, height as f32);
            if self.high_contrast.get() {
                snapshot.append_color(&gdk::RGBA::new(0.0, 0.0, 0.0, 1.0), &bounds);
                return;
            }

            let cells = self.cells.borrow();
            let base = if let Some(first) = cells.first() {
                gdk::RGBA::new(
                    (first.color.red / 255.0 * 0.24) as f32,
                    (first.color.green / 255.0 * 0.24) as f32,
                    (first.color.blue / 255.0 * 0.24) as f32,
                    1.0,
                )
            } else {
                gdk::RGBA::new(0.05, 0.04, 0.08, 1.0)
            };

            snapshot.push_clip(&bounds);
            snapshot.append_color(&base, &bounds);

            if !cells.is_empty() {
                let animated = !self.reduce_motion.get();
                let t = self.elapsed.get();
                let d = if animated { drift(t) } else { RESTING_DRIFT };

                // Naturally soft Gaussian-bell radial clouds stay silky smooth at full resolution
                // without an expensive full-viewport blur pass on software or modest GPU renderers.
                snapshot.save();
                let cx = (width * 0.5) as f32;
                let cy = (height * 0.5) as f32;
                snapshot.translate(&graphene::Point::new(
                    cx + (d.dx * width) as f32,
                    cy + (d.dy * height) as f32,
                ));
                snapshot.rotate(d.degrees as f32);
                snapshot.scale(d.scale as f32, d.scale as f32);
                snapshot.translate(&graphene::Point::new(-cx, -cy));

                let field = width.max(height) * 1.18;
                let ox = (width - field) * 0.5;
                let oy = (height - field) * 0.5;
                let slot = field / GRID_SIDE as f64;

                for (index, cell) in cells.iter().enumerate() {
                    let pose = if animated {
                        breathe(index, t)
                    } else {
                        RESTING_CELL
                    };
                    let col = (index % GRID_SIDE) as f64;
                    let row = (index / GRID_SIDE) as f64;
                    let bx = ox + (col + cell.x + pose.dx) * slot;
                    let by = oy + (row + cell.y + pose.dy) * slot;
                    let radius = (slot * 0.98 * cell.scale * pose.scale) as f32;

                    let r = (cell.color.red / 255.0) as f32;
                    let g = (cell.color.green / 255.0) as f32;
                    let b = (cell.color.blue / 255.0) as f32;
                    let a = (0.52 * pose.opacity) as f32;

                    let blob_bounds = graphene::Rect::new(
                        bx as f32 - radius,
                        by as f32 - radius,
                        radius * 2.0,
                        radius * 2.0,
                    );
                    snapshot.append_radial_gradient(
                        &blob_bounds,
                        &graphene::Point::new(bx as f32, by as f32),
                        radius,
                        radius,
                        0.0,
                        1.0,
                        &[
                            gsk::ColorStop::new(0.0, gdk::RGBA::new(r, g, b, a)),
                            gsk::ColorStop::new(0.28, gdk::RGBA::new(r, g, b, a * 0.74)),
                            gsk::ColorStop::new(0.56, gdk::RGBA::new(r, g, b, a * 0.34)),
                            gsk::ColorStop::new(0.80, gdk::RGBA::new(r, g, b, a * 0.10)),
                            gsk::ColorStop::new(1.0, gdk::RGBA::new(r, g, b, 0.0)),
                        ],
                    );
                }

                snapshot.restore();
            }

            // Readability scrim keeping white typography legible over bright album covers.
            snapshot.append_color(&gdk::RGBA::new(0.0, 0.0, 0.0, 0.30), &bounds);
            snapshot.pop();
        }
    }
}

glib::wrapper! {
    pub struct MeshWidget(ObjectSubclass<imp::MeshWidget>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for MeshWidget {
    fn default() -> Self {
        let widget: Self = glib::Object::new();
        widget.set_can_target(false);
        widget.set_hexpand(true);
        widget.set_vexpand(true);
        widget
    }
}

impl MeshWidget {
    pub fn set_on_tick(&self, cb: Rc<dyn Fn()>) {
        *self.imp().on_tick.borrow_mut() = Some(cb);
    }

    pub fn set_reduce_motion(&self, reduce_motion: bool) {
        if self.imp().reduce_motion.replace(reduce_motion) != reduce_motion {
            self.sync_animation();
            self.queue_draw();
        }
    }

    pub fn set_high_contrast(&self, high_contrast: bool) {
        if self.imp().high_contrast.replace(high_contrast) != high_contrast {
            self.sync_animation();
            self.queue_draw();
        }
    }

    pub fn set_cells(&self, new_cells: Vec<MeshCell>) {
        *self.imp().cells.borrow_mut() = new_cells;
        self.sync_animation();
        self.queue_draw();
    }

    fn sync_animation(&self) {
        let imp = self.imp();
        let should_run = self.is_mapped()
            && self.is_visible()
            && !imp.reduce_motion.get()
            && !imp.high_contrast.get()
            && !imp.cells.borrow().is_empty();
        if !should_run {
            if let Some(id) = imp.tick.borrow_mut().take() {
                id.remove();
            }
            return;
        }
        if imp.tick.borrow().is_some() {
            return;
        }
        imp.started_at.set(Instant::now());
        let weak = self.downgrade();
        let id = glib::timeout_add_local(Duration::from_millis(50), move || {
            let Some(widget) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let imp = widget.imp();
            if !widget.is_mapped()
                || !widget.is_visible()
                || imp.reduce_motion.get()
                || imp.high_contrast.get()
            {
                imp.tick.borrow_mut().take();
                return glib::ControlFlow::Break;
            }
            imp.elapsed.set(imp.started_at.get().elapsed().as_secs_f64());
            widget.queue_draw();
            if let Some(cb) = imp.on_tick.borrow().as_ref() {
                cb();
            }
            glib::ControlFlow::Continue
        });
        *imp.tick.borrow_mut() = Some(id);
    }
}

/// Manages artwork loading and palette extraction for a `MeshWidget`.
pub struct MeshBackdrop {
    pub widget: MeshWidget,
    cache: Rc<ArtworkCache>,
    shown: RefCell<Option<String>>,
    pending: RefCell<Option<gtk::Image>>,
}

impl MeshBackdrop {
    pub fn new(cache: Rc<ArtworkCache>) -> Self {
        Self {
            widget: MeshWidget::default(),
            cache,
            shown: RefCell::new(None),
            pending: RefCell::new(None),
        }
    }

    pub fn set_options(&self, reduce_motion: bool, high_contrast: bool) {
        self.widget.set_reduce_motion(reduce_motion);
        self.widget.set_high_contrast(high_contrast);
    }

    pub fn update(&self, remote: Option<&str>) {
        if self.shown.borrow().as_deref() == remote {
            return;
        }
        *self.shown.borrow_mut() = remote.map(str::to_owned);
        self.pending.borrow_mut().take();
        let Some(remote) = remote else {
            self.widget.set_cells(Vec::new());
            return;
        };
        let image = gtk::Image::new();
        let weak = self.widget.downgrade();
        image.connect_paintable_notify(move |image| {
            let Some(widget) = weak.upgrade() else {
                return;
            };
            let Some(texture) = image.paintable().and_downcast::<gdk::Texture>() else {
                return;
            };
            if let Some((rgba, w, h)) = sample_texture_rgba(&texture) {
                if let Some(pal) = palette(&rgba, w, h) {
                    widget.set_cells(cells(&pal));
                }
            }
        });
        self.cache.show(Some(remote), &image);
        *self.pending.borrow_mut() = Some(image);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_and_cells_are_deterministic_and_repeat_distinct_colors() {
        let mut rgba = vec![0u8; 16 * 16 * 4];
        for y in 0..16 {
            for x in 0..16 {
                let offset = (y * 16 + x) * 4;
                if x < 10 {
                    rgba[offset..offset + 4].copy_from_slice(&[240, 20, 80, 255]);
                } else {
                    rgba[offset..offset + 4].copy_from_slice(&[30, 80, 220, 255]);
                }
            }
        }
        let pal = palette(&rgba, 16, 16).expect("non-empty palette");
        assert_eq!(pal.len(), PALETTE_SIZE);
        assert_eq!(pal[0].weight, 1.0);
        assert!(pal[0].red > 200.0);
        assert!(pal[1].blue > 180.0);
        let first = cells(&pal);
        let second = cells(&pal);
        assert_eq!(first.len(), GRID_SIDE * GRID_SIDE);
        assert_eq!(first, second);
    }

    #[test]
    fn drift_and_breathe_stay_bounded() {
        for step in 0..40 {
            let t = f64::from(step) * 1.7;
            let d = drift(t);
            assert!((1.10..=1.20).contains(&d.scale));
            let b = breathe(step as usize, t);
            assert!((0.80..=1.01).contains(&b.opacity));
            assert!((0.90..=1.10).contains(&b.scale));
        }
    }

    #[test]
    fn high_resolution_variant_upgrades_google_and_youtube_urls() {
        assert_eq!(
            high_resolution_variant("https://lh3.googleusercontent.com/abc=w120-h120-l90-rj", 1080)
                .as_deref(),
            Some("https://lh3.googleusercontent.com/abc=w1080-h1080-l90-rj")
        );
        assert_eq!(
            high_resolution_variant("https://yt3.ggpht.com/xyz=s120-c-k", 1080).as_deref(),
            Some("https://yt3.ggpht.com/xyz=s1080-c-k")
        );
        assert_eq!(
            high_resolution_variant("https://i.ytimg.com/vi/dQw4w9WgXcQ/hqdefault.jpg", 1080)
                .as_deref(),
            Some("https://i.ytimg.com/vi/dQw4w9WgXcQ/maxresdefault.jpg")
        );
        assert_eq!(
            high_resolution_variant("https://lh3.googleusercontent.com/bare", 1080).as_deref(),
            Some("https://lh3.googleusercontent.com/bare=w1080-h1080-l90-rj")
        );
    }
}
