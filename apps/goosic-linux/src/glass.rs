//! Real frosted-glass blur container for GTK 4 frames.
//!
//! GTK 4 CSS has no `backdrop-filter: blur()`, so this widget captures the sibling scene behind
//! it inside GSK's render graph (`push_rounded_clip` -> `push_blur` -> translated underlay
//! snapshot) and composites a frosted veil, top specular highlight and subtle rim border before
//! snapshotting its child controls on top.
//!
//! High-contrast mode bypasses blur and transparency completely to draw solid, crisp surfaces.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, glib, graphene, gsk};

thread_local! {
    /// Prevents recursive glass snapshots if an underlay ever references a widget containing glass.
    static IN_GLASS_SNAPSHOT: Cell<bool> = const { Cell::new(false) };
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GlassKind {
    /// Flush full-height docked sidebar.
    Sidebar,
    /// Narrow overlay drawer sidebar over page content.
    SidebarOverlay,
    /// Right-hand Lyrics / Up Next side panel.
    SidePanel,
    /// Floating bottom playback capsule (full pill).
    #[default]
    PlayerPill,
    /// Narrow two-row floating playback capsule.
    PlayerStacked,
    /// Floating transparent Search entry pill.
    SearchPill,
    /// Linked segmented category selector pill.
    SegmentedPill,
    /// Immersive or floating glass capsule (Full Player / Mini Player chrome, sidebar toggle).
    Capsule,
    /// Circular frosted icon button (Like, More, Artist Hero About/Subscribe).
    Circle,
    /// Rounded glass card (Find bar, inline notices, settings card).
    Card,
}

impl GlassKind {
    fn blur_radius(self) -> f32 {
        match self {
            Self::Sidebar => 28.0,
            Self::SidebarOverlay | Self::SidePanel => 34.0,
            Self::PlayerPill | Self::PlayerStacked => 32.0,
            Self::SearchPill => 28.0,
            Self::SegmentedPill => 24.0,
            Self::Capsule => 26.0,
            Self::Circle => 22.0,
            Self::Card => 24.0,
        }
    }

    fn corner_radius(self, height: f32) -> f32 {
        match self {
            Self::Sidebar | Self::SidebarOverlay | Self::SidePanel => 0.0,
            Self::PlayerPill
            | Self::SearchPill
            | Self::SegmentedPill
            | Self::Capsule
            | Self::Circle => height / 2.0,
            Self::PlayerStacked => 26.0_f32.min(height / 2.0),
            Self::Card => 14.0_f32.min(height / 2.0),
        }
    }

    fn tint(self, light: bool) -> gdk::RGBA {
        match (self, light) {
            (Self::Sidebar, false) => gdk::RGBA::new(0.08, 0.07, 0.12, 0.50),
            (Self::Sidebar, true) => gdk::RGBA::new(0.95, 0.95, 0.98, 0.62),
            (Self::SidebarOverlay, false) => gdk::RGBA::new(0.09, 0.08, 0.13, 0.72),
            (Self::SidebarOverlay, true) => gdk::RGBA::new(0.96, 0.96, 0.99, 0.80),
            (Self::SidePanel, false) => gdk::RGBA::new(0.08, 0.07, 0.13, 0.68),
            (Self::SidePanel, true) => gdk::RGBA::new(0.95, 0.95, 0.98, 0.78),
            (Self::PlayerPill | Self::PlayerStacked, false) => gdk::RGBA::new(0.13, 0.11, 0.19, 0.56),
            (Self::PlayerPill | Self::PlayerStacked, true) => gdk::RGBA::new(1.0, 1.0, 1.0, 0.68),
            (Self::SearchPill, false) => gdk::RGBA::new(0.14, 0.12, 0.20, 0.52),
            (Self::SearchPill, true) => gdk::RGBA::new(1.0, 1.0, 1.0, 0.70),
            (Self::SegmentedPill, false) => gdk::RGBA::new(0.12, 0.10, 0.18, 0.54),
            (Self::SegmentedPill, true) => gdk::RGBA::new(0.94, 0.94, 0.97, 0.68),
            (Self::Capsule, _) => gdk::RGBA::new(0.12, 0.11, 0.16, 0.34),
            (Self::Circle, _) => gdk::RGBA::new(0.12, 0.11, 0.16, 0.30),
            (Self::Card, false) => gdk::RGBA::new(0.12, 0.11, 0.18, 0.56),
            (Self::Card, true) => gdk::RGBA::new(1.0, 1.0, 1.0, 0.72),
        }
    }

    fn specular_top(self, light: bool) -> gdk::RGBA {
        match (self, light) {
            (Self::Sidebar | Self::SidebarOverlay | Self::SidePanel, _) => {
                gdk::RGBA::new(1.0, 1.0, 1.0, 0.04)
            }
            (Self::Capsule | Self::Circle, _) => gdk::RGBA::new(1.0, 1.0, 1.0, 0.14),
            (_, false) => gdk::RGBA::new(1.0, 1.0, 1.0, 0.11),
            (_, true) => gdk::RGBA::new(1.0, 1.0, 1.0, 0.36),
        }
    }

    fn border(self, light: bool) -> ([f32; 4], gdk::RGBA) {
        let color = match (self, light) {
            (Self::Capsule | Self::Circle, _) => gdk::RGBA::new(1.0, 1.0, 1.0, 0.24),
            (_, false) => gdk::RGBA::new(1.0, 1.0, 1.0, 0.20),
            (_, true) => gdk::RGBA::new(0.0, 0.0, 0.0, 0.11),
        };
        let widths = match self {
            Self::Sidebar | Self::SidebarOverlay => [0.0, 1.0, 0.0, 0.0],
            Self::SidePanel => [0.0, 0.0, 0.0, 1.0],
            _ => [1.0, 1.0, 1.0, 1.0],
        };
        (widths, color)
    }

    fn shadow(self, light: bool) -> Option<(f32, f32, f32, gdk::RGBA)> {
        let alpha = if light { 0.14 } else { 0.34 };
        match self {
            Self::PlayerPill | Self::PlayerStacked => {
                Some((0.0, 10.0, 28.0, gdk::RGBA::new(0.0, 0.0, 0.0, alpha)))
            }
            Self::SearchPill | Self::Capsule | Self::Card => {
                Some((0.0, 8.0, 22.0, gdk::RGBA::new(0.0, 0.0, 0.0, alpha * 0.85)))
            }
            Self::SegmentedPill | Self::Circle => {
                Some((0.0, 5.0, 16.0, gdk::RGBA::new(0.0, 0.0, 0.0, alpha * 0.72)))
            }
            Self::SidebarOverlay => Some((8.0, 0.0, 28.0, gdk::RGBA::new(0.0, 0.0, 0.0, alpha))),
            Self::Sidebar | Self::SidePanel => None,
        }
    }
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct GlassFrame {
        pub child: RefCell<Option<gtk::Widget>>,
        pub underlay: glib::WeakRef<gtk::Widget>,
        pub kind: Cell<GlassKind>,
        pub light: Cell<bool>,
        pub high_contrast: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for GlassFrame {
        const NAME: &'static str = "GoosicGlassFrame";
        type Type = super::GlassFrame;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_layout_manager_type::<gtk::BinLayout>();
            klass.set_css_name("goosic-glass");
        }
    }

    impl ObjectImpl for GlassFrame {
        fn dispose(&self) {
            if let Some(child) = self.child.borrow_mut().take() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for GlassFrame {
        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let obj = self.obj();
            let width = obj.width() as f32;
            let height = obj.height() as f32;
            if width > 0.0 && height > 0.0 {
                let kind = self.kind.get();
                let light = self.light.get();
                let high_contrast = self.high_contrast.get();
                let bounds = graphene::Rect::new(0.0, 0.0, width, height);
                let radius = kind.corner_radius(height);
                let rounded = gsk::RoundedRect::from_rect(bounds, radius);

                if high_contrast {
                    snapshot.push_rounded_clip(&rounded);
                    snapshot.append_color(&gdk::RGBA::new(0.0, 0.0, 0.0, 1.0), &bounds);
                    snapshot.pop();
                    let white = gdk::RGBA::new(1.0, 1.0, 1.0, 1.0);
                    let widths = match kind {
                        GlassKind::Sidebar | GlassKind::SidebarOverlay => [0.0, 2.0, 0.0, 0.0],
                        GlassKind::SidePanel => [0.0, 0.0, 0.0, 2.0],
                        _ => [2.0, 2.0, 2.0, 2.0],
                    };
                    snapshot.append_border(&rounded, &widths, &[white, white, white, white]);
                } else {
                    if let Some((dx, dy, blur, color)) = kind.shadow(light) {
                        snapshot.append_outset_shadow(&rounded, &color, dx, dy, 0.0, blur);
                    }
                    snapshot.push_rounded_clip(&rounded);

                    // Opaque base prevents sharp underlying pixels from bleeding through the blur.
                    let base = if matches!(kind, GlassKind::Capsule | GlassKind::Circle) {
                        gdk::RGBA::new(0.06, 0.05, 0.09, 1.0)
                    } else if light {
                        gdk::RGBA::new(0.96, 0.96, 0.98, 1.0)
                    } else {
                        gdk::RGBA::new(0.09, 0.08, 0.13, 1.0)
                    };
                    snapshot.append_color(&base, &bounds);

                    let should_blur = IN_GLASS_SNAPSHOT.with(|busy| !busy.get());
                    if should_blur {
                        if let Some(underlay) = self.underlay.upgrade() {
                            if let Some(underlay_parent) = underlay.parent() {
                                if let Some(rel) = obj.compute_bounds(&underlay) {
                                    let blur_r = kind.blur_radius();
                                    // Clip tightly around this frame (+blur margin) so GSK only
                                    // blurs the slice behind the glass instead of the full window.
                                    let sample_rect = graphene::Rect::new(
                                        -blur_r,
                                        -blur_r,
                                        width + blur_r * 2.0,
                                        height + blur_r * 2.0,
                                    );
                                    IN_GLASS_SNAPSHOT.with(|busy| busy.set(true));
                                    snapshot.push_blur(f64::from(blur_r));
                                    snapshot.push_clip(&sample_rect);
                                    snapshot.append_color(&base, &sample_rect);
                                    snapshot.save();
                                    snapshot.translate(&graphene::Point::new(-rel.x(), -rel.y()));
                                    underlay_parent.snapshot_child(&underlay, snapshot);
                                    snapshot.restore();
                                    snapshot.pop();
                                    snapshot.pop();
                                    IN_GLASS_SNAPSHOT.with(|busy| busy.set(false));
                                }
                            }
                        }
                    }

                    // Translucent frosted veil.
                    snapshot.append_color(&kind.tint(light), &bounds);

                    // Subtle top-edge luminous sheen for depth without Liquid Glass distortion.
                    let top = kind.specular_top(light);
                    let clear = gdk::RGBA::new(1.0, 1.0, 1.0, 0.0);
                    snapshot.append_linear_gradient(
                        &bounds,
                        &graphene::Point::new(0.0, 0.0),
                        &graphene::Point::new(0.0, height.min(48.0)),
                        &[
                            gsk::ColorStop::new(0.0, top),
                            gsk::ColorStop::new(1.0, clear),
                        ],
                    );

                    snapshot.pop();

                    let (widths, border_color) = kind.border(light);
                    snapshot.append_border(
                        &rounded,
                        &widths,
                        &[border_color, border_color, border_color, border_color],
                    );
                }
            }

            if let Some(child) = self.child.borrow().as_ref() {
                obj.snapshot_child(child, snapshot);
            }
        }
    }
}

glib::wrapper! {
    pub struct GlassFrame(ObjectSubclass<imp::GlassFrame>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl GlassFrame {
    pub fn new(kind: GlassKind) -> Self {
        let frame: Self = glib::Object::new();
        frame.imp().kind.set(kind);
        frame
    }

    pub fn set_child(&self, child: &impl IsA<gtk::Widget>) {
        if let Some(previous) = self.imp().child.borrow_mut().take() {
            previous.unparent();
        }
        let widget = child.upcast_ref::<gtk::Widget>();
        widget.set_parent(self);
        *self.imp().child.borrow_mut() = Some(widget.clone());
    }

    pub fn set_underlay(&self, underlay: &impl IsA<gtk::Widget>) {
        let widget = underlay.upcast_ref::<gtk::Widget>();
        self.imp().underlay.set(Some(widget));
        self.queue_draw();
    }

    pub fn set_kind(&self, kind: GlassKind) {
        if self.imp().kind.replace(kind) != kind {
            self.queue_draw();
        }
    }

    pub fn set_appearance(&self, light: bool, high_contrast: bool) {
        let imp = self.imp();
        let changed =
            imp.light.replace(light) != light || imp.high_contrast.replace(high_contrast) != high_contrast;
        if changed {
            self.queue_draw();
        }
    }
}

/// Coordinates redraw invalidation and theme updates for a set of frosted-glass frames sharing
/// an underlay scene.
#[derive(Clone, Default)]
pub struct GlassGroup {
    frames: Rc<RefCell<Vec<glib::WeakRef<GlassFrame>>>>,
    light: Rc<Cell<bool>>,
    high_contrast: Rc<Cell<bool>>,
}

impl GlassGroup {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn wrap(
        &self,
        child: &impl IsA<gtk::Widget>,
        kind: GlassKind,
        underlay: Option<&gtk::Widget>,
    ) -> GlassFrame {
        let frame = GlassFrame::new(kind);
        frame.set_child(child);
        if let Some(underlay) = underlay {
            frame.set_underlay(underlay);
        }
        frame.set_appearance(self.light.get(), self.high_contrast.get());
        self.register(&frame);
        frame
    }

    pub fn register(&self, frame: &GlassFrame) {
        frame.set_appearance(self.light.get(), self.high_contrast.get());
        self.frames.borrow_mut().push(frame.downgrade());
    }

    pub fn set_underlay_all(&self, underlay: &impl IsA<gtk::Widget>) {
        let widget = underlay.upcast_ref::<gtk::Widget>();
        self.frames.borrow_mut().retain(|weak| {
            if let Some(frame) = weak.upgrade() {
                frame.set_underlay(widget);
                true
            } else {
                false
            }
        });
    }

    pub fn set_appearance(&self, light: bool, high_contrast: bool) {
        self.light.set(light);
        self.high_contrast.set(high_contrast);
        self.frames.borrow_mut().retain(|weak| {
            if let Some(frame) = weak.upgrade() {
                frame.set_appearance(light, high_contrast);
                true
            } else {
                false
            }
        });
    }

    pub fn invalidate(&self) {
        self.frames.borrow_mut().retain(|weak| {
            if let Some(frame) = weak.upgrade() {
                if frame.is_visible() && frame.is_mapped() {
                    frame.queue_draw();
                }
                true
            } else {
                false
            }
        });
    }
}
