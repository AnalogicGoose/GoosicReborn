//! Full-window playback presentation. All controls call the same shell methods as the pill.

use crate::artwork::ArtworkCache;
use crate::layout;
use crate::playback::Player;
use crate::player_bar::{BarActions, PlayerBar};
use crate::side_panels::LyricsPanel;
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

pub struct FullPlayer {
    pub root: gtk::Box,
    pub bar: PlayerBar,
    pub lyrics: LyricsPanel,
    body: gtk::Box,
    cover_column: gtk::Box,
    cover_slot: gtk::Box,
    cache: Rc<ArtworkCache>,
    shown: RefCell<Option<String>>,
    queue: RefCell<Option<gtk::Box>>,
}

impl FullPlayer {
    pub fn new(
        actions: Rc<BarActions>,
        cache: Rc<ArtworkCache>,
        close: impl Fn() + 'static,
        fullscreen: impl Fn() + 'static,
    ) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 12);
        root.add_css_class("goosic-full-player");
        root.set_hexpand(true);
        root.set_vexpand(true);
        root.set_visible(false);
        let header = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        let title = gtk::Label::builder()
            .label("NOW PLAYING")
            .hexpand(true)
            .xalign(0.0)
            .build();
        title.add_css_class("dim-label");
        let fill = gtk::Button::builder()
            .icon_name("view-fullscreen-symbolic")
            .tooltip_text("Toggle full screen (F11)")
            .build();
        fill.connect_clicked(move |_| fullscreen());
        let done = gtk::Button::builder()
            .icon_name("go-down-symbolic")
            .tooltip_text("Close full player (Esc)")
            .build();
        done.connect_clicked(move |_| close());
        header.append(&title);
        header.append(&fill);
        header.append(&done);
        root.append(&header);
        let cover_slot = gtk::Box::new(gtk::Orientation::Vertical, 0);
        cover_slot.set_halign(gtk::Align::Center);
        cover_slot.add_css_class("goosic-full-cover");
        cover_slot.set_overflow(gtk::Overflow::Hidden);
        let cover_column = gtk::Box::new(gtk::Orientation::Vertical, 24);
        cover_column.set_hexpand(true);
        cover_column.set_valign(gtk::Align::Center);
        cover_column.append(&cover_slot);
        let bar = PlayerBar::new(actions.clone(), cache.clone());
        bar.full_style();
        cover_column.append(&bar.root);
        let lyrics = LyricsPanel::new(move |position| (actions.seek)(position));
        lyrics.root.add_css_class("goosic-full-lyrics");
        lyrics.root.set_hexpand(true);
        let body = gtk::Box::new(gtk::Orientation::Horizontal, 48);
        body.set_vexpand(true);
        body.append(&cover_column);
        body.append(&lyrics.root);
        root.append(&body);
        Self {
            root,
            bar,
            lyrics,
            body,
            cover_column,
            cover_slot,
            cache,
            shown: RefCell::new(None),
            queue: RefCell::new(None),
        }
    }

    pub fn mount_queue(&self, panel: &gtk::Box) {
        if panel.parent().as_ref() != Some(self.body.upcast_ref()) {
            if let Some(parent) = panel.parent().and_downcast::<gtk::Box>() {
                parent.remove(panel);
            }
            self.body.append(panel);
        }
        *self.queue.borrow_mut() = Some(panel.clone());
    }

    pub fn layout(&self, width: i32, height: i32, lyrics: bool, queue: bool) {
        let auxiliary = lyrics || queue;
        let split = width >= 900 && auxiliary;
        self.body.set_orientation(if split {
            gtk::Orientation::Horizontal
        } else {
            gtk::Orientation::Vertical
        });
        self.body.set_spacing(if split { 48 } else { 12 });
        self.root
            .set_margin_start(if width >= 900 { 48 } else { 20 });
        self.root.set_margin_end(if width >= 900 { 48 } else { 20 });
        self.root.set_margin_top(20);
        self.root.set_margin_bottom(24);
        self.lyrics.root.set_visible(lyrics);
        let size = layout::cover_size(if split { width / 2 } else { width }, height);
        self.cover_slot.set_visible(!auxiliary || split);
        self.cover_slot.set_size_request(size, size);
        if let Some(image) = self.cover_slot.first_child().and_downcast::<gtk::Image>() {
            image.set_pixel_size(size.max(1));
        }
        self.cover_column.set_valign(if auxiliary && !split {
            gtk::Align::End
        } else {
            gtk::Align::Center
        });
        // In the lyrics-only layout, lyrics get the space above the controls.
        let trailing = if queue {
            self.queue.borrow().clone()
        } else {
            Some(self.lyrics.root.clone())
        };
        self.body.reorder_child_after(
            &self.cover_column,
            if auxiliary && !split {
                trailing.as_ref()
            } else {
                None
            },
        );
        self.cover_column.set_vexpand(!auxiliary || split);
    }

    pub fn update(&self, player: &Player, loaded: bool) {
        self.bar.update(player, loaded);
        let remote = player
            .current
            .as_ref()
            .and_then(|track| track.thumbnail.as_deref());
        if self.shown.borrow().as_deref() == remote {
            return;
        }
        *self.shown.borrow_mut() = remote.map(str::to_owned);
        while let Some(child) = self.cover_slot.first_child() {
            self.cover_slot.remove(&child);
        }
        let image = gtk::Image::builder()
            .icon_name("audio-x-generic-symbolic")
            .pixel_size(self.cover_slot.height_request().max(1))
            .build();
        self.cache.show(remote, &image);
        self.cover_slot.append(&image);
    }
}
