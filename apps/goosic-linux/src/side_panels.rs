//! The queue and lyrics panels beside the page.
//!
//! Both are refreshed from the shell's state every time the player reports, several times a
//! second, so each remembers what it last drew and does the work only when that changed.

use crate::artwork::ArtworkCache;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use goosic_shell_support::catalog::Track;
use gtk::prelude::*;
use gtk::{glib, pango};

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

pub struct LyricsPanel {
    pub root: gtk::Box,
    status: gtk::Label,
    lines: gtk::Box,
    scroller: gtk::ScrolledWindow,
    labels: RefCell<Vec<(gtk::Label, String)>>,
    drawn_revision: Cell<Option<u64>>,
    active: Rc<Cell<Option<usize>>>,
    following: Rc<Cell<bool>>,
    needs_scroll: Rc<Cell<bool>>,
    resume: gtk::Button,
    seek: Rc<dyn Fn(f64)>,
    seekable: Cell<bool>,
}

impl Default for LyricsPanel {
    fn default() -> Self {
        Self::new(|_| {})
    }
}

impl LyricsPanel {
    pub fn new(seek: impl Fn(f64) + 'static) -> LyricsPanel {
        let status = dim("");
        let lines = gtk::Box::new(gtk::Orientation::Vertical, 6);
        lines.add_css_class("goosic-lyrics-lines");
        lines.set_margin_top(6);
        lines.set_margin_bottom(12);
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
        let viewport = gtk::Overlay::new();
        viewport.set_child(Some(&scroller));
        viewport.add_overlay(&resume);
        let root = panel("Lyrics", &gtk::Label::new(None));
        {
            let needs_scroll = needs_scroll.clone();
            root.connect_map(move |_| needs_scroll.set(true));
        }
        root.append(&status);
        root.append(&viewport);
        LyricsPanel {
            root,
            status,
            lines,
            scroller,
            labels: RefCell::default(),
            drawn_revision: Cell::new(None),
            active,
            following,
            needs_scroll,
            resume,
            seek: Rc::new(seek),
            seekable: Cell::new(false),
        }
    }

    pub fn update(&self, lyrics: &Lyrics, position_seconds: f64) {
        if self.drawn_revision.get() != Some(lyrics.revision()) {
            self.redraw(lyrics);
        }
        let active = lyrics.active_line(position_seconds);
        let follow_again = self.following.get() && self.needs_scroll.replace(false);
        if active == self.active.get() && !follow_again {
            return;
        }
        let labels = self.labels.borrow();
        if let Some((label, text)) = self.active.get().and_then(|index| labels.get(index)) {
            label.set_use_markup(false);
            label.set_label(text);
            label.add_css_class("dim-label");
            label.remove_css_class("goosic-lyric-active");
        }
        if let Some((label, text)) = active.and_then(|index| labels.get(index)) {
            label.set_markup(&format!("<b>{}</b>", glib::markup_escape_text(text)));
            label.remove_css_class("dim-label");
            label.add_css_class("goosic-lyric-active");
            // A third of the way down, so the lines that come next are in view.
            if let Some(bounds) = label
                .compute_bounds(&self.lines)
                .filter(|_| self.following.get())
            {
                let adjustment = self.scroller.vadjustment();
                let target = f64::from(bounds.y()) - adjustment.page_size() / 3.0;
                adjustment.set_value(target.max(0.0));
                self.needs_scroll.set(adjustment.page_size() <= 0.0);
            }
        }
        self.active.set(active);
    }

    fn redraw(&self, lyrics: &Lyrics) {
        self.status.set_label(lyrics.status());
        while let Some(child) = self.lines.first_child() {
            self.lines.remove(&child);
        }
        let mut labels = Vec::new();
        if let Some(document) = lyrics.document() {
            for line in &document.lines {
                // An instrumental gap is an empty line; a note keeps its place visible.
                let text = if line.text.is_empty() {
                    "♪".to_owned()
                } else {
                    line.text.clone()
                };
                let label = dim(&text);
                if document.synced && line.at_ms >= 0 {
                    let button = gtk::Button::builder()
                        .child(&label)
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
                } else {
                    self.lines.append(&label);
                }
                labels.push((label, text));
            }
            if document.truncated {
                self.lines.append(&dim(
                    "These lyrics were long, so only the first part is shown.",
                ));
            }
        }
        *self.labels.borrow_mut() = labels;
        self.drawn_revision.set(Some(lyrics.revision()));
        self.active.set(None);
        self.following.set(true);
        self.resume.set_visible(false);
        self.scroller.vadjustment().set_value(0.0);
    }

    pub fn set_seekable(&self, seekable: bool) {
        if self.seekable.replace(seekable) == seekable {
            return;
        }
        for (label, _) in self.labels.borrow().iter() {
            if let Some(button) = label.parent().and_downcast::<gtk::Button>() {
                button.set_sensitive(seekable);
            }
        }
    }
}

/// A panel: a heading with a detail on its right, and room below for the body.
fn panel(title: &str, detail: &gtk::Label) -> gtk::Box {
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
    root
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
