//! The queue and lyrics panels beside the page and inside the Full Player.
//!
//! `LyricsPanel` implements the macOS word-timed lyrics presentation driven by
//! `goosic_shell_support::lyrics_motion`: progressive per-word highlight sweeps, animated
//! three-dot instrumental interludes, contextual distance blur (sharp on hover, focus, manual
//! scroll, or High Contrast), smooth spring following anchored at 36% viewport height, and
//! session-only local timed-lyrics import.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Instant;

use goosic_protocol::LyricsLine;
use goosic_shell_support::catalog::Track;
use goosic_shell_support::lyrics_motion::{
    self, covered, empty_line, interlude_after, introduction, padding, rest_start, sweeps, target,
    Interlude, Sweep,
};
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, glib, graphene, gsk, pango};

use crate::artwork::ArtworkCache;
use crate::lyrics::Lyrics;

#[derive(Clone, Copy)]
pub enum QueueEdit {
    Remove(usize),
    Move(usize, usize),
    ClearUpcoming,
    ClearPlayed,
    UndoClear,
}

pub struct QueuePanel {
    pub root: gtk::Box,
    count: gtk::Label,
    undo: gtk::Button,
    cache: Rc<ArtworkCache>,
    edit: Rc<dyn Fn(QueueEdit)>,
    list: gtk::ListBox,
    drawn: RefCell<Option<(Vec<Track>, Option<usize>)>>,
}

impl QueuePanel {
    /// `on_play` receives the index of the row the user chose.
    pub fn new(
        on_play: impl Fn(usize) + 'static,
        on_edit: impl Fn(QueueEdit) + 'static,
        cache: Rc<ArtworkCache>,
    ) -> QueuePanel {
        let edit: Rc<dyn Fn(QueueEdit)> = Rc::new(on_edit);
        let count = dim("");
        let list = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .build();
        list.set_placeholder(Some(&dim(
            "Empty. Play something from the catalog to build a queue.",
        )));
        list.connect_row_activated(move |_, row| {
            if let Ok(index) = usize::try_from(row.index()) {
                on_play(index);
            }
        });
        let root = panel("Playing Next", &count);
        let tools = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        for (label, operation) in [
            ("Clear played", QueueEdit::ClearPlayed),
            ("Clear upcoming", QueueEdit::ClearUpcoming),
        ] {
            let button = gtk::Button::with_label(label);
            let edit = edit.clone();
            button.connect_clicked(move |_| edit(operation));
            tools.append(&button);
        }
        root.append(&tools);
        let undo = gtk::Button::builder()
            .label("Undo clear")
            .visible(false)
            .build();
        {
            let edit = edit.clone();
            undo.connect_clicked(move |_| edit(QueueEdit::UndoClear));
        }
        root.append(&undo);
        root.append(&scrolled(&list));
        QueuePanel {
            root,
            count,
            undo,
            cache,
            edit,
            list,
            drawn: RefCell::new(None),
        }
    }

    pub fn set_undo_available(&self, available: bool) {
        self.undo.set_visible(available);
    }

