//! Catalog artwork, fetched anonymously and cached on disk.
//!
//! Which hosts may be fetched, how large a thumbnail may be, how many fetches run at once and what
//! a URL is cached as are shared rules from `goosic-shell-support`; what is here is the Linux half.
//! The session is libsoup's own and is given no cookie jar, so a thumbnail request can never become
//! an authenticated one: artwork is public CDN content, fetched as anonymously as a catalog read.
//! Files live in the XDG cache directory, because they can always be fetched again.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::rc::Rc;

use goosic_shell_support::artwork::{cache_key, is_allowed, MAX_BYTES, MAX_CONCURRENT_FETCHES};
use gtk::prelude::*;
use gtk::{gdk, glib};
use soup::prelude::*;

/// Decoded textures kept in memory. Past this the map is emptied and refilled from disk, which is
/// cheap, rather than tracking recency for a cache this small.
const MEMORY_LIMIT: usize = 400;

const TIMEOUT_SECONDS: u32 = 15;

pub struct ArtworkCache {
    directory: PathBuf,
    session: soup::Session,
    textures: RefCell<HashMap<String, gdk::Texture>>,
    /// Images waiting for a URL. Held weakly: a row scrolled away is not kept alive by its artwork.
    waiting: RefCell<HashMap<String, Vec<glib::WeakRef<gtk::Image>>>>,
    in_flight: RefCell<HashSet<String>>,
    queued: RefCell<VecDeque<String>>,
    /// URLs that failed or were refused, so a broken image is not retried on every bind.
    failed: RefCell<HashSet<String>>,
}

impl ArtworkCache {
    pub fn new() -> Rc<ArtworkCache> {
        let session = soup::Session::new();
        session.set_timeout(TIMEOUT_SECONDS);
        Rc::new(ArtworkCache {
            directory: glib::user_cache_dir().join("goosic").join("artwork"),
            session,
            textures: RefCell::default(),
            waiting: RefCell::default(),
            in_flight: RefCell::default(),
            queued: RefCell::default(),
            failed: RefCell::default(),
        })
    }

    /// Shows `remote` in `image` now if it is cached, and as soon as it arrives otherwise. The image
    /// keeps whatever it showed until then, and for good if the fetch fails: artwork is decoration.
    pub fn show(self: &Rc<Self>, remote: Option<&str>, image: &gtk::Image) {
        let Some(remote) = remote.filter(|remote| !remote.is_empty()) else {
            return;
        };
        if let Some(texture) = self.cached(remote) {
            image.set_paintable(Some(&texture));
            return;
        }
        if self.failed.borrow().contains(remote) {
            return;
        }
        self.waiting
            .borrow_mut()
            .entry(remote.to_owned())
            .or_default()
            .push(image.downgrade());
        self.schedule(remote);
    }

    fn file_for(&self, remote: &str) -> PathBuf {
        self.directory.join(format!("{}.img", cache_key(remote)))
    }

    fn cached(&self, remote: &str) -> Option<gdk::Texture> {
        if let Some(texture) = self.textures.borrow().get(remote) {
            return Some(texture.clone());
        }
        let file = self.file_for(remote);
        if !file.exists() {
            return None;
        }
        match gdk::Texture::from_filename(&file) {
            Ok(texture) => {
                self.remember(remote, &texture);
                Some(texture)
            }
            Err(_) => {
                // Only decodable files are written, so this one was damaged; fetch it again.
                let _ = std::fs::remove_file(&file);
                None
            }
        }
    }

    fn remember(&self, remote: &str, texture: &gdk::Texture) {
        let mut textures = self.textures.borrow_mut();
        if textures.len() >= MEMORY_LIMIT {
            textures.clear();
        }
        textures.insert(remote.to_owned(), texture.clone());
    }

    fn schedule(self: &Rc<Self>, remote: &str) {
        if self.in_flight.borrow().contains(remote)
            || self.queued.borrow().iter().any(|q| q == remote)
        {
            return;
        }
        if !is_allowed(remote) {
            self.give_up(remote);
            return;
        }
        if self.in_flight.borrow().len() >= MAX_CONCURRENT_FETCHES {
            self.queued.borrow_mut().push_back(remote.to_owned());
            return;
        }
        self.start(remote.to_owned());
    }

    fn start(self: &Rc<Self>, remote: String) {
        let Ok(message) = soup::Message::new("GET", &remote) else {
            self.give_up(&remote);
            return;
        };
        self.in_flight.borrow_mut().insert(remote.clone());
        let fetched = self
            .session
            .send_and_read_future(&message, glib::Priority::LOW);
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let result = fetched.await;
            let Some(cache) = weak.upgrade() else {
                return;
            };
            cache.in_flight.borrow_mut().remove(&remote);
            let decoded = match result {
                Ok(bytes)
                    if message.status() == soup::Status::Ok
                        && !bytes.is_empty()
                        && bytes.len() <= MAX_BYTES =>
                {
                    gdk::Texture::from_bytes(&bytes)
                        .ok()
                        .map(|texture| (texture, bytes))
                }
                _ => None,
            };
            match decoded {
                Some((texture, bytes)) => {
                    cache.store(&remote, &bytes);
                    cache.remember(&remote, &texture);
                    cache.deliver(&remote, &texture);
                }
                None => cache.give_up(&remote),
            }
            cache.start_queued();
        });
    }

    fn start_queued(self: &Rc<Self>) {
        loop {
            if self.in_flight.borrow().len() >= MAX_CONCURRENT_FETCHES {
                return;
            }
            let Some(next) = self.queued.borrow_mut().pop_front() else {
                return;
            };
            self.start(next);
        }
    }

    /// Writes beside the destination and renames, so a half-written file is never read back as a
    /// cache entry. A failure only costs a fetch next time.
    fn store(&self, remote: &str, bytes: &[u8]) {
        let file = self.file_for(remote);
        let partial = file.with_extension("partial");
        let written = std::fs::create_dir_all(&self.directory)
            .and_then(|()| std::fs::write(&partial, bytes))
            .and_then(|()| std::fs::rename(&partial, &file));
        if written.is_err() {
            let _ = std::fs::remove_file(&partial);
        }
    }

    fn deliver(&self, remote: &str, texture: &gdk::Texture) {
        let images = self.waiting.borrow_mut().remove(remote).unwrap_or_default();
        for image in images.iter().filter_map(glib::WeakRef::upgrade) {
            image.set_paintable(Some(texture));
        }
    }

    fn give_up(&self, remote: &str) {
        self.failed.borrow_mut().insert(remote.to_owned());
        self.waiting.borrow_mut().remove(remote);
    }
}
