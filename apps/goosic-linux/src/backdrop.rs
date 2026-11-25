//! A full-bleed blurred copy of the playing cover tinted with the window colour, matching the
//! macOS shell's ambient artwork backdrop. GTK's render graph keeps the blur off the UI thread.

use crate::artwork::ArtworkCache;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, glib, graphene};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct Backdrop {
        pub image: RefCell<Option<gdk::Paintable>>,
        pub light: Cell<bool>,
        pub immersive: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Backdrop {
        const NAME: &'static str = "GoosicArtworkBackdrop";
        type Type = super::Backdrop;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for Backdrop {}
    impl WidgetImpl for Backdrop {
        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let obj = self.obj();
            let width = f64::from(obj.width());
            let height = f64::from(obj.height());
            let image = self.image.borrow();
            let Some(image) = image.as_ref() else { return };
            if width <= 0.0 || height <= 0.0 {
                return;
            }
            let bounds = graphene::Rect::new(0.0, 0.0, width as f32, height as f32);
            let iw = f64::from(image.intrinsic_width().max(1));
            let ih = f64::from(image.intrinsic_height().max(1));
            // Overscan well past the blur spread so the blurred edges never fade to transparent.
            let spread = if self.immersive.get() { 260.0 } else { 220.0 };
            let scale = ((width + spread) / iw).max((height + spread) / ih);
            let (w, h) = (iw * scale, ih * scale);

            snapshot.push_clip(&bounds);
            snapshot.push_blur(if self.immersive.get() { 110.0 } else { 88.0 });
            snapshot.save();
            snapshot.translate(&graphene::Point::new(
                ((width - w) / 2.0) as f32,
                ((height - h) / 2.0) as f32,
            ));
            image.snapshot(snapshot, w, h);
            snapshot.restore();
            snapshot.pop();

            // Lay the window's own tint over the blurred cover: deep dark in the full player so
            // vivid covers stay saturated rather than grey, and a readable veil in the browser.
            let veil = if self.immersive.get() {
                gdk::RGBA::new(0.0, 0.0, 0.0, 0.34)
            } else if self.light.get() {
                gdk::RGBA::new(0.97, 0.97, 0.99, 0.78)
            } else {
                gdk::RGBA::new(0.08, 0.07, 0.12, 0.52)
            };
            snapshot.append_color(&veil, &bounds);
            snapshot.pop();
        }
    }
}

glib::wrapper! {
    pub struct Backdrop(ObjectSubclass<imp::Backdrop>)
        @extends gtk::Widget, @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for Backdrop {
    fn default() -> Self {
        let widget: Self = glib::Object::new();
        widget.set_can_target(false);
        widget.set_hexpand(true);
        widget.set_vexpand(true);
        widget
    }
}

pub struct ArtworkBackdrop {
    pub widget: Backdrop,
    cache: Rc<ArtworkCache>,
    shown: RefCell<Option<String>>,
    // Each request owns a fresh image, so late artwork cannot replace the next track's cover.
    pending: RefCell<Option<gtk::Image>>,
    on_update: RefCell<Option<Rc<dyn Fn()>>>,
}

impl ArtworkBackdrop {
    pub fn new(cache: Rc<ArtworkCache>) -> Self {
        Self {
            widget: Backdrop::default(),
            cache,
            shown: RefCell::new(None),
            pending: RefCell::new(None),
            on_update: RefCell::new(None),
        }
    }

    pub fn set_on_update(&self, callback: impl Fn() + 'static) {
        *self.on_update.borrow_mut() = Some(Rc::new(callback));
    }

    pub fn set_appearance(&self, light: bool, immersive: bool) {
        let imp = self.widget.imp();
        let changed = imp.light.replace(light) != light || imp.immersive.replace(immersive) != immersive;
        if changed {
            self.widget.queue_draw();
            if let Some(cb) = self.on_update.borrow().as_ref() {
                cb();
            }
        }
    }

    pub fn update(&self, remote: Option<&str>, enabled: bool) {
        self.widget.set_visible(enabled);
        if self.shown.borrow().as_deref() == remote {
            return;
        }
        *self.shown.borrow_mut() = remote.map(str::to_owned);
        self.pending.borrow_mut().take();
        self.widget.imp().image.borrow_mut().take();
        self.widget.queue_draw();
        if let Some(cb) = self.on_update.borrow().as_ref() {
            cb();
        }
        let Some(remote) = remote else { return };
        let image = gtk::Image::new();
        let weak = self.widget.downgrade();
        let on_update = self.on_update.borrow().clone();
        image.connect_paintable_notify(move |image| {
            if let Some(widget) = weak.upgrade() {
                *widget.imp().image.borrow_mut() = image.paintable();
                widget.queue_draw();
                if let Some(cb) = on_update.as_ref() {
                    cb();
                }
            }
        });
        self.cache.show(Some(remote), &image);
        *self.pending.borrow_mut() = Some(image);
    }
}
