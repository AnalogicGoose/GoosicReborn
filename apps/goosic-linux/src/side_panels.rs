//! The queue and lyrics panels beside the page.
//!
//! Both are refreshed from the shell's state every time the player reports, several times a
//! second, so each remembers what it last drew and does the work only when that changed.

use std::cell::{Cell, RefCell};

use goosic_shell_support::catalog::Track;
use gtk::prelude::*;
use gtk::{glib, pango};

use crate::lyrics::Lyrics;

pub struct QueuePanel {
    pub root: gtk::Box,
    count: gtk::Label,
    list: gtk::ListBox,
    drawn: RefCell<Option<(Vec<String>, Option<usize>)>>,
}

impl QueuePanel {
    /// `on_play` receives the index of the row the user chose.
    pub fn new(on_play: impl Fn(usize) + 'static) -> QueuePanel {
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
        let root = panel("Queue", &count);
        root.append(&scrolled(&list));
        QueuePanel {
            root,
            count,
            list,
            drawn: RefCell::new(None),
        }
    }

    /// Draws `queue`, marking `current` — the queue position of the track that is playing.
    pub fn update(&self, queue: &[Track], current: Option<usize>) {
        let key = (
            queue
                .iter()
                .map(|track| track.id.clone())
                .collect::<Vec<_>>(),
            current,
        );
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
            self.list.append(&entry);
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
    active: Cell<Option<usize>>,
}

impl Default for LyricsPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl LyricsPanel {
    pub fn new() -> LyricsPanel {
        let status = dim("");
        let lines = gtk::Box::new(gtk::Orientation::Vertical, 6);
        lines.set_margin_top(6);
        lines.set_margin_bottom(12);
        let scroller = scrolled(&lines);
        let root = panel("Lyrics", &gtk::Label::new(None));
        root.append(&status);
        root.append(&scroller);
        LyricsPanel {
            root,
            status,
            lines,
            scroller,
            labels: RefCell::default(),
            drawn_revision: Cell::new(None),
            active: Cell::new(None),
        }
    }

    pub fn update(&self, lyrics: &Lyrics, position_seconds: f64) {
        if self.drawn_revision.get() != Some(lyrics.revision()) {
            self.redraw(lyrics);
        }
        let active = lyrics.active_line(position_seconds);
        if active == self.active.get() {
            return;
        }
        let labels = self.labels.borrow();
        if let Some((label, text)) = self.active.get().and_then(|index| labels.get(index)) {
            label.set_use_markup(false);
            label.set_label(text);
            label.add_css_class("dim-label");
        }
        if let Some((label, text)) = active.and_then(|index| labels.get(index)) {
            label.set_markup(&format!("<b>{}</b>", glib::markup_escape_text(text)));
            label.remove_css_class("dim-label");
            // A third of the way down, so the lines that come next are in view.
            if let Some(bounds) = label.compute_bounds(&self.lines) {
                let adjustment = self.scroller.vadjustment();
                let target = f64::from(bounds.y()) - adjustment.page_size() / 3.0;
                adjustment.set_value(target.max(0.0));
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
                self.lines.append(&label);
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
        self.scroller.vadjustment().set_value(0.0);
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
