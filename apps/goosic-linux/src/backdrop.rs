//! A cropped, blurred copy of the playing cover. GTK's render graph keeps blur off the UI thread.

use crate::artwork::ArtworkCache;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, glib, graphene};
use std::cell::RefCell;
use std::rc::Rc;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct Backdrop {
        pub image: RefCell<Option<gdk::Paintable>>,
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
            let iw = f64::from(image.intrinsic_width().max(1));
            let ih = f64::from(image.intrinsic_height().max(1));
            let scale = ((width + 160.0) / iw).max((height + 160.0) / ih);
            let (w, h) = (iw * scale, ih * scale);
            snapshot.push_clip(&graphene::Rect::new(0.0, 0.0, width as f32, height as f32));
            snapshot.push_opacity(0.32);
            snapshot.push_blur(60.0);
            snapshot.save();
            snapshot.translate(&graphene::Point::new(
                ((width - w) / 2.0) as f32,
                ((height - h) / 2.0) as f32,
            ));
            image.snapshot(snapshot, w, h);
            snapshot.restore();
            snapshot.pop();
            snapshot.pop();
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
}

impl ArtworkBackdrop {
    pub fn new(cache: Rc<ArtworkCache>) -> Self {
        Self {
            widget: Backdrop::default(),
            cache,
            shown: RefCell::new(None),
            pending: RefCell::new(None),
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
        let Some(remote) = remote else { return };
        let image = gtk::Image::new();
        let weak = self.widget.downgrade();
        image.connect_paintable_notify(move |image| {
            if let Some(widget) = weak.upgrade() {
                *widget.imp().image.borrow_mut() = image.paintable();
                widget.queue_draw();
            }
        });
        self.cache.show(Some(remote), &image);
        *self.pending.borrow_mut() = Some(image);
    }
}