    /// Draws `queue`, marking `current` — the queue position of the track that is playing.
    pub fn update(&self, queue: &[Track], current: Option<usize>) {
        let key = (queue.to_vec(), current);
        if self.drawn.borrow().as_ref() == Some(&key) {
            return;
        }
        self.count.set_label(&match queue.len() {
            1 => "1 track".to_owned(),
            count => format!("{count} tracks"),
        });
        self.list.remove_all();
        for (index, track) in queue.iter().enumerate() {
            let playing = current == Some(index);
            let title = single_line(&track.title);
            if playing {
                title.set_markup(&format!(
                    "<b>{}</b>",
                    glib::markup_escape_text(&track.title)
                ));
            }
            let detail = if playing {
                format!("Now playing · {}", track.artist)
            } else {
                track.artist.clone()
            };
            let artist = single_line(&detail);
            artist.add_css_class("dim-label");
            let entry = gtk::Box::new(gtk::Orientation::Vertical, 2);
            entry.set_margin_top(4);
            entry.set_margin_bottom(4);
            entry.set_tooltip_text(Some("Play from here"));
            entry.append(&title);
            entry.append(&artist);
            let line = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            line.add_css_class("goosic-queue-row");
            if playing {
                line.add_css_class("goosic-queue-current");
            }
            let image = gtk::Image::builder()
                .icon_name("audio-x-generic-symbolic")
                .pixel_size(40)
                .build();
            self.cache.show(track.thumbnail.as_deref(), &image);
            line.append(&image);
            entry.set_hexpand(true);
            line.append(&entry);
            let controls = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            for (icon, tip, operation, enabled) in [
                (
                    "go-up-symbolic",
                    "Move up",
                    QueueEdit::Move(index, index.saturating_sub(1)),
                    index > 0,
                ),
                (
                    "go-down-symbolic",
                    "Move down",
                    QueueEdit::Move(index, index + 1),
                    index + 1 < queue.len(),
                ),
                (
                    "list-remove-symbolic",
                    "Remove from queue",
                    QueueEdit::Remove(index),
                    !playing,
                ),
            ] {
                let button = gtk::Button::builder()
                    .icon_name(icon)
                    .tooltip_text(tip)
                    .sensitive(enabled)
                    .build();
                button.add_css_class("flat");
                let edit = self.edit.clone();
                button.connect_clicked(move |_| edit(operation));
                controls.append(&button);
            }
            line.append(&controls);
            self.list.append(&line);
        }
        *self.drawn.borrow_mut() = Some(key);
    }
}

// MARK: Custom word-timed lyric line widget

#[derive(Clone)]
pub struct WordSpan {
    byte_start: u32,
    byte_end: u32,
    sweep: Sweep,
}

mod lyric_line_imp {
    use super::*;

