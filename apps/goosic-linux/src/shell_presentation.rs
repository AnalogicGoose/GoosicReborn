//! Window layout, keyboard shortcuts and alternate playback presentations.

use super::*;

impl Shell {
    pub(super) fn wire_layout(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        self.window.connect_realize(move |window| {
            if let Some(surface) = window.surface() {
                let weak = weak.clone();
                surface.connect_layout(move |_, _, _| {
                    let weak = weak.clone();
                    glib::idle_add_local_once(move || {
                        let Some(shell) = weak.upgrade() else {
                            return;
                        };
                        let size = (shell.window.width(), shell.window.height());
                        if size.0 > 0 && size != shell.layout_size.get() {
                            let old = shell.layout_size.replace(size);
                            if size.0 < 820 && old.0 >= 820 {
                                shell.sidebar_open.set(false);
                            }
                            shell.apply_layout();
                        }
                    });
                });
            }
        });
        self.apply_layout();
    }

    pub(super) fn set_sleep_timer(self: &Rc<Self>, minutes: i32) {
        self.sleep_deadline.set(None);
        self.sleep_end_track.borrow_mut().take();
        if minutes == -1 {
            let id = self
                .player
                .borrow()
                .current
                .as_ref()
                .map(|track| track.video_id.clone());
            let Some(id) = id else {
                return self.set_status("Choose a song before setting its sleep timer.");
            };
            *self.sleep_end_track.borrow_mut() = Some(id);
            self.set_status("Goosic will pause at the end of this song.");
        } else if matches!(minutes, 15 | 30 | 45 | 60) {
            self.sleep_deadline.set(Some(
                Instant::now() + std::time::Duration::from_secs(minutes as u64 * 60),
            ));
            self.set_status(&format!("Goosic will pause in {minutes} minutes."));
        } else {
            self.set_status("Sleep timer off.");
        }
    }

    pub(super) fn wire_sleep_timer(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        glib::timeout_add_seconds_local(1, move || {
            let Some(shell) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            shell
                .queue_panel
                .set_undo_available(shell.player.borrow().can_undo_clear(Instant::now()));
            if shell
                .sleep_deadline
                .get()
                .is_some_and(|deadline| Instant::now() >= deadline)
                && shell.player.borrow().transition() == PlaybackTransition::Idle
            {
                shell.sleep_deadline.set(None);
                shell.pause_for_sleep();
            }
            glib::ControlFlow::Continue
        });
    }

    pub(super) fn pause_for_sleep(&self) {
        let (owner, generation) = {
            let mut player = self.player.borrow_mut();
            player.listener_paused = true;
            player.start_pending = false;
            (player.lease.owner, player.lease.generation)
        };
        if owner == Owner::OfficialWebView && self.official.is_loaded_for(generation) {
            self.official.pause();
        } else if owner == Owner::LocalDownloadedFile && self.local.is_loaded_for(generation) {
            self.local.pause();
        }
        self.set_status("Sleep timer finished. Pause requested.");
    }

    pub(super) fn apply_layout(&self) {
        let (width, height) = self.layout_size.get();
        let full = self.full_open.get();
        let panel = !full && (self.queue_visible.get() || self.lyrics.borrow().visible);
        let geometry = layout::window(width, self.sidebar_open.get(), panel);
        self.sidebar.set_halign(gtk::Align::Fill);
        self.sidebar.set_width_request(-1);
        self.sidebar
            .set_margin_end((width - geometry.sidebar - 8).max(8));
        self.sidebar.set_visible(self.sidebar_open.get() && !full);
        self.main.set_margin_start(geometry.left + 26);
        self.main.set_margin_end(geometry.right + 26);
        // Clearance belongs to the scroll content, so rows can pass behind floating chrome.
        self.main.set_margin_top(0);
        self.main.set_margin_bottom(0);
        self.side.set_width_request(geometry.panel);
        self.side
            .set_margin_bottom(if geometry.stacked_player { 148 } else { 108 });
        self.bar.set_stacked(geometry.stacked_player);
        self.bar.root.set_halign(gtk::Align::Fill);
        self.bar.root.set_margin_start(geometry.player_left);
        self.bar
            .root
            .set_margin_end((width - geometry.player_left - geometry.player_width).max(20));
        // Request the available width; the maximum-width rule already lives in layout::window.
        self.bar
            .root
            .set_width_request((geometry.player_width - 32).max(1));
        self.bar.root.set_visible(!full);
        self.sidebar_toggle.set_visible(!full);
        self.main.set_visible(!full);
        self.content.set_visible(true);
        self.full_player.root.set_visible(full);
        if full {
            self.window.add_css_class("goosic-immersive");
        } else {
            self.window.remove_css_class("goosic-immersive");
        }
        let available = (width - geometry.left - geometry.right - 52).max(1);
        let search_inset = ((available - 620) / 2).max(0);
        let navigation_clearance = if geometry.left == 0 { 32 } else { 0 };
        self.search_bar
            .set_margin_start(search_inset + navigation_clearance);
        self.search_bar.set_margin_end(search_inset);
        let search_height = self
            .search_bar
            .measure(gtk::Orientation::Vertical, available - navigation_clearance)
            .1;
        let top = if self.search_bar.is_visible() {
            search_height + 32
        } else {
            42
        };
        let bar_height = self.bar.root.measure(gtk::Orientation::Vertical, -1).1;
        ui::set_clearance(&self.rows, top, bar_height + 36);
        self.full_player.layout(
            width,
            height,
            self.lyrics.borrow().visible,
            full && self.queue_visible.get(),
        );
    }

    pub(super) fn toggle_sidebar(self: &Rc<Self>) {
        self.sidebar_open.set(!self.sidebar_open.get());
        self.apply_layout();
    }