    #[derive(Default)]
    pub struct LyricLineWidget {
        pub text: RefCell<String>,
        pub words: RefCell<Vec<WordSpan>>,
        pub immersive: Cell<bool>,
        pub light: Cell<bool>,
        pub high_contrast: Cell<bool>,
        pub synced: Cell<bool>,
        pub active: Cell<bool>,
        pub sung_past: Cell<bool>,
        pub hovered: Cell<bool>,
        pub blur_radius: Cell<f32>,
        pub position_ms: Cell<f64>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for LyricLineWidget {
        const NAME: &'static str = "GoosicLyricLine";
        type Type = super::LyricLineWidget;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for LyricLineWidget {}

    impl WidgetImpl for LyricLineWidget {
        fn request_mode(&self) -> gtk::SizeRequestMode {
            gtk::SizeRequestMode::HeightForWidth
        }

        fn measure(&self, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            let obj = self.obj();
            let layout = obj.make_layout(if orientation == gtk::Orientation::Vertical && for_size > 0 {
                for_size
            } else {
                320
            });
            let (w, h) = layout.pixel_size();
            let pad = if self.immersive.get() { 10 } else { 7 };
            if orientation == gtk::Orientation::Horizontal {
                (60.min(w), w.max(60), -1, -1)
            } else {
                let total = h + pad * 2;
                (total, total, -1, -1)
            }
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let obj = self.obj();
            let width = obj.width();
            let height = obj.height();
            if width <= 0 || height <= 0 {
                return;
            }
            let pad_y = if self.immersive.get() { 10.0 } else { 7.0 };
            let layout = obj.make_layout(width);
            let light = self.light.get() && !self.immersive.get();
            let hc = self.high_contrast.get();
            let active = self.active.get();
            let hovered = self.hovered.get();
            let blur = if hc || hovered || active {
                0.0
            } else {
                self.blur_radius.get()
            };

            let base_rgb = if light && !hc {
                (0.11_f32, 0.11_f32, 0.14_f32)
            } else {
                (1.0_f32, 1.0_f32, 1.0_f32)
            };

            let words = self.words.borrow();
            let has_words = self.synced.get() && !words.is_empty();

            if blur > 0.1 {
                snapshot.push_blur(f64::from(blur));
            }
            snapshot.save();
            snapshot.translate(&graphene::Point::new(0.0, pad_y));

            if hc {
                let attrs = pango::AttrList::new();
                if active {
                    attrs.insert(pango::AttrInt::new_underline(pango::Underline::Single));
                }
                layout.set_attributes(Some(&attrs));
                let alpha = if active || hovered { 1.0 } else { 0.85 };
                let color = gdk::RGBA::new(1.0, 1.0, 1.0, alpha);
                snapshot.append_layout(&layout, &color);
            } else if active && has_words {
                let pos_ms = self.position_ms.get();
                // Draw unsung base slightly translucent, then overlay per-word progressive alphas.
                let attrs = pango::AttrList::new();
                for span in words.iter() {
                    let p = span.sweep.progress(pos_ms);
                    let a = (0.36 + 0.64 * p).clamp(0.36, 1.0);
                    let mut attr = pango::AttrInt::new_foreground_alpha((a * 65535.0) as u16);
                    attr.set_start_index(span.byte_start);
                    attr.set_end_index(span.byte_end);
                    attrs.insert(attr);
                }
                layout.set_attributes(Some(&attrs));

                // Soft luminous halo behind active words in immersive mode.
                if self.immersive.get() {
                    snapshot.push_blur(9.0);
                    let glow = gdk::RGBA::new(1.0, 1.0, 1.0, 0.28);
                    snapshot.append_layout(&layout, &glow);
                    snapshot.pop();
                }

                let color = gdk::RGBA::new(base_rgb.0, base_rgb.1, base_rgb.2, 1.0);
                snapshot.append_layout(&layout, &color);
            } else {
                let alpha = if active || hovered {
                    1.0
                } else if !self.synced.get() {
                    0.86
                } else if self.sung_past.get() {
                    0.34
                } else {
                    0.42
                };
                if active && self.immersive.get() {
                    snapshot.push_blur(10.0);
                    let glow = gdk::RGBA::new(1.0, 1.0, 1.0, 0.30);
                    snapshot.append_layout(&layout, &glow);
                    snapshot.pop();
                }
                let color = gdk::RGBA::new(base_rgb.0, base_rgb.1, base_rgb.2, alpha);
                snapshot.append_layout(&layout, &color);
            }

            snapshot.restore();
            if blur > 0.1 {
                snapshot.pop();
            }
        }
    }
}

glib::wrapper! {
    pub struct LyricLineWidget(ObjectSubclass<lyric_line_imp::LyricLineWidget>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl LyricLineWidget {
    fn new(line: &LyricsLine, synced: bool, immersive: bool) -> Self {
        let widget: Self = glib::Object::new();
        widget.set_hexpand(true);
        widget.set_halign(gtk::Align::Fill);
        let text = if line.text.is_empty() {
            "♪".to_owned()
        } else {
            line.text.clone()
        };
        let spans = build_word_spans(line);
        let imp = widget.imp();
        *imp.text.borrow_mut() = text;
        *imp.words.borrow_mut() = spans;
        imp.synced.set(synced);
        imp.immersive.set(immersive);

        let motion = gtk::EventControllerMotion::new();
        let weak = widget.downgrade();
        motion.connect_enter(move |_, _, _| {
            if let Some(w) = weak.upgrade() {
                w.imp().hovered.set(true);
                w.queue_draw();
            }
        });
        let weak = widget.downgrade();
        motion.connect_leave(move |_| {
            if let Some(w) = weak.upgrade() {
                w.imp().hovered.set(false);
                w.queue_draw();
            }
        });
        widget.add_controller(motion);
        widget
    }

    fn make_layout(&self, width: i32) -> pango::Layout {
        let imp = self.imp();
        let text = imp.text.borrow();
        let layout = self.create_pango_layout(Some(&text));
        layout.set_wrap(pango::WrapMode::WordChar);
        layout.set_width(width.max(60) * pango::SCALE);
        let mut desc = pango::FontDescription::new();
        desc.set_weight(if imp.immersive.get() {
            pango::Weight::Bold
        } else {
            pango::Weight::Semibold
        });
        let size_pt = if imp.immersive.get() { 20 } else { 14 };
        desc.set_size(size_pt * pango::SCALE);
        layout.set_font_description(Some(&desc));
        layout
    }

    fn set_visual(
        &self,
        active: bool,
        sung_past: bool,
        blur_radius: f32,
        position_ms: f64,
        light: bool,
        high_contrast: bool,
    ) {
        let imp = self.imp();
        let has_words = !imp.words.borrow().is_empty();
        let changed = imp.active.replace(active) != active
            || imp.sung_past.replace(sung_past) != sung_past
            || (imp.blur_radius.replace(blur_radius) - blur_radius).abs() > 0.05
            || imp.light.replace(light) != light
            || imp.high_contrast.replace(high_contrast) != high_contrast
            || (active && has_words && (imp.position_ms.replace(position_ms) - position_ms).abs() > 8.0);
        if changed {
            self.queue_draw();
        }
    }
}

fn build_word_spans(line: &LyricsLine) -> Vec<WordSpan> {
    if line.words.is_empty() {
        return Vec::new();
    }
    let word_sweeps = sweeps(line);
    let mut cursor = 0usize;
    let mut out = Vec::with_capacity(line.words.len());
    for (word, sweep) in line.words.iter().zip(word_sweeps) {
        let needle = word.text.as_str();
        if needle.is_empty() {
            continue;
        }
        let start = if let Some(rel) = line.text[cursor..].find(needle) {
            cursor + rel
        } else {
            cursor
        };
        let end = (start + needle.len()).min(line.text.len());
        cursor = end;
        out.push(WordSpan {
            byte_start: start as u32,
            byte_end: end as u32,
            sweep,
        });
    }
    out
}

// MARK: Animated three-dot instrumental interlude widget

mod interlude_imp {
    use super::*;

    pub struct InterludeDots {
        pub interlude: Cell<Interlude>,
        pub position_ms: Cell<f64>,
        pub active: Cell<bool>,
        pub animated: Cell<bool>,
        pub immersive: Cell<bool>,
        pub light: Cell<bool>,
        pub high_contrast: Cell<bool>,
    }

    impl Default for InterludeDots {
        fn default() -> Self {
            Self {
                interlude: Cell::new(Interlude {
                    start_ms: 0,
                    end_ms: 1,
                }),
                position_ms: Cell::new(0.0),
                active: Cell::new(false),
                animated: Cell::new(true),
                immersive: Cell::new(false),
                light: Cell::new(false),
                high_contrast: Cell::new(false),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for InterludeDots {
        const NAME: &'static str = "GoosicInterludeDots";
        type Type = super::InterludeDots;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for InterludeDots {}

    impl WidgetImpl for InterludeDots {
        fn measure(&self, orientation: gtk::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            if orientation == gtk::Orientation::Horizontal {
                (72, 96, -1, -1)
            } else {
                let h = if self.immersive.get() { 34 } else { 26 };
                (h, h, -1, -1)
            }
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            if !self.active.get() {
                return;
            }
            let obj = self.obj();
            let height = obj.height() as f32;
            let interlude = self.interlude.get();
            let pos_ms = self.position_ms.get();
            let scale = interlude.scale(pos_ms, self.animated.get() && !self.high_contrast.get()) as f32;
            if scale <= 0.01 {
                return;
            }
            let base_diameter = if self.immersive.get() { 11.0_f32 } else { 8.0_f32 };
            let diameter = base_diameter * scale;
            let gap = if self.immersive.get() { 12.0_f32 } else { 9.0_f32 };
            let cy = height * 0.5;
            let rgb = if self.light.get() && !self.immersive.get() && !self.high_contrast.get() {
                (0.11_f32, 0.11_f32, 0.14_f32)
            } else {
                (1.0_f32, 1.0_f32, 1.0_f32)
            };

            for dot in 0..3 {
                let opacity = if self.high_contrast.get() {
                    1.0_f32
                } else {
                    interlude.dot_opacity(dot, pos_ms) as f32
                };
                let cx = base_diameter * 0.5 + dot as f32 * (base_diameter + gap);
                let rect = graphene::Rect::new(
                    cx - diameter * 0.5,
                    cy - diameter * 0.5,
                    diameter,
                    diameter,
                );
                let rounded = gsk::RoundedRect::from_rect(rect, diameter * 0.5);
                snapshot.push_rounded_clip(&rounded);
                snapshot.append_color(&gdk::RGBA::new(rgb.0, rgb.1, rgb.2, opacity), &rect);
                snapshot.pop();
            }
        }
    }
}

glib::wrapper! {
    pub struct InterludeDots(ObjectSubclass<interlude_imp::InterludeDots>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl InterludeDots {
    fn new(interlude: Interlude, immersive: bool) -> Self {
        let widget: Self = glib::Object::new();
        widget.set_halign(gtk::Align::Start);
        widget.imp().interlude.set(interlude);
        widget.imp().immersive.set(immersive);
        widget
    }

    fn update(&self, pos_ms: f64, animated: bool, light: bool, high_contrast: bool) {
        let imp = self.imp();
        let interlude = imp.interlude.get();
        let active = pos_ms >= interlude.start_ms as f64 && pos_ms < interlude.end_ms as f64;
        let changed = imp.active.replace(active) != active
            || imp.animated.replace(animated) != animated
            || imp.light.replace(light) != light
            || imp.high_contrast.replace(high_contrast) != high_contrast
            || (active && (imp.position_ms.replace(pos_ms) - pos_ms).abs() > 16.0);
        if changed {
            self.queue_draw();
        }
    }
}

// MARK: True alpha-mask top/bottom viewport fade (zero dark rectangle over colorful backdrops)

mod fade_mask_imp {
    use super::*;

    #[derive(Default)]
    pub struct FadeMask {
        pub child: RefCell<Option<gtk::Widget>>,
        pub enabled: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for FadeMask {
        const NAME: &'static str = "GoosicLyricsFadeMask";
        type Type = super::FadeMask;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_layout_manager_type::<gtk::BinLayout>();
        }
    }

    impl ObjectImpl for FadeMask {
        fn dispose(&self) {
            if let Some(child) = self.child.borrow_mut().take() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for FadeMask {
        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let obj = self.obj();
            let Some(child) = self.child.borrow().clone() else {
                return;
            };
            let w = obj.width() as f32;
            let h = obj.height() as f32;
            if !self.enabled.get() || w <= 0.0 || h <= 140.0 {
                obj.snapshot_child(&child, snapshot);
                return;
            }
            let bounds = graphene::Rect::new(0.0, 0.0, w, h);
            let top = (44.0 / h).clamp(0.04, 0.16);
            let bottom = (1.0 - 60.0 / h).clamp(0.80, 0.96);
            let clear = gdk::RGBA::new(1.0, 1.0, 1.0, 0.0);
            let solid = gdk::RGBA::new(1.0, 1.0, 1.0, 1.0);

            snapshot.push_mask(gsk::MaskMode::Alpha);
            snapshot.append_linear_gradient(
                &bounds,
                &graphene::Point::new(0.0, 0.0),
                &graphene::Point::new(0.0, h),
                &[
                    gsk::ColorStop::new(0.0, clear),
                    gsk::ColorStop::new(top, solid),
                    gsk::ColorStop::new(bottom, solid),
                    gsk::ColorStop::new(1.0, clear),
                ],
            );
            snapshot.pop();
            obj.snapshot_child(&child, snapshot);
            snapshot.pop();
        }
    }
}

glib::wrapper! {
    pub struct FadeMask(ObjectSubclass<fade_mask_imp::FadeMask>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl FadeMask {
    fn new(child: &impl IsA<gtk::Widget>) -> Self {
        let mask: Self = glib::Object::new();
        mask.set_hexpand(true);
        mask.set_vexpand(true);
        mask.imp().enabled.set(true);
        let widget = child.upcast_ref::<gtk::Widget>();
        widget.set_parent(&mask);
        *mask.imp().child.borrow_mut() = Some(widget.clone());
        mask
    }

    fn set_enabled(&self, enabled: bool) {
        if self.imp().enabled.replace(enabled) != enabled {
            self.queue_draw();
        }
    }
}

// MARK: LyricsPanel

enum LyricItem {
    Line {
        index: usize,
        widget: LyricLineWidget,
        container: gtk::Widget,
        rest_ms: Option<i64>,
    },
    Dots {
        interlude: Interlude,
        widget: InterludeDots,
    },
}

struct SpringScroll {
    from: f64,
    to: f64,
    started_at: Instant,
}

pub struct LyricsPanel {
    pub root: gtk::Box,
    header: gtk::Box,
    status: gtk::Label,
    lines: gtk::Box,
    top_spacer: gtk::Box,
    bottom_spacer: gtk::Box,
    fade_mask: FadeMask,
    scroller: gtk::ScrolledWindow,
    items: RefCell<Vec<LyricItem>>,
    drawn_revision: Cell<Option<u64>>,
    active: Rc<Cell<Option<usize>>>,
    following: Rc<Cell<bool>>,
    needs_scroll: Rc<Cell<bool>>,
    resume: gtk::Button,
    seek: Rc<dyn Fn(f64)>,
    seekable: Cell<bool>,
    immersive: Cell<bool>,
    light: Cell<bool>,
    high_contrast: Cell<bool>,
    reduce_motion: Cell<bool>,
    spring: RefCell<Option<SpringScroll>>,
}

impl Default for LyricsPanel {
    fn default() -> Self {
        Self::new(|_| {})
    }
}

impl LyricsPanel {
    pub fn new(seek: impl Fn(f64) + 'static) -> LyricsPanel {
        let status = dim("");
        let lines = gtk::Box::new(gtk::Orientation::Vertical, 4);
        lines.add_css_class("goosic-lyrics-lines");
        let top_spacer = gtk::Box::builder().height_request(120).build();
        let bottom_spacer = gtk::Box::builder().height_request(180).build();
        let scroller = scrolled(&lines);
        let following = Rc::new(Cell::new(true));
        let active = Rc::new(Cell::new(None));
        let needs_scroll = Rc::new(Cell::new(true));
        let resume = gtk::Button::builder()
            .label("Resume lyrics")
            .halign(gtk::Align::End)
            .valign(gtk::Align::End)
            .visible(false)
            .build();
        {
            let (following, needs_scroll) = (following.clone(), needs_scroll.clone());
            resume.connect_clicked(move |button| {
                following.set(true);
                needs_scroll.set(true);
                button.set_visible(false);
            });
        }
        let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
        scroll.set_propagation_phase(gtk::PropagationPhase::Capture);
        {
            let (following, resume) = (following.clone(), resume.clone());
            scroll.connect_scroll(move |_, _, _| {
                following.set(false);
                resume.set_visible(true);
                glib::Propagation::Proceed
            });
        }
        scroller.add_controller(scroll);
        let drag = gtk::GestureClick::new();
        drag.set_propagation_phase(gtk::PropagationPhase::Capture);
        {
            let (following, resume) = (following.clone(), resume.clone());
            drag.connect_pressed(move |_, _, _, _| {
                following.set(false);
                resume.set_visible(true);
            });
        }
        scroller.vscrollbar().add_controller(drag);
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        {
            let (following, resume) = (following.clone(), resume.clone());
            keys.connect_key_pressed(move |_, key, _, _| {
                if matches!(
                    key,
                    gtk::gdk::Key::Up
                        | gtk::gdk::Key::Down
                        | gtk::gdk::Key::Page_Up
                        | gtk::gdk::Key::Page_Down
                        | gtk::gdk::Key::Home
                        | gtk::gdk::Key::End
                ) {
                    following.set(false);
                    resume.set_visible(true);
                }
                glib::Propagation::Proceed
            });
        }
        scroller.add_controller(keys);

        let fade_mask = FadeMask::new(&scroller);
        let viewport = gtk::Overlay::new();
        viewport.set_child(Some(&fade_mask));
        viewport.add_overlay(&resume);
        let (root, header) = panel_with_header("Lyrics", &gtk::Label::new(None));
        {
            let needs_scroll = needs_scroll.clone();
            root.connect_map(move |_| needs_scroll.set(true));
        }
        resume.add_css_class("goosic-resume-lyrics");
        resume.set_margin_end(10);
        resume.set_margin_bottom(10);
        root.append(&status);
        root.append(&viewport);
        LyricsPanel {
            root,
            header,
            status,
            lines,
            top_spacer,
            bottom_spacer,
            fade_mask,
            scroller,
            items: RefCell::default(),
            drawn_revision: Cell::new(None),
            active,
            following,
            needs_scroll,
            resume,
            seek: Rc::new(seek),
            seekable: Cell::new(false),
            immersive: Cell::new(false),
            light: Cell::new(false),
            high_contrast: Cell::new(false),
            reduce_motion: Cell::new(false),
            spring: RefCell::new(None),
        }
    }

    pub fn set_immersive(&self, immersive: bool) {
        self.immersive.set(immersive);
        self.header.set_visible(!immersive);
        self.status.set_visible(!immersive);
    }

    pub fn set_appearance(&self, light: bool, high_contrast: bool, reduce_motion: bool) {
        self.light.set(light);
        self.high_contrast.set(high_contrast);
        self.reduce_motion.set(reduce_motion);
        self.fade_mask.set_enabled(!high_contrast);
    }

    pub fn update(&self, lyrics: &Lyrics, position_seconds: f64) {
        if self.drawn_revision.get() != Some(lyrics.revision()) {
            self.redraw(lyrics);
        }

        let pos_ms = (position_seconds * 1000.0).max(0.0);
        let raw_active = lyrics.active_line(position_seconds);
        let following = self.following.get();
        let light = self.light.get();
        let hc = self.high_contrast.get();
        let animated = !self.reduce_motion.get();

        // Respect supplied line ends: if the active line ended and a rest started, un-highlight it.
        let active = raw_active.filter(|&idx| {
            self.items.borrow().iter().any(|item| match item {
                LyricItem::Line {
                    index,
                    rest_ms: Some(end),
                    ..
                } if *index == idx => pos_ms < *end as f64,
                LyricItem::Line { index, .. } if *index == idx => true,
                _ => false,
            })
        });

        let mut active_widget: Option<gtk::Widget> = None;
        for item in self.items.borrow().iter() {
            match item {
                LyricItem::Line {
                    index,
                    widget,
                    container,
                    ..
                } => {
                    let is_active = active == Some(*index);
                    let sung_past = raw_active.is_some_and(|a| *index < a);
                    let blur = if following && !hc {
                        match raw_active {
                            Some(a) => match index.abs_diff(a) {
                                0 => 0.0,
                                1 => 1.2,
                                2 => 2.2,
                                _ => 3.2,
                            },
                            None => 0.0,
                        }
                    } else {
                        0.0
                    };
                    widget.set_visual(is_active, sung_past, blur, pos_ms, light, hc);
                    if is_active {
                        active_widget = Some(container.clone());
                    }
                }
                LyricItem::Dots { interlude, widget } => {
                    widget.update(pos_ms, animated, light, hc);
                    if active_widget.is_none()
                        && pos_ms >= interlude.start_ms as f64
                        && pos_ms < interlude.end_ms as f64
                    {
                        active_widget = Some(widget.clone().upcast());
                    }
                }
            }
        }

        let follow_again = following && self.needs_scroll.replace(false);
        if active != self.active.get() || follow_again {
            self.active.set(active);
            if following {
                if let Some(target_widget) = active_widget {
                    let adj = self.scroller.vadjustment();
                    let viewport = adj.page_size();
                    if viewport > 0.0 {
                        let (pad_top, pad_bottom) = padding(viewport, self.immersive.get());
                        self.top_spacer.set_height_request(pad_top.round() as i32);
                        self.bottom_spacer
                            .set_height_request(pad_bottom.round() as i32);
                        if let Some(bounds) = target_widget.compute_bounds(&self.lines) {
                            let scrollable = (adj.upper() - viewport).max(0.0);
                            let dest = target(
                                f64::from(bounds.y()),
                                f64::from(bounds.height()),
                                viewport,
                                scrollable,
                            );
                            if !animated || (adj.value() - dest).abs() < 1.0 {
                                *self.spring.borrow_mut() = None;
                                adj.set_value(dest);
                            } else {
                                *self.spring.borrow_mut() = Some(SpringScroll {
                                    from: adj.value(),
                                    to: dest,
                                    started_at: Instant::now(),
                                });
                            }
                        }
                    } else {
                        self.needs_scroll.set(true);
                    }
                }
            }
        }

        // Advance active spring scroll if running.
        if following {
            let mut slot = self.spring.borrow_mut();
            if let Some(spring) = slot.as_ref() {
                let elapsed = spring.started_at.elapsed().as_secs_f64();
                let fraction = covered(elapsed);
                let adj = self.scroller.vadjustment();
                if fraction >= 0.995 || elapsed >= lyrics_motion::ROW_SPRING_PERIOD * 2.5 {
                    adj.set_value(spring.to);
                    *slot = None;
                } else {
                    adj.set_value(spring.from + (spring.to - spring.from) * fraction);
                }
            }
        } else {
            *self.spring.borrow_mut() = None;
        }
    }

    fn redraw(&self, lyrics: &Lyrics) {
        self.status.set_label(lyrics.status());
        while let Some(child) = self.lines.first_child() {
            self.lines.remove(&child);
        }
        let mut items = Vec::new();
        let immersive = self.immersive.get();
        if let Some(document) = lyrics.document() {
            if document.synced {
                self.lines.append(&self.top_spacer);
            }
            if let Some(intro) = introduction(&document.lines, document.synced) {
                let dots = InterludeDots::new(intro, immersive);
                self.lines.append(&dots);
                items.push(LyricItem::Dots {
                    interlude: intro,
                    widget: dots,
                });
            }
            for (index, line) in document.lines.iter().enumerate() {
                if let Some(gap) = empty_line(&document.lines, document.synced, index) {
                    let dots = InterludeDots::new(gap, immersive);
                    self.lines.append(&dots);
                    items.push(LyricItem::Dots {
                        interlude: gap,
                        widget: dots,
                    });
                    continue;
                }
                let widget = LyricLineWidget::new(line, document.synced, immersive);
                let container: gtk::Widget = if document.synced && line.at_ms >= 0 {
                    let button = gtk::Button::builder()
                        .child(&widget)
                        .sensitive(self.seekable.get())
                        .build();
                    button.add_css_class("goosic-lyric-row");
                    let (seek, following, resume, needs_scroll) = (
                        self.seek.clone(),
                        self.following.clone(),
                        self.resume.clone(),
                        self.needs_scroll.clone(),
                    );
                    let seconds = line.at_ms as f64 / 1000.0;
                    button.connect_clicked(move |_| {
                        following.set(true);
                        resume.set_visible(false);
                        needs_scroll.set(true);
                        seek(seconds);
                    });
                    self.lines.append(&button);
                    button.upcast()
                } else {
                    self.lines.append(&widget);
                    widget.clone().upcast()
                };
                items.push(LyricItem::Line {
                    index,
                    widget,
                    container,
                    rest_ms: rest_start(&document.lines, index),
                });
                if let Some(gap) = interlude_after(&document.lines, document.synced, index) {
                    let dots = InterludeDots::new(gap, immersive);
                    self.lines.append(&dots);
                    items.push(LyricItem::Dots {
                        interlude: gap,
                        widget: dots,
                    });
                }
            }
            if document.truncated {
                self.lines.append(&dim(
                    "These lyrics were long, so only the first part is shown.",
                ));
            }
            if document.synced {
                self.lines.append(&self.bottom_spacer);
            }
        }
        *self.items.borrow_mut() = items;
        self.drawn_revision.set(Some(lyrics.revision()));
        self.active.set(None);
        self.following.set(true);
        self.needs_scroll.set(true);
        self.resume.set_visible(false);
        *self.spring.borrow_mut() = None;
        self.scroller.vadjustment().set_value(0.0);
    }

    pub fn set_seekable(&self, seekable: bool) {
        if self.seekable.replace(seekable) == seekable {
            return;
        }
        for item in self.items.borrow().iter() {
            if let LyricItem::Line { container, .. } = item {
                if let Some(button) = container.downcast_ref::<gtk::Button>() {
                    button.set_sensitive(seekable);
                }
            }
        }
    }
}

/// A panel: a heading with a detail on its right, and room below for the body.
fn panel(title: &str, detail: &gtk::Label) -> gtk::Box {
    panel_with_header(title, detail).0
}

fn panel_with_header(title: &str, detail: &gtk::Label) -> (gtk::Box, gtk::Box) {
    let heading = gtk::Label::builder()
        .label(format!("<b>{}</b>", glib::markup_escape_text(title)))
        .use_markup(true)
        .xalign(0.0)
        .hexpand(true)
        .build();
    let header = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    header.append(&heading);
    header.append(detail);
    let root = gtk::Box::new(gtk::Orientation::Vertical, 6);
    root.set_vexpand(true);
    root.set_margin_top(12);
    root.set_margin_bottom(12);
    root.set_margin_start(12);
    root.set_margin_end(12);
    root.append(&header);
    (root, header)
}

fn scrolled(child: &impl IsA<gtk::Widget>) -> gtk::ScrolledWindow {
    gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(child)
        .build()
}

fn dim(text: &str) -> gtk::Label {
    let label = gtk::Label::builder()
        .label(text)
        .xalign(0.0)
        .wrap(true)
        .wrap_mode(pango::WrapMode::WordChar)
        .build();
    label.add_css_class("dim-label");
    label
}

fn single_line(text: &str) -> gtk::Label {
    gtk::Label::builder()
        .label(text)
        .xalign(0.0)
        .ellipsize(pango::EllipsizeMode::End)
        .build()
}