    pub(super) fn toggle_full_player(self: &Rc<Self>) {
        if self.full_open.get() {
            return self.close_full_player();
        }
        self.full_open.set(true);
        self.full_player.root.grab_focus();
        self.sync_side_panels();
        self.refresh_player();
        self.load_lyrics();
    }

    pub(super) fn close_full_player(self: &Rc<Self>) {
        self.full_open.set(false);
        if self.window.is_fullscreen() {
            self.window.unfullscreen();
        }
        self.sync_side_panels();
        self.refresh_player();
    }

    pub(super) fn toggle_fullscreen(self: &Rc<Self>) {
        if !self.full_open.get() {
            self.toggle_full_player();
        }
        if self.window.is_fullscreen() {
            self.window.unfullscreen();
        } else {
            self.window.fullscreen();
        }
    }

    pub(super) fn escape(self: &Rc<Self>) {
        if self.bar.close_volume() || self.full_player.bar.close_volume() {
            return;
        }
        if self.full_open.get() {
            return self.close_full_player();
        }
        self.queue_visible.set(false);
        self.lyrics.borrow_mut().visible = false;
        self.save_preferences(PreferencesPatch {
            queue_visible: Some(false),
            ..Default::default()
        });
        self.sync_side_panels();
        if self.layout_size.get().0 < 820 {
            self.sidebar_open.set(false);
            self.apply_layout();
        }
    }

    pub(super) fn search_focus(self: &Rc<Self>) {
        self.close_full_player();
        self.navigate(Route::Search);
        fn focus_entry(widget: &gtk::Widget) -> bool {
            if widget.is::<gtk::SearchEntry>() {
                return widget.grab_focus();
            }
            let mut child = widget.first_child();
            while let Some(widget) = child {
                if focus_entry(&widget) {
                    return true;
                }
                child = widget.next_sibling();
            }
            false
        }
        focus_entry(self.search_bar.upcast_ref());
    }

    pub(super) fn wire_keyboard(self: &Rc<Self>) {
        type Shortcut = (&'static str, &'static str, fn(&Rc<Shell>));
        let bindings: &[Shortcut] = &[
            ("next", "<Control>Right", Shell::next),
            ("previous", "<Control>Left", Shell::previous),
            ("mute", "<Control>m", Shell::toggle_muted),
            ("shuffle", "<Control>s", Shell::toggle_shuffle),
            ("repeat", "<Control>r", Shell::cycle_repeat),
            ("search", "<Control>f", Shell::search_focus),
            ("lyrics", "<Control>l", Shell::toggle_lyrics),
            // Ctrl+Q remains Linux's explicit Quit action.
            ("queue", "<Control><Shift>q", Shell::toggle_queue),
            ("back", "<Alt>Left", Shell::back),
            (
                "full-player",
                "<Control><Shift>f",
                Shell::toggle_full_player,
            ),
            ("fullscreen", "F11", Shell::toggle_fullscreen),
            ("escape", "Escape", Shell::escape),
            ("sidebar", "<Control>b", Shell::toggle_sidebar),
        ];
        let Some(app) = self.window.application() else {
            return;
        };
        for &(name, key, method) in bindings {
            let action = gio::SimpleAction::new(name, None);
            let call = forward_unit(&Rc::downgrade(self), method);
            action.connect_activate(move |_, _| call());
            self.window.add_action(&action);
            app.set_accels_for_action(&format!("win.{name}"), &[key]);
        }
        for (name, key, seek, delta) in [
            ("seek-forward", "<Shift>Right", true, 10.0),
            ("seek-back", "<Shift>Left", true, -10.0),
            ("volume-up", "<Control>Up", false, 0.05),
            ("volume-down", "<Control>Down", false, -0.05),
        ] {
            let action = gio::SimpleAction::new(name, None);
            let weak = Rc::downgrade(self);
            action.connect_activate(move |_, _| {
                if let Some(shell) = weak.upgrade() {
                    let value = {
                        let player = shell.player.borrow();
                        if seek {
                            player.current_time + delta
                        } else {
                            player.volume + delta
                        }
                    };
                    if seek {
                        shell.seek(value);
                    } else {
                        shell.set_volume(value);
                    }
                }
            });
            self.window.add_action(&action);
            app.set_accels_for_action(&format!("win.{name}"), &[key]);
        }
        let settings = gio::SimpleAction::new("settings", None);
        let weak = Rc::downgrade(self);
        settings.connect_activate(move |_, _| {
            if let Some(shell) = weak.upgrade() {
                shell.close_full_player();
                shell.navigate(Route::Settings);
            }
        });
        self.window.add_action(&settings);
        app.set_accels_for_action("win.settings", &["<Control>comma"]);
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(self);
        keys.connect_key_pressed(move |_, key, _, modifiers| {
            if key != gtk::gdk::Key::space
                || modifiers.intersects(
                    gtk::gdk::ModifierType::CONTROL_MASK
                        | gtk::gdk::ModifierType::ALT_MASK
                        | gtk::gdk::ModifierType::SHIFT_MASK,
                )
            {
                return glib::Propagation::Proceed;
            }
            let Some(shell) = weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            let mut focus = gtk::prelude::GtkWindowExt::focus(&shell.window);
            while let Some(widget) = focus {
                if widget.is::<gtk::Editable>()
                    || widget.is::<gtk::TextView>()
                    || widget.is::<gtk::Button>()
                    || widget.is::<gtk::MenuButton>()
                    || widget.is::<gtk::Range>()
                {
                    return glib::Propagation::Proceed;
                }
                focus = widget.parent();
            }
            shell.toggle_pause();
            glib::Propagation::Stop
        });
        self.window.add_controller(keys);
    }
}
