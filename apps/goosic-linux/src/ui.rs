//! Drawing what `pages` decides, following the macOS design language and layout in
//! `docs/UI_DESIGN.md`.
//!
//! Every screen is one `GtkListView` over the rows `Browser::rows` returns. Only the rows on screen
//! have widgets, so a 500-track album costs what a dozen rows cost, and a row's widgets are built
//! when it is bound rather than kept for every row of every page.

use std::rc::Rc;

use goosic_protocol::ArtistHeader;
use goosic_shell_support::catalog::{Card, CardAction, Track};
use goosic_shell_support::navigation::{EntityReference, Route, SearchFilter, Theme};
use gtk::prelude::*;
use gtk::{gio, glib, pango};

use crate::artwork::ArtworkCache;
use crate::glass::{GlassFrame, GlassGroup, GlassKind};
use crate::pages::{route_title, LibrarySection, MoreRow, PageRow, ShellFacts};
use crate::theme;

/// What a row can ask the shell to do, and the artwork cache rows draw from.
pub struct Actions {
    pub navigate: Box<dyn Fn(Route)>,
    pub open: Box<dyn Fn(EntityReference)>,
    /// Plays a track; the list is what becomes the queue, and may be empty.
    pub play: Box<dyn Fn(Track, Rc<[Track]>)>,
    pub shuffle_play: Box<dyn Fn(Rc<[Track]>)>,
    pub start_radio_for: Box<dyn Fn(Track)>,
    pub retry: Box<dyn Fn()>,
    pub back: Box<dyn Fn()>,
    pub set_theme: Box<dyn Fn(Theme)>,
    pub set_hide_explicit: Box<dyn Fn(bool)>,
    pub set_start_page: Box<dyn Fn(String)>,
    pub set_reduce_motion: Box<dyn Fn(bool)>,
    pub set_artwork_background: Box<dyn Fn(bool)>,
    pub set_discord_status: Box<dyn Fn(bool)>,
    pub set_debug_mode: Box<dyn Fn(bool)>,
    pub toggle_autoplay: Box<dyn Fn()>,
    pub toggle_shuffle: Box<dyn Fn()>,
    pub import_legacy: Box<dyn Fn()>,
    /// Fetches the next part of a page that continues.
    pub load_more: Box<dyn Fn()>,
    pub sign_in: Box<dyn Fn()>,
    pub switch_account: Box<dyn Fn(String)>,
    pub sign_out: Box<dyn Fn()>,
    pub remove_account: Box<dyn Fn(String)>,
    pub select_library_section: Box<dyn Fn(LibrarySection)>,
    pub create_playlist: Box<dyn Fn()>,
    pub manage_playlist: Box<dyn Fn(String)>,
    pub save_playlist: Box<dyn Fn(String, bool)>,
    pub like_track: Box<dyn Fn(String)>,
    pub unlike_track: Box<dyn Fn(String)>,
    pub subscribe_artist: Box<dyn Fn(String, bool)>,
    pub add_to_playlist: Box<dyn Fn(String)>,
    pub can_edit_library: Box<dyn Fn() -> bool>,
    pub is_refused: Box<dyn Fn(&str) -> bool>,
    pub queue_track: Box<dyn Fn(Track, bool)>,
    pub submit_search: Box<dyn Fn(String)>,
    pub clear_recent_searches: Box<dyn Fn()>,
    pub artwork: Rc<ArtworkCache>,
}

/// A scrolled list that draws `PageRow`s, and the store that feeds it.
pub fn page_list(actions: Rc<Actions>) -> (gtk::ScrolledWindow, gio::ListStore) {
    let at_end = actions.clone();
    let store = gio::ListStore::new::<glib::BoxedAnyObject>();
    let factory = gtk::SignalListItemFactory::new();
    // Since GTK 4.12 a factory also builds section headers, so it is handed a plain object.
    factory.connect_setup(|_, item| {
        let item = list_item(item);
        item.set_activatable(false);
        item.set_selectable(false);
        item.set_child(Some(&gtk::Box::new(gtk::Orientation::Vertical, 0)));
    });
    factory.connect_bind(move |_, item| {
        let item = list_item(item);
        let container = item
            .child()
            .and_downcast::<gtk::Box>()
            .expect("every row is set up with a box");
        while let Some(child) = container.first_child() {
            container.remove(&child);
        }
        let object = item
            .item()
            .and_downcast::<glib::BoxedAnyObject>()
            .expect("the store holds rows");
        container.append(&row_widget(&object.borrow::<PageRow>(), &actions));
    });
    let list = gtk::ListView::new(
        Some(gtk::NoSelection::new(Some(store.clone()))),
        Some(factory),
    );
    let scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&list)
        .build();
    scroller.add_css_class("goosic-page");
    scroller.connect_edge_reached(move |_, edge| {
        if edge == gtk::PositionType::Bottom {
            (at_end.load_more)();
        }
    });
    (scroller, store)
}

pub fn set_clearance(store: &gio::ListStore, top: i32, bottom: i32) {
    if store.n_items() < 2 {
        return;
    }
    for (position, height) in [(0, top), (store.n_items() - 1, bottom)] {
        let Some(object) = store.item(position).and_downcast::<glib::BoxedAnyObject>() else {
            continue;
        };
        let changed = match *object.borrow::<PageRow>() {
            PageRow::Clearance(old) => old != height,
            _ => false,
        };
        if changed {
            store.splice(
                position,
                1,
                &[glib::BoxedAnyObject::new(PageRow::Clearance(height))],
            );
        }
    }
}

fn list_item(object: &glib::Object) -> &gtk::ListItem {
    object
        .downcast_ref::<gtk::ListItem>()
        .expect("the page list has no section headers")
}

/// Replaces what the list shows, in one change. Only the rows after the last unchanged one are
/// replaced, so a page that grew at its end keeps its scroll position and the rows on screen keep
/// their widgets.
pub fn set_rows(store: &gio::ListStore, mut rows: Vec<PageRow>) {
    fn clearance(store: &gio::ListStore, index: u32, fallback: i32) -> i32 {
        store
            .item(index)
            .and_downcast::<glib::BoxedAnyObject>()
            .map_or(fallback, |object| match *object.borrow::<PageRow>() {
                PageRow::Clearance(height) => height,
                _ => fallback,
            })
    }
    let top = clearance(store, 0, 28);
    let bottom = clearance(store, store.n_items().saturating_sub(1), 110);
    rows.insert(0, PageRow::Clearance(top));
    rows.push(PageRow::Clearance(bottom));
    let unchanged = (0..store.n_items())
        .zip(rows.iter())
        .take_while(|(position, row)| {
            store
                .item(*position)
                .and_downcast::<glib::BoxedAnyObject>()
                .is_some_and(|object| same_row(&object.borrow::<PageRow>(), row))
        })
        .count();
    let objects: Vec<glib::BoxedAnyObject> = rows
        .into_iter()
        .skip(unchanged)
        .map(glib::BoxedAnyObject::new)
        .collect();
    let unchanged = u32::try_from(unchanged).expect("a list store position fits in u32");
    store.splice(unchanged, store.n_items() - unchanged, &objects);
}

/// Row equality that stays cheap on a long album: a track row's list is compared by identity, then
/// by the ids it holds, rather than field by field for every row.
fn same_row(old: &PageRow, new: &PageRow) -> bool {
    match (old, new) {
        (
            PageRow::Track {
                track: old_track,
                context: old_context,
            },
            PageRow::Track {
                track: new_track,
                context: new_context,
            },
        ) => {
            old_track == new_track
                && (Rc::ptr_eq(old_context, new_context)
                    || (old_context.len() == new_context.len()
                        && old_context
                            .iter()
                            .zip(new_context.iter())
                            .all(|(old, new)| old.id == new.id)))
        }
        _ => old == new,
    }
}

/// What a sidebar navigation row opens, matching the macOS sidebar hierarchy:
/// Search + Home; Discover (Explore, Charts, Moods & genres, New releases);
/// Library (Liked Music, Artists, Albums); Playlists (All Playlists + user playlists).
#[derive(Clone, Copy, PartialEq, Eq)]
enum SidebarEntry {
    Section(&'static str),
    Route(Route),
    Library(LibrarySection, &'static str, &'static str),
}

const SIDEBAR_ENTRIES: [SidebarEntry; 13] = [
    SidebarEntry::Route(Route::Search),
    SidebarEntry::Route(Route::Home),
    SidebarEntry::Section("Discover"),
    SidebarEntry::Route(Route::Explore),
    SidebarEntry::Route(Route::Charts),
    SidebarEntry::Route(Route::MoodsAndGenres),
    SidebarEntry::Route(Route::NewReleases),
    SidebarEntry::Section("Library"),
    SidebarEntry::Library(LibrarySection::Songs, "Liked Music", "emote-love-symbolic"),
    SidebarEntry::Library(
        LibrarySection::Artists,
        "Artists",
        "audio-input-microphone-symbolic",
    ),
    SidebarEntry::Library(
        LibrarySection::Albums,
        "Albums",
        "media-optical-cd-audio-symbolic",
    ),
    SidebarEntry::Section("Playlists"),
    SidebarEntry::Library(
        LibrarySection::Playlists,
        "All Playlists",
        "view-grid-symbolic",
    ),
];

/// Callbacks for the sidebar's navigation items and anchored account menu.
pub struct SidebarActions {
    pub navigate: Box<dyn Fn(Route)>,
    pub select_library: Box<dyn Fn(LibrarySection)>,
    pub open_entity: Box<dyn Fn(EntityReference)>,
    pub sign_in: Box<dyn Fn()>,
    pub switch_account: Box<dyn Fn(String)>,
    pub sign_out: Box<dyn Fn()>,
    pub artwork: Rc<ArtworkCache>,
}

/// The docked translucent sidebar, matching the macOS screenshot: grouped routes with `#FF0552`
/// symbolic icons, user playlists with thumbnails, and an anchored bottom account menu.
pub struct Sidebar {
    pub root: gtk::Box,
    pub routes: gtk::ListBox,
    pub playlists: gtk::Box,
    pub account: gtk::Label,
    pub avatar: gtk::Label,
    pub connection: gtk::Label,
    pub status: gtk::Label,
    account_menu_box: gtk::Box,
    account_popover: gtk::Popover,
    actions: Rc<SidebarActions>,
}

impl Sidebar {
    pub fn update_account_menu(&self, facts: &ShellFacts) {
        while let Some(child) = self.account_menu_box.first_child() {
            self.account_menu_box.remove(&child);
        }
        if !facts.accounts.is_empty() {
            let heading = dim("Accounts");
            heading.set_margin_start(8);
            heading.set_margin_top(2);
            self.account_menu_box.append(&heading);
            for account in &facts.accounts {
                let active = facts.active_account_id.as_deref() == Some(account.id.as_str());
                let label = if active {
                    format!("●  {}", account.display_name)
                } else {
                    account.display_name.clone()
                };
                let button = gtk::Button::with_label(&label);
                button.set_sensitive(!active && !facts.account_busy && facts.connected);
                let (actions, close, id) = (
                    self.actions.clone(),
                    self.account_popover.clone(),
                    account.id.clone(),
                );
                button.connect_clicked(move |_| {
                    close.popdown();
                    (actions.switch_account)(id.clone());
                });
                self.account_menu_box.append(&button);
            }
            self.account_menu_box
                .append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        }
        let add = gtk::Button::with_label("Add Account…");
        add.set_sensitive(facts.connected && !facts.account_busy);
        {
            let (actions, close) = (self.actions.clone(), self.account_popover.clone());
            add.connect_clicked(move |_| {
                close.popdown();
                (actions.sign_in)();
            });
        }
        self.account_menu_box.append(&add);
        if facts.active_account_id.is_some() {
            let sign_out = gtk::Button::with_label("Sign Out");
            sign_out.set_sensitive(!facts.account_busy);
            {
                let (actions, close) = (self.actions.clone(), self.account_popover.clone());
                sign_out.connect_clicked(move |_| {
                    close.popdown();
                    (actions.sign_out)();
                });
            }
            self.account_menu_box.append(&sign_out);
        }
        self.account_menu_box
            .append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        let settings = gtk::Button::with_label("Settings…");
        {
            let (actions, close) = (self.actions.clone(), self.account_popover.clone());
            settings.connect_clicked(move |_| {
                close.popdown();
                (actions.navigate)(Route::Settings);
            });
        }
        self.account_menu_box.append(&settings);
    }

    pub fn set_playlists(&self, cards: &[Card]) {
        while let Some(child) = self.playlists.first_child() {
            self.playlists.remove(&child);
        }
        for card in cards {
            let Some(CardAction::Show(entity)) = &card.action else {
                continue;
            };
            let thumb = artwork_picture(
                &self.actions.artwork,
                card.thumbnail.as_deref(),
                22,
                22,
                false,
                "folder-music-symbolic",
            );
            thumb.add_css_class("goosic-sidebar-thumb");
            let label = single_line(plain(&card.title));
            label.set_hexpand(true);
            let content = row(8);
            content.set_valign(gtk::Align::Center);
            content.append(&thumb);
            content.append(&label);
            let button = gtk::Button::builder()
                .child(&content)
                .tooltip_text(&card.title)
                .build();
            button.add_css_class("flat");
            button.add_css_class("goosic-sidebar-playlist");
            let (actions, entity) = (self.actions.clone(), entity.clone());
            button.connect_clicked(move |_| (actions.open_entity)(entity.clone()));
            self.playlists.append(&button);
        }
    }
}

pub fn sidebar(actions: SidebarActions) -> Sidebar {
    let actions = Rc::new(actions);
    let routes = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::Single)
        .build();
    routes.add_css_class("navigation-sidebar");

    for entry in SIDEBAR_ENTRIES {
        match entry {
            SidebarEntry::Section(heading) => {
                let label = plain(heading);
                label.add_css_class("goosic-sidebar-section");
                label.set_margin_start(10);
                label.set_margin_top(14);
                label.set_margin_bottom(4);
                let header = gtk::ListBoxRow::new();
                header.set_selectable(false);
                header.set_activatable(false);
                header.set_child(Some(&label));
                routes.append(&header);
            }
            SidebarEntry::Route(route) => {
                routes.append(&sidebar_nav_item(route_icon(route), route_title(route)));
            }
            SidebarEntry::Library(_, title, icon_name) => {
                routes.append(&sidebar_nav_item(icon_name, title));
            }
        }
    }
    select_route(&routes, Route::Home);
    {
        let actions = actions.clone();
        routes.connect_row_activated(move |_, row| {
            let Some(entry) = usize::try_from(row.index())
                .ok()
                .and_then(|index| SIDEBAR_ENTRIES.get(index))
            else {
                return;
            };
            match *entry {
                SidebarEntry::Route(route) => (actions.navigate)(route),
                SidebarEntry::Library(section, _, _) => {
                    (actions.navigate)(Route::Library);
                    (actions.select_library)(section);
                }
                SidebarEntry::Section(_) => {}
            }
        });
    }

    let playlists = column(2);
    playlists.set_margin_start(4);
    playlists.set_margin_end(4);

    let nav_body = column(2);
    nav_body.append(&routes);
    nav_body.append(&playlists);

    let navigation = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&nav_body)
        .build();

    let connection = dim("Connecting…");
    connection.add_css_class("goosic-account-connection");
    let status = dim("");
    status.set_wrap_mode(pango::WrapMode::WordChar);
    status.set_lines(2);
    status.set_ellipsize(pango::EllipsizeMode::End);
    status.set_visible(false);
    let account = single_line(plain("Guest"));
    account.set_max_width_chars(16);
    account.add_css_class("goosic-account-name");
    let avatar = gtk::Label::builder()
        .label("G")
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .build();
    avatar.add_css_class("goosic-account-avatar");
    let account_text = column(1);
    account_text.set_valign(gtk::Align::Center);
    account_text.set_hexpand(true);
    account_text.append(&account);
    account_text.append(&connection);
    let chevron = gtk::Image::builder()
        .icon_name("pan-down-symbolic")
        .pixel_size(12)
        .valign(gtk::Align::Center)
        .build();
    chevron.add_css_class("dim-label");

    let account_contents = row(10);
    account_contents.set_valign(gtk::Align::Center);
    account_contents.append(&avatar);
    account_contents.append(&account_text);
    account_contents.append(&chevron);

    let account_menu_box = column(4);
    account_menu_box.set_margin_top(6);
    account_menu_box.set_margin_bottom(6);
    account_menu_box.set_margin_start(6);
    account_menu_box.set_margin_end(6);
    let account_popover = gtk::Popover::new();
    account_popover.add_css_class("goosic-popover");
    account_popover.set_child(Some(&account_menu_box));

    let account_button = gtk::MenuButton::builder()
        .child(&account_contents)
        .popover(&account_popover)
        .tooltip_text("Account and settings")
        .build();
    account_button.add_css_class("goosic-account-row");

    let footer_divider = gtk::Separator::new(gtk::Orientation::Horizontal);
    footer_divider.add_css_class("goosic-sidebar-divider");

    let root = column(0);
    root.set_width_request(212);
    root.append(&navigation);
    root.append(&status);
    root.append(&footer_divider);
    root.append(&account_button);

    let sidebar = Sidebar {
        root,
        routes,
        playlists,
        account,
        avatar,
        connection,
        status,
        account_menu_box,
        account_popover,
        actions,
    };
    sidebar.update_account_menu(&ShellFacts::default());
    sidebar
}

fn sidebar_nav_item(icon_name: &str, title: &str) -> gtk::Box {
    let icon = gtk::Image::builder()
        .icon_name(icon_name)
        .pixel_size(16)
        .valign(gtk::Align::Center)
        .build();
    icon.add_css_class("goosic-route-icon");
    let label = single_line(plain(title));
    label.set_valign(gtk::Align::Center);
    let item = row(10);
    item.set_margin_top(5);
    item.set_margin_bottom(5);
    item.set_margin_start(10);
    item.set_margin_end(8);
    item.append(&icon);
    item.append(&label);
    item
}

fn route_icon(route: Route) -> &'static str {
    match route {
        Route::Search => "system-search-symbolic",
        Route::Home => "user-home-symbolic",
        Route::Explore => "web-browser-symbolic",
        Route::Charts => "view-sort-ascending-symbolic",
        Route::MoodsAndGenres => "face-smile-symbolic",
        Route::NewReleases => "starred-symbolic",
        Route::Library => "view-grid-symbolic",
        Route::Settings => "preferences-system-symbolic",
    }
}

/// Marks `route` as selected in the sidebar.
pub fn select_route(routes: &gtk::ListBox, route: Route) {
    let index = SIDEBAR_ENTRIES.iter().position(|entry| match *entry {
        SidebarEntry::Route(candidate) => candidate == route,
        SidebarEntry::Library(LibrarySection::Playlists, _, _) => route == Route::Library,
        _ => false,
    });
    let row = index.and_then(|index| routes.row_at_index(i32::try_from(index).ok()?));
    routes.select_row(row.as_ref());
}

/// Marks a specific Library section (`Liked Music`, `Artists`, `Albums`, `All Playlists`) as selected.
pub fn select_library_section(routes: &gtk::ListBox, section: LibrarySection) {
    let index = SIDEBAR_ENTRIES
        .iter()
        .position(|entry| matches!(*entry, SidebarEntry::Library(s, _, _) if s == section));
    let row = index.and_then(|index| routes.row_at_index(i32::try_from(index).ok()?));
    routes.select_row(row.as_ref());
}

/// The floating macOS-style Search capsule and linked native segmented category control.
/// Both sit inside frosted `GlassFrame`s so scrolled rows blur smoothly beneath them.
pub fn search_bar(
    on_submit: impl Fn(String) + 'static,
    on_filter: impl Fn(SearchFilter) + 'static,
    glass: &GlassGroup,
) -> gtk::Box {
    let on_submit = Rc::new(on_submit);
    let entry = gtk::SearchEntry::builder()
        .placeholder_text("Search music")
        .hexpand(true)
        .build();
    entry.add_css_class("goosic-search-entry");
    let search = gtk::Button::with_label("Search");
    search.add_css_class("goosic-search-submit");
    {
        let on_submit = on_submit.clone();
        entry.connect_activate(move |entry| on_submit(entry.text().to_string()));
    }
    {
        let entry = entry.clone();
        search.connect_clicked(move |_| on_submit(entry.text().to_string()));
    }
    let pill = row(8);
    pill.add_css_class("goosic-search-pill");
    pill.append(&entry);
    pill.append(&search);
    let pill_glass = glass.wrap(&pill, GlassKind::SearchPill, None);

    let on_filter = Rc::new(on_filter);
    let segments = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    segments.add_css_class("goosic-search-segments");
    segments.set_halign(gtk::Align::Center);
    let mut first: Option<gtk::ToggleButton> = None;
    for filter in SearchFilter::ALL {
        let toggle = gtk::ToggleButton::with_label(filter_label(filter));
        toggle.add_css_class("goosic-segment-button");
        match &first {
            Some(first) => toggle.set_group(Some(first)),
            None => {
                toggle.set_active(true);
                first = Some(toggle.clone());
            }
        }
        let on_filter = on_filter.clone();
        toggle.connect_toggled(move |toggle| {
            if toggle.is_active() {
                on_filter(filter);
            }
        });
        segments.append(&toggle);
    }
    let segments_glass = glass.wrap(&segments, GlassKind::SegmentedPill, None);
    segments_glass.set_halign(gtk::Align::Center);
    let filter_scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Never)
        .propagate_natural_height(true)
        .child(&segments_glass)
        .build();

    let root = column(12);
    root.set_margin_top(16);
    root.set_margin_start(24);
    root.set_margin_end(24);
    root.append(&pill_glass);
    root.append(&filter_scroll);
    root
}

/// Floating frosted "Find in Playlist" bar triggered by Ctrl+Shift+F on track lists.
pub fn find_bar(
    on_change: impl Fn(String) + 'static,
    on_close: impl Fn() + 'static,
    glass: &GlassGroup,
) -> (GlassFrame, gtk::SearchEntry) {
    let entry = gtk::SearchEntry::builder()
        .placeholder_text("Find in playlist or album")
        .hexpand(true)
        .width_request(260)
        .build();
    entry.add_css_class("goosic-search-entry");
    entry.connect_search_changed(move |e| on_change(e.text().to_string()));
    let close = gtk::Button::builder()
        .icon_name("window-close-symbolic")
        .tooltip_text("Close find (Esc)")
        .build();
    close.add_css_class("flat");
    {
        let entry = entry.clone();
        close.connect_clicked(move |_| {
            entry.set_text("");
            on_close();
        });
    }
    let box_inner = row(8);
    box_inner.add_css_class("goosic-find-bar");
    box_inner.append(&entry);
    box_inner.append(&close);
    let frame = glass.wrap(&box_inner, GlassKind::SearchPill, None);
    frame.set_halign(gtk::Align::End);
    frame.set_valign(gtk::Align::Start);
    frame.set_margin_top(14);
    frame.set_margin_end(24);
    frame.set_visible(false);
    (frame, entry)
}

fn filter_label(filter: SearchFilter) -> &'static str {
    match filter {
        SearchFilter::All => "All",
        SearchFilter::Songs => "Songs",
        SearchFilter::Albums => "Albums",
        SearchFilter::Artists => "Artists",
        SearchFilter::Playlists => "Playlists",
        SearchFilter::Videos => "Videos",
    }
}

fn row_widget(page_row: &PageRow, actions: &Rc<Actions>) -> gtk::Widget {
    match page_row {
        PageRow::Clearance(height) => gtk::Box::builder().height_request(*height).build().upcast(),
        PageRow::Back => {
            let icon = gtk::Image::from_icon_name("go-previous-symbolic");
            let label = plain("Back");
            let content = row(4);
            content.append(&icon);
            content.append(&label);
            let back = gtk::Button::builder()
                .child(&content)
                .halign(gtk::Align::Start)
                .build();
            back.add_css_class("goosic-back-button");
            let actions = actions.clone();
            back.connect_clicked(move |_| (actions.back)());
            padded(&back, 10, 4)
        }
        PageRow::Header { title, subtitle } => {
            let header = column(4);
            let heading = plain(title);
            heading.add_css_class("goosic-page-title");
            header.append(&heading);
            if !subtitle.is_empty() {
                header.append(&dim(subtitle));
            }
            padded(&header, 12, 10)
        }
        PageRow::CollectionHeader {
            title,
            subtitle,
            kind,
            thumbnail,
            tracks,
            playlist_id,
        } => padded(
            &collection_header(
                title,
                subtitle,
                kind,
                thumbnail.as_deref(),
                tracks,
                playlist_id.as_deref(),
                actions,
            ),
            8,
            18,
        ),
        PageRow::ArtistHero {
            artist_id,
            name,
            header,
            tracks,
        } => padded(&artist_hero(artist_id, name, header, tracks, actions), 4, 18),
        PageRow::RecentSearches(queries) => padded(&recent_searches(queries, actions), 8, 12),
        PageRow::SearchBrowse => padded(&search_browse(actions), 8, 20),
        PageRow::GuestNotice => {
            let notice = row(8);
            notice.add_css_class("goosic-guest-notice");
            let pill = plain("Guest");
            pill.add_css_class("goosic-guest-badge");
            notice.append(&pill);
            notice.append(&dim(
                "Browsing live YouTube Music without an account",
            ));
            padded(&notice, 0, 8)
        }
        PageRow::LibrarySections(active) => {
            let tabs = row(4);
            tabs.add_css_class("goosic-search-segments");
            tabs.set_halign(gtk::Align::Start);
            let active = *active;
            for section in LibrarySection::ALL {
                let button = gtk::ToggleButton::with_label(section.title());
                button.add_css_class("goosic-segment-button");
                button.set_active(section == active);
                let actions = actions.clone();
                button.connect_toggled(move |btn| {
                    if btn.is_active() && section != active {
                        (actions.select_library_section)(section);
                    }
                });
                tabs.append(&button);
            }
            padded(&tabs, 2, 12)
        }
        PageRow::LibraryActions => {
            let icon = gtk::Image::from_icon_name("list-add-symbolic");
            let content = row(6);
            content.append(&icon);
            content.append(&plain("New Playlist"));
            let button = gtk::Button::builder()
                .child(&content)
                .halign(gtk::Align::Start)
                .build();
            button.add_css_class("goosic-glass-pill-action");
            let actions = actions.clone();
            button.connect_clicked(move |_| (actions.create_playlist)());
            padded(&button, 4, 12)
        }
        PageRow::PlaylistActions(id) => {
            let controls = row(8);
            let save = gtk::Button::with_label("Save to Library");
            let remove = gtk::Button::with_label("Remove from Library");
            let manage = gtk::Button::with_label("Manage Playlist");
            for btn in [&save, &remove, &manage] {
                btn.add_css_class("goosic-glass-pill-action");
            }
            let (id, actions) = (id.clone(), actions.clone());
            let saved_id = id.clone();
            let saved_actions = actions.clone();
            save.connect_clicked(move |_| (saved_actions.save_playlist)(saved_id.clone(), true));
            let removed_id = id.clone();
            let removed_actions = actions.clone();
            remove.connect_clicked(move |_| {
                (removed_actions.save_playlist)(removed_id.clone(), false)
            });
            manage.connect_clicked(move |_| (actions.manage_playlist)(id.clone()));
            controls.append(&save);
            controls.append(&remove);
            controls.append(&manage);
            padded(&controls, 4, 10)
        }
        PageRow::Loading { subject } => {
            let loading = row(8);
            loading.append(&gtk::Spinner::builder().spinning(true).build());
            loading.append(&dim(&format!("Loading {subject}…")));
            padded(&loading, 18, 18)
        }
        PageRow::Failure { title, detail } => {
            let failure = column(6);
            failure.append(&bold(title));
            failure.append(&dim(detail));
            failure.append(&retry_button("Try again", actions));
            padded(&failure, 18, 18)
        }
        PageRow::Empty { title, message } => {
            let empty = column(4);
            empty.append(&bold(title));
            empty.append(&dim(message));
            padded(&empty, 18, 18)
        }
        PageRow::NotLoaded { subject } => {
            let idle = column(6);
            idle.append(&dim("Not loaded yet."));
            idle.append(&retry_button(&format!("Load {subject}"), actions));
            padded(&idle, 18, 18)
        }
        PageRow::PlayAll(tracks) => {
            let play_all = gtk::Button::builder()
                .label("Play")
                .halign(gtk::Align::Start)
                .build();
            play_all.add_css_class("goosic-collection-play");
            let (actions, tracks) = (actions.clone(), tracks.clone());
            play_all.connect_clicked(move |_| {
                if let Some(first) = tracks.first() {
                    (actions.play)(first.clone(), tracks.clone());
                }
            });
            padded(&play_all, 4, 8)
        }
        PageRow::Track { track, context } => padded(&track_row(track, context, actions), 2, 2),
        PageRow::ShelfTitle(title) => {
            let heading = plain(title);
            heading.add_css_class("goosic-shelf-title");
            padded(&heading, 18, 8)
        }
        PageRow::TrackShelf(tracks) => padded(&track_shelf(tracks, actions), 0, 12),
        PageRow::CardShelf { title, cards } => padded(&card_shelf(title, cards, actions), 14, 14),
        PageRow::Cards(cards) => padded(&card_strip(cards, actions).0, 0, 14),
        PageRow::Truncated => padded(
            &dim("This page was long, so only the first part is shown."),
            4,
            18,
        ),
        PageRow::Settings(facts) => padded(&settings_page(facts, actions), 8, 24),
        PageRow::More(more) => {
            let line = row(8);
            let action_button = |label: &str| {
                let button = gtk::Button::with_label(label);
                button.add_css_class("goosic-glass-pill-action");
                let actions = actions.clone();
                button.connect_clicked(move |_| (actions.load_more)());
                button
            };
            match more {
                MoreRow::Available => line.append(&action_button("Load more")),
                MoreRow::Loading => {
                    line.append(&gtk::Spinner::builder().spinning(true).build());
                    line.append(&dim("Loading more…"));
                }
                MoreRow::Failed(message) => {
                    line.append(&dim(&format!("Could not load more: {message}")));
                    line.append(&action_button("Try again"));
                }
            }
            padded(&line, 8, 24)
        }
    }
}

/// macOS `NativeMacCollectionHeader`: rounded artwork beside title, pink byline, metadata,
/// circular Shuffle button, and frosted glass Play capsule.
fn collection_header(
    title: &str,
    subtitle: &str,
    kind: &str,
    thumbnail: Option<&str>,
    tracks: &Rc<[Track]>,
    playlist_id: Option<&str>,
    actions: &Rc<Actions>,
) -> gtk::Box {
    let cover = artwork_picture(
        &actions.artwork,
        thumbnail,
        216,
        216,
        false,
        "audio-x-generic-symbolic",
    );
    cover.add_css_class("goosic-collection-cover");

    let meta = column(6);
    meta.set_valign(gtk::Align::End);
    meta.set_hexpand(true);
    let heading = plain(if title.is_empty() { kind } else { title });
    heading.add_css_class("goosic-collection-title");
    meta.append(&heading);
    if !subtitle.is_empty() {
        let sub = dim(subtitle);
        sub.add_css_class("goosic-collection-meta");
        meta.append(&sub);
    }
    if !tracks.is_empty() {
        let count = dim(&match tracks.len() {
            1 => "1 song".to_owned(),
            n => format!("{n} songs"),
        });
        count.add_css_class("goosic-collection-meta");
        meta.append(&count);
    }

    let buttons = row(10);
    buttons.set_margin_top(12);
    let shuffle = gtk::Button::builder()
        .icon_name("media-playlist-shuffle-symbolic")
        .tooltip_text("Shuffle")
        .sensitive(!tracks.is_empty())
        .build();
    shuffle.add_css_class("goosic-collection-circle");
    {
        let (actions, tracks) = (actions.clone(), tracks.clone());
        shuffle.connect_clicked(move |_| (actions.shuffle_play)(tracks.clone()));
    }

    let play_content = row(8);
    play_content.set_halign(gtk::Align::Center);
    play_content.append(&gtk::Image::from_icon_name("media-playback-start-symbolic"));
    play_content.append(&plain("Play"));
    let play = gtk::Button::builder()
        .child(&play_content)
        .sensitive(!tracks.is_empty())
        .build();
    play.add_css_class("goosic-collection-play");
    {
        let (actions, tracks) = (actions.clone(), tracks.clone());
        play.connect_clicked(move |_| {
            if let Some(first) = tracks.first() {
                (actions.play)(first.clone(), tracks.clone());
            }
        });
    }
    buttons.append(&shuffle);
    buttons.append(&play);

    if let Some(id) = playlist_id {
        let save = gtk::Button::with_label("Save");
        save.add_css_class("goosic-glass-pill-action");
        let (actions_s, id_s) = (actions.clone(), id.to_owned());
        save.connect_clicked(move |_| (actions_s.save_playlist)(id_s.clone(), true));
        let manage = gtk::Button::with_label("Manage");
        manage.add_css_class("goosic-glass-pill-action");
        let (actions_m, id_m) = (actions.clone(), id.to_owned());
        manage.connect_clicked(move |_| (actions_m.manage_playlist)(id_m.clone()));
        buttons.append(&save);
        buttons.append(&manage);
    }
    meta.append(&buttons);

    let header = row(28);
    header.append(&cover);
    header.append(&meta);
    header
}

/// macOS `NativeMacArtistHero`: edge-to-edge top-anchored banner (or circular portrait),
/// tint-sampled gradient foot scrim, bold centered name, audience line, and frosted About / Play / Subscribe.
fn artist_hero(
    artist_id: &str,
    name: &str,
    header: &ArtistHeader,
    tracks: &Rc<[Track]>,
    actions: &Rc<Actions>,
) -> gtk::Overlay {
    let hero_height = goosic_shell_support::hero::MIN_HEIGHT as i32;
    let backdrop = gtk::Box::new(gtk::Orientation::Vertical, 0);
    backdrop.set_height_request(hero_height);
    backdrop.set_hexpand(true);
    backdrop.set_overflow(gtk::Overflow::Hidden);
    backdrop.add_css_class("goosic-artist-hero");

    let scrim = gtk::Box::new(gtk::Orientation::Vertical, 0);
    scrim.add_css_class("goosic-artist-scrim");
    scrim.set_can_target(false);
    scrim.set_hexpand(true);
    scrim.set_vexpand(true);

    // Apply sampled artist page tint from the loaded banner/portrait texture via `hero::tint`.
    let apply_tint = |picture: &gtk::Picture, scrim: &gtk::Box| {
        let scrim_weak = scrim.downgrade();
        let update = move |picture: &gtk::Picture| {
            let Some(scrim) = scrim_weak.upgrade() else {
                return;
            };
            let Some(texture) = picture.paintable().and_downcast::<gtk::gdk::Texture>() else {
                return;
            };
            if let Some((r, g, b)) = crate::mesh::sample_hero_tint(&texture) {
                let (ri, gi, bi) = (
                    (r * 255.0).round() as u8,
                    (g * 255.0).round() as u8,
                    (b * 255.0).round() as u8,
                );
                let css = format!(
                    ".goosic-artist-scrim {{ background: linear-gradient(to bottom, rgba({ri},{gi},{bi},0.06) 0%, rgba({ri},{gi},{bi},0.52) 55%, rgba({ri},{gi},{bi},0.94) 100%); }}"
                );
                let provider = gtk::CssProvider::new();
                provider.load_from_string(&css);
                #[allow(deprecated)]
                scrim
                    .style_context()
                    .add_provider(&provider, gtk::STYLE_PROVIDER_PRIORITY_USER);
            }
        };
        update(picture);
        picture.connect_paintable_notify(update);
    };

    if let Some(banner) = header.banner.as_deref() {
        let picture = gtk::Picture::builder()
            .content_fit(gtk::ContentFit::Cover)
            .can_shrink(true)
            .hexpand(true)
            .vexpand(true)
            .build();
        actions.artwork.show_picture(Some(banner), &picture);
        apply_tint(&picture, &scrim);
        backdrop.append(&picture);
    } else if let Some(portrait) = header.portrait.as_deref() {
        let circle = artwork_picture(
            &actions.artwork,
            Some(portrait),
            150,
            150,
            true,
            "audio-input-microphone-symbolic",
        );
        if let Some(picture) = circle
            .child()
            .and_downcast::<gtk::Overlay>()
            .and_then(|o| o.last_child())
            .and_downcast::<gtk::Picture>()
        {
            apply_tint(&picture, &scrim);
        }
        circle.set_halign(gtk::Align::Center);
        circle.set_margin_top(28);
        backdrop.append(&circle);
    }

    let title = gtk::Label::builder()
        .label(name)
        .justify(gtk::Justification::Center)
        .halign(gtk::Align::Center)
        .wrap(true)
        .build();
    title.add_css_class("goosic-artist-hero-name");

    let content = column(8);
    content.set_halign(gtk::Align::Center);
    content.set_valign(gtk::Align::End);
    content.set_margin_bottom(24);
    content.set_margin_start(24);
    content.set_margin_end(24);
    content.append(&title);

    if let Some(audience) = header.audience.as_deref() {
        let aud = gtk::Label::builder()
            .label(audience)
            .halign(gtk::Align::Center)
            .build();
        aud.add_css_class("goosic-artist-hero-audience");
        content.append(&aud);
    }

    let buttons = row(22);
    buttons.set_halign(gtk::Align::Center);
    buttons.set_margin_top(10);

    // Frosted About popover button blurring the artist banner behind it.
    let about_popover = gtk::Popover::new();
    about_popover.add_css_class("goosic-popover");
    let about_box = column(8);
    about_box.set_margin_top(12);
    about_box.set_margin_bottom(12);
    about_box.set_margin_start(14);
    about_box.set_margin_end(14);
    about_box.set_width_request(320);
    about_box.append(&bold(name));
    if let Some(subs) = header.subscribers.as_deref() {
        about_box.append(&dim(&format!("{subs} subscribers")));
    }
    if let Some(desc) = header.description.as_deref() {
        let d = plain(desc);
        d.set_max_width_chars(46);
        about_box.append(&d);
    }
    about_popover.set_child(Some(&about_box));
    let about_btn = gtk::MenuButton::builder()
        .icon_name("dialog-information-symbolic")
        .tooltip_text("About")
        .valign(gtk::Align::Center)
        .popover(&about_popover)
        .sensitive(header.description.is_some() || header.subscribers.is_some())
        .build();
    about_btn.add_css_class("goosic-artist-round-menu");
    let about_glass = GlassFrame::new(GlassKind::Circle);
    about_glass.set_child(&about_btn);
    about_glass.set_underlay(&backdrop);
    about_glass.set_valign(gtk::Align::Center);

    // Large white Play button centered under artist name.
    let play_btn = gtk::Button::builder()
        .child(
            &gtk::Image::builder()
                .icon_name("media-playback-start-symbolic")
                .pixel_size(26)
                .build(),
        )
        .tooltip_text("Play")
        .valign(gtk::Align::Center)
        .sensitive(!tracks.is_empty())
        .build();
    play_btn.add_css_class("goosic-artist-play-button");
    {
        let (actions, tracks) = (actions.clone(), tracks.clone());
        play_btn.connect_clicked(move |_| {
            if let Some(first) = tracks.first() {
                (actions.play)(first.clone(), tracks.clone());
            }
        });
    }

    // Frosted Subscribe star toggle on the right (balances About so Play stays centered).
    let sub_btn = gtk::ToggleButton::builder()
        .icon_name("non-starred-symbolic")
        .tooltip_text(if (actions.can_edit_library)() {
            "Subscribe to artist"
        } else {
            "Sign in to subscribe"
        })
        .valign(gtk::Align::Center)
        .sensitive((actions.can_edit_library)())
        .build();
    sub_btn.add_css_class("goosic-artist-round-btn");
    {
        let (actions, artist_id) = (actions.clone(), artist_id.to_owned());
        sub_btn.connect_toggled(move |btn| {
            let subscribed = btn.is_active();
            btn.set_icon_name(if subscribed {
                "starred-symbolic"
            } else {
                "non-starred-symbolic"
            });
            btn.set_tooltip_text(Some(if subscribed {
                "Unsubscribe from artist"
            } else {
                "Subscribe to artist"
            }));
            (actions.subscribe_artist)(artist_id.clone(), subscribed);
        });
    }
    let sub_glass = GlassFrame::new(GlassKind::Circle);
    sub_glass.set_child(&sub_btn);
    sub_glass.set_underlay(&backdrop);
    sub_glass.set_valign(gtk::Align::Center);

    buttons.append(&about_glass);
    buttons.append(&play_btn);
    buttons.append(&sub_glass);
    content.append(&buttons);

    let overlay = gtk::Overlay::new();
    overlay.set_overflow(gtk::Overflow::Hidden);
    overlay.add_css_class("goosic-artist-hero-frame");
    overlay.set_child(Some(&backdrop));
    overlay.add_overlay(&scrim);
    overlay.add_overlay(&content);
    overlay
}

/// Recently searched horizontal pill strip with a Clear button, shown above Browse categories.
fn recent_searches(queries: &[String], actions: &Rc<Actions>) -> gtk::Box {
    let section = column(10);
    let header = row(8);
    let heading = plain("Recently searched");
    heading.add_css_class("goosic-shelf-title");
    heading.set_hexpand(true);
    let clear = gtk::Button::with_label("Clear");
    clear.add_css_class("goosic-glass-pill-action");
    {
        let actions = actions.clone();
        clear.connect_clicked(move |_| (actions.clear_recent_searches)());
    }
    header.append(&heading);
    header.append(&clear);
    section.append(&header);

    let strip = row(10);
    for query in queries {
        let icon = gtk::Image::from_icon_name("document-open-recent-symbolic");
        icon.add_css_class("dim-label");
        let label = single_line(plain(query));
        let inner = row(8);
        inner.append(&icon);
        inner.append(&label);
        let btn = gtk::Button::builder().child(&inner).build();
        btn.add_css_class("goosic-recent-search-chip");
        let (actions, q) = (actions.clone(), query.clone());
        btn.connect_clicked(move |_| (actions.submit_search)(q.clone()));
        strip.append(&btn);
    }
    let scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Never)
        .propagate_natural_height(true)
        .child(&strip)
        .build();
    section.append(&scroller);
    section
}

/// macOS Search landing page: colorful gradient Browse category cards.
fn search_browse(actions: &Rc<Actions>) -> gtk::Box {
    let section = column(14);
    let heading = plain("Browse categories");
    heading.add_css_class("goosic-shelf-title");
    section.append(&heading);

    let grid = gtk::FlowBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .homogeneous(true)
        .min_children_per_line(1)
        .max_children_per_line(4)
        .column_spacing(16)
        .row_spacing(16)
        .build();

    for (route, icon_name, css_class) in [
        (
            Route::Explore,
            "web-browser-symbolic",
            "goosic-browse-blue",
        ),
        (
            Route::Charts,
            "view-sort-ascending-symbolic",
            "goosic-browse-orange",
        ),
        (
            Route::MoodsAndGenres,
            "face-smile-symbolic",
            "goosic-browse-purple",
        ),
        (
            Route::NewReleases,
            "starred-symbolic",
            "goosic-browse-pink",
        ),
    ] {
        let glyph = gtk::Image::builder()
            .icon_name(icon_name)
            .pixel_size(52)
            .halign(gtk::Align::End)
            .valign(gtk::Align::Start)
            .margin_top(16)
            .margin_end(16)
            .build();
        glyph.add_css_class("goosic-browse-glyph");

        let label = gtk::Label::builder()
            .label(route_title(route))
            .halign(gtk::Align::Start)
            .valign(gtk::Align::End)
            .margin_start(16)
            .margin_bottom(16)
            .build();
        label.add_css_class("goosic-browse-title");

        let card_bg = gtk::Overlay::new();
        card_bg.set_size_request(180, 136);
        let fill = gtk::Box::new(gtk::Orientation::Vertical, 0);
        fill.set_hexpand(true);
        fill.set_vexpand(true);
        card_bg.set_child(Some(&fill));
        card_bg.add_overlay(&glyph);
        card_bg.add_overlay(&label);

        let button = gtk::Button::builder().child(&card_bg).build();
        button.set_overflow(gtk::Overflow::Hidden);
        button.add_css_class("goosic-browse-card");
        button.add_css_class(css_class);
        let actions = actions.clone();
        button.connect_clicked(move |_| (actions.navigate)(route));
        grid.insert(&button, -1);
    }
    section.append(&grid);
    section
}

fn track_row(track: &Track, context: &Rc<[Track]>, actions: &Rc<Actions>) -> gtk::Box {
    let line = row(12);
    line.append(&artwork_picture(
        &actions.artwork,
        track.thumbnail.as_deref(),
        42,
        42,
        false,
        "audio-x-generic-symbolic",
    ));

    let text = column(2);
    text.set_hexpand(true);
    text.set_valign(gtk::Align::Center);
    let title_line = row(6);
    let title = single_line(plain(&track.title));
    title.add_css_class("goosic-track-title");
    title_line.append(&title);
    if track.explicit {
        let badge = plain("E");
        badge.add_css_class("goosic-explicit-badge");
        title_line.append(&badge);
    }
    text.append(&title_line);
    let subtitle = single_line(dim(&track.secondary_text()));
    subtitle.add_css_class("goosic-track-subtitle");
    text.append(&subtitle);
    line.append(&text);

    let duration = dim(&track.duration);
    duration.set_valign(gtk::Align::Center);
    line.append(&duration);
    let refused = (actions.is_refused)(&track.video_id);
    let button = gtk::Button::builder()
        .child(&line)
        .tooltip_text(if refused {
            "This song isn’t available to play here."
        } else {
            "Play"
        })
        .build();
    button.add_css_class("flat");
    button.add_css_class("goosic-track-row");
    if refused {
        button.add_css_class("goosic-track-refused");
    }
    let (selected, context_for_play, play_actions) = (track.clone(), context.clone(), actions.clone());
    button.connect_clicked(move |_| (play_actions.play)(selected.clone(), context_for_play.clone()));
    let outer = row(4);
    button.set_hexpand(true);
    outer.append(&button);
    outer.append(&track_menu(track, context, actions));
    outer
}

fn track_menu(track: &Track, context: &Rc<[Track]>, actions: &Rc<Actions>) -> gtk::MenuButton {
    let popover = gtk::Popover::new();
    popover.add_css_class("goosic-popover");
    let menu = column(4);
    menu.set_margin_start(6);
    menu.set_margin_end(6);
    menu.set_margin_top(6);
    menu.set_margin_bottom(6);

    let play_btn = gtk::Button::with_label("Play");
    {
        let (track, context, action, close) = (
            track.clone(),
            context.clone(),
            actions.clone(),
            popover.clone(),
        );
        play_btn.connect_clicked(move |_| {
            close.popdown();
            (action.play)(track.clone(), context.clone());
        });
    }
    menu.append(&play_btn);

    for (label, next) in [("Play Next", true), ("Add to Queue", false)] {
        let button = gtk::Button::with_label(label);
        let (track, action, close) = (track.clone(), actions.clone(), popover.clone());
        button.connect_clicked(move |_| {
            close.popdown();
            (action.queue_track)(track.clone(), next);
        });
        menu.append(&button);
    }

    let radio_btn = gtk::Button::with_label("Start Radio");
    {
        let (track, action, close) = (track.clone(), actions.clone(), popover.clone());
        radio_btn.connect_clicked(move |_| {
            close.popdown();
            (action.start_radio_for)(track.clone());
        });
    }
    menu.append(&radio_btn);

    if (actions.can_edit_library)() {
        menu.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        let like = gtk::Button::with_label("Like");
        let id = track.video_id.clone();
        let action = actions.clone();
        let close = popover.clone();
        like.connect_clicked(move |_| {
            close.popdown();
            (action.like_track)(id.clone());
        });
        let unlike = gtk::Button::with_label("Remove from Liked Music");
        let id = track.video_id.clone();
        let action = actions.clone();
        let close = popover.clone();
        unlike.connect_clicked(move |_| {
            close.popdown();
            (action.unlike_track)(id.clone());
        });
        menu.append(&like);
        menu.append(&unlike);
    }
    if track.artist_id.is_some() || track.album_id.is_some() {
        menu.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        if let Some(artist_id) = &track.artist_id {
            let button = gtk::Button::with_label("Go to Artist");
            let (id, action, close) = (artist_id.clone(), actions.clone(), popover.clone());
            button.connect_clicked(move |_| {
                close.popdown();
                (action.open)(EntityReference::Artist(id.clone()));
            });
            menu.append(&button);
        }
        if let Some(album_id) = &track.album_id {
            let button = gtk::Button::with_label("Go to Album");
            let (id, action, close) = (album_id.clone(), actions.clone(), popover.clone());
            button.connect_clicked(move |_| {
                close.popdown();
                (action.open)(EntityReference::Album(id.clone()));
            });
            menu.append(&button);
        }
    }
    if (actions.can_edit_library)() {
        menu.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        let add = gtk::Button::with_label("Add to Playlist…");
        let id = track.video_id.clone();
        let action = actions.clone();
        let close = popover.clone();
        add.connect_clicked(move |_| {
            close.popdown();
            (action.add_to_playlist)(id.clone());
        });
        menu.append(&add);
    }
    if !track.video_id.is_empty() {
        menu.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        let copy = gtk::Button::with_label("Copy Link");
        let url = format!("https://music.youtube.com/watch?v={}", track.video_id);
        let close = popover.clone();
        copy.connect_clicked(move |btn| {
            close.popdown();
            btn.clipboard().set_text(&url);
        });
        menu.append(&copy);
    }
    popover.set_child(Some(&menu));
    let button = gtk::MenuButton::builder()
        .icon_name("view-more-horizontal-symbolic")
        .tooltip_text("Song actions")
        .valign(gtk::Align::Center)
        .popover(&popover)
        .build();
    button.add_css_class("goosic-song-menu");
    button
}

fn track_shelf(tracks: &Rc<[Track]>, actions: &Rc<Actions>) -> gtk::ScrolledWindow {
    let columns = row(18);
    columns.set_margin_bottom(10);
    for group in tracks.chunks(4) {
        let track_column = column(2);
        track_column.set_width_request(310);
        for track in group {
            let line = row(10);
            line.append(&artwork_picture(
                &actions.artwork,
                track.thumbnail.as_deref(),
                48,
                48,
                false,
                "audio-x-generic-symbolic",
            ));
            let labels = column(2);
            labels.set_valign(gtk::Align::Center);
            labels.set_hexpand(true);
            let title = single_line(plain(&track.title));
            title.add_css_class("goosic-track-title");
            labels.append(&title);
            let subtitle = single_line(dim(&track.secondary_text()));
            subtitle.add_css_class("goosic-track-subtitle");
            labels.append(&subtitle);
            line.append(&labels);
            let button = gtk::Button::builder().child(&line).build();
            button.add_css_class("flat");
            button.add_css_class("goosic-track-row");
            let (track, context, actions) = (track.clone(), tracks.clone(), actions.clone());
            let menu_track = track.clone();
            let menu_context = context.clone();
            let menu_actions = actions.clone();
            if (actions.is_refused)(&track.video_id) {
                button.add_css_class("goosic-track-refused");
                button.set_tooltip_text(Some("This song isn’t available to play here."));
            }
            button.connect_clicked(move |_| (actions.play)(track.clone(), context.clone()));
            let item = row(4);
            button.set_hexpand(true);
            item.append(&button);
            item.append(&track_menu(&menu_track, &menu_context, &menu_actions));
            track_column.append(&item);
        }
        columns.append(&track_column);
    }
    gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Never)
        .propagate_natural_height(true)
        .child(&columns)
        .build()
}

/// macOS-style titled card carousel with trailing circular `<` `>` scroll chevrons.
fn card_shelf(title: &str, cards: &[Card], actions: &Rc<Actions>) -> gtk::Box {
    let (scroller, hadj) = card_strip(cards, actions);

    let header = row(8);
    header.set_valign(gtk::Align::Center);
    let heading = plain(title);
    heading.add_css_class("goosic-shelf-title");
    heading.set_hexpand(true);
    header.append(&heading);

    if cards.len() > 1 {
        let prev = gtk::Button::builder()
            .icon_name("go-previous-symbolic")
            .tooltip_text("Previous items")
            .valign(gtk::Align::Center)
            .build();
        prev.add_css_class("goosic-carousel-arrow");
        let next = gtk::Button::builder()
            .icon_name("go-next-symbolic")
            .tooltip_text("More items")
            .valign(gtk::Align::Center)
            .build();
        next.add_css_class("goosic-carousel-arrow");
        {
            let hadj = hadj.clone();
            prev.connect_clicked(move |_| {
                let step = (hadj.page_size() * 0.75).max(340.0);
                hadj.set_value((hadj.value() - step).max(hadj.lower()));
            });
        }
        {
            let hadj = hadj.clone();
            next.connect_clicked(move |_| {
                let step = (hadj.page_size() * 0.75).max(340.0);
                let max = (hadj.upper() - hadj.page_size()).max(0.0);
                hadj.set_value((hadj.value() + step).min(max));
            });
        }
        header.append(&prev);
        header.append(&next);
    }

    let shelf = column(10);
    shelf.append(&header);
    shelf.append(&scroller);
    shelf
}

fn card_strip(cards: &[Card], actions: &Rc<Actions>) -> (gtk::ScrolledWindow, gtk::Adjustment) {
    let strip = row(16);
    strip.set_margin_bottom(8);
    for card in cards {
        strip.append(&card_widget(card, actions));
    }
    let scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::External)
        .vscrollbar_policy(gtk::PolicyType::Never)
        .propagate_natural_height(true)
        .child(&strip)
        .build();
    let hadj = scroller.hadjustment();
    (scroller, hadj)
}

fn is_wide_video_card(card: &Card) -> bool {
    if !matches!(&card.action, Some(CardAction::Play(_))) {
        return false;
    }
    let lower = card.subtitle.to_ascii_lowercase();
    lower.starts_with("video") || lower.contains(" views")
}

fn card_widget(card: &Card, actions: &Rc<Actions>) -> gtk::Button {
    let is_artist = matches!(&card.action, Some(CardAction::Show(EntityReference::Artist(_))));
    let is_wide = is_wide_video_card(card);
    let art_width = if is_wide { 280 } else { 158 };
    let art_height = 158;
    let fallback_icon = if is_artist {
        "audio-input-microphone-symbolic"
    } else {
        "audio-x-generic-symbolic"
    };

    let body = column(6);
    body.set_size_request(art_width, -1);
    let art = artwork_picture(
        &actions.artwork,
        card.thumbnail.as_deref(),
        art_width,
        art_height,
        is_artist,
        fallback_icon,
    );
    if is_artist {
        art.set_halign(gtk::Align::Center);
        body.append(&art);
    } else {
        let art_overlay = gtk::Overlay::new();
        art_overlay.set_child(Some(&art));
        let badge = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        badge.add_css_class("goosic-card-badge");
        badge.set_halign(gtk::Align::End);
        badge.set_valign(gtk::Align::End);
        badge.set_margin_end(8);
        badge.set_margin_bottom(8);
        badge.set_can_target(false);
        let badge_icon = gtk::Image::builder()
            .icon_name(if matches!(&card.action, Some(CardAction::Play(_))) {
                "media-playback-start-symbolic"
            } else {
                "go-next-symbolic"
            })
            .pixel_size(14)
            .halign(gtk::Align::Center)
            .valign(gtk::Align::Center)
            .build();
        badge.append(&badge_icon);
        art_overlay.add_overlay(&badge);
        body.append(&art_overlay);
    }

    let title = single_line(plain(&card.title));
    title.add_css_class("goosic-card-title");
    title.set_max_width_chars(if is_wide { 36 } else { 20 });
    let subtitle = single_line(dim(&card.subtitle));
    subtitle.add_css_class("goosic-card-subtitle");
    subtitle.set_max_width_chars(if is_wide { 36 } else { 20 });
    if is_artist {
        title.set_xalign(0.5);
        subtitle.set_xalign(0.5);
    }
    body.append(&title);
    body.append(&subtitle);

    let button = gtk::Button::builder()
        .child(&body)
        .tooltip_text(&card.title)
        .build();
    button.add_css_class("flat");
    button.add_css_class("goosic-card");
    match &card.action {
        Some(CardAction::Show(entity)) => {
            let entity = entity.clone();
            let actions = actions.clone();
            button.connect_clicked(move |_| (actions.open)(entity.clone()));
        }
        Some(CardAction::Play(track)) => {
            let track = track.as_ref().clone();
            let actions = actions.clone();
            button.connect_clicked(move |_| (actions.play)(track.clone(), Rc::from(Vec::new())));
        }
        None => button.set_sensitive(false),
    }
    button
}

fn settings_page(facts: &ShellFacts, actions: &Rc<Actions>) -> gtk::Box {
    let page = column(14);

    let appearance = settings_group("Appearance");
    appearance.append(&dim("Choose how Goosic follows your desktop."));
    let themes = row(4);
    themes.add_css_class("goosic-search-segments");
    themes.set_halign(gtk::Align::Start);
    let mut first: Option<gtk::ToggleButton> = None;
    for option in Theme::ALL {
        let toggle = gtk::ToggleButton::with_label(theme::label(option));
        toggle.add_css_class("goosic-segment-button");
        match &first {
            Some(first) => toggle.set_group(Some(first)),
            None => first = Some(toggle.clone()),
        }
        toggle.set_active(option == facts.theme);
        let actions = actions.clone();
        toggle.connect_toggled(move |toggle| {
            if toggle.is_active() {
                (actions.set_theme)(option);
            }
        });
        themes.append(&toggle);
    }
    appearance.append(&themes);
    let backdrop = gtk::CheckButton::with_label("Use the playing cover as the background");
    backdrop.set_active(facts.artwork_background);
    {
        let actions = actions.clone();
        backdrop
            .connect_toggled(move |button| (actions.set_artwork_background)(button.is_active()));
    }
    appearance.append(&backdrop);
    page.append(&appearance);

    let content = settings_group("Content");
    let explicit = gtk::CheckButton::with_label("Hide explicit songs");
    explicit.set_active(facts.hide_explicit);
    {
        let actions = actions.clone();
        explicit.connect_toggled(move |button| (actions.set_hide_explicit)(button.is_active()));
    }
    content.append(&explicit);
    let start = column(8);
    start.append(&plain("Start page"));
    let start_row = row(4);
    start_row.add_css_class("goosic-search-segments");
    start_row.set_halign(gtk::Align::Start);
    let mut first_start: Option<gtk::ToggleButton> = None;
    for (value, label) in [
        ("home", "Home"),
        ("library", "Library"),
        ("liked", "Liked songs"),
        ("last", "Last page"),
    ] {
        let button = gtk::ToggleButton::with_label(label);
        button.add_css_class("goosic-segment-button");
        match &first_start {
            Some(first) => button.set_group(Some(first)),
            None => first_start = Some(button.clone()),
        }
        button.set_active(facts.start_page == value);
        let actions = actions.clone();
        button.connect_toggled(move |button| {
            if button.is_active() {
                (actions.set_start_page)(value.to_owned());
            }
        });
        start_row.append(&button);
    }
    start.append(&start_row);
    content.append(&start);
    page.append(&content);

    let playback_group = settings_group("Playback & Integrations");
    let autoplay = wrapped_check("Autoplay — keep the music going with related recommendations");
    autoplay.set_active(facts.autoplay);
    {
        let actions = actions.clone();
        autoplay.connect_toggled(move |_| (actions.toggle_autoplay)());
    }
    playback_group.append(&autoplay);
    let shuffle = wrapped_check("Shuffle — play the current queue in a randomized order");
    shuffle.set_active(facts.shuffle);
    {
        let actions = actions.clone();
        shuffle.connect_toggled(move |_| (actions.toggle_shuffle)());
    }
    playback_group.append(&shuffle);
    let discord = wrapped_check("Discord status — show the playing song on your Discord profile");
    discord.set_active(facts.discord_status);
    {
        let actions = actions.clone();
        discord.connect_toggled(move |button| (actions.set_discord_status)(button.is_active()));
    }
    playback_group.append(&discord);
    page.append(&playback_group);

    let motion = settings_group("Motion & Diagnostics");
    let reduce = wrapped_check("Reduce decorative motion");
    reduce.set_active(facts.reduce_motion);
    {
        let actions = actions.clone();
        reduce.connect_toggled(move |button| (actions.set_reduce_motion)(button.is_active()));
    }
    motion.append(&reduce);
    let debug = wrapped_check("Debug mode — show technical status details");
    debug.set_active(facts.debug_mode);
    {
        let actions = actions.clone();
        debug.connect_toggled(move |button| (actions.set_debug_mode)(button.is_active()));
    }
    motion.append(&debug);
    page.append(&motion);
    let shortcuts = gtk::Expander::builder().label("Keyboard shortcuts").build();
    let keys = column(6);
    for (key, action) in [
        ("Space", "Play / pause, outside text fields and controls"),
        ("Ctrl+Left / Right", "Previous / next song"),
        ("Shift+Left / Right", "Seek backward / forward ten seconds"),
        ("Ctrl+Up / Down", "Volume up / down"),
        ("Ctrl+M", "Mute"),
        ("Alt+Shift+B", "Like / unlike current song"),
        ("Ctrl+S / Ctrl+R", "Shuffle / repeat"),
        ("Ctrl+F / Ctrl+K", "Search"),
        ("Ctrl+Shift+F", "Find in playlist or album"),
        ("Ctrl+L / Ctrl+Alt+L", "Lyrics"),
        ("Ctrl+Shift+Q / Ctrl+Alt+U", "Up Next"),
        ("Ctrl+Alt+M", "Mini Player"),
        ("Ctrl+B", "Sidebar"),
        ("Ctrl+Shift+Alt+F / F11", "Full-Screen Player"),
        ("Esc", "Close full player, find bar, or panels"),
        ("Alt+Left / Ctrl+[", "Go back"),
        ("Ctrl+,", "Settings"),
        ("Ctrl+W / Ctrl+Q", "Close window / quit"),
    ] {
        keys.append(&dim(&format!("{key}  ·  {action}")));
    }
    shortcuts.set_child(Some(&keys));
    page.append(&shortcuts);

    let accounts = settings_group("Accounts");
    accounts.append(&dim(if facts.accounts.is_empty() {
        "Sign in to use your YouTube Music account."
    } else {
        "Choose the account Goosic uses for playback."
    }));
    let add = gtk::Button::builder()
        .label("Add account")
        .halign(gtk::Align::Start)
        .sensitive(facts.connected && !facts.account_busy)
        .build();
    add.add_css_class("goosic-glass-pill-action");
    {
        let actions = actions.clone();
        add.connect_clicked(move |_| (actions.sign_in)());
    }
    accounts.append(&add);
    for account in &facts.accounts {
        let active = facts.active_account_id.as_deref() == Some(account.id.as_str());
        let line = row(8);
        let text = column(2);
        text.set_hexpand(true);
        text.append(&plain(&account.display_name));
        text.append(&dim(account
            .email
            .as_deref()
            .or(account.channel.as_deref())
            .unwrap_or("YouTube Music account")));
        line.append(&text);
        if active {
            let label = dim("Active");
            label.set_valign(gtk::Align::Center);
            line.append(&label);
        } else {
            let switch = gtk::Button::with_label("Switch");
            switch.set_sensitive(!facts.account_busy);
            let (actions, id) = (actions.clone(), account.id.clone());
            switch.connect_clicked(move |_| (actions.switch_account)(id.clone()));
            line.append(&switch);
        }
        let leave = gtk::Button::with_label(if active { "Sign out" } else { "Remove" });
        leave.set_sensitive(!facts.account_busy);
        let (actions, id) = (actions.clone(), account.id.clone());
        leave.connect_clicked(move |_| {
            if active {
                (actions.sign_out)();
            } else {
                (actions.remove_account)(id.clone());
            }
        });
        line.append(&leave);
        accounts.append(&line);
    }
    accounts.append(&dim(
        "Sign-in data stays in each account's local web profile.",
    ));
    page.append(&accounts);

    let playback = settings_group("Playback");
    playback.append(&plain(
        "Volume, shuffle, repeat and autoplay are available in the player controls.",
    ));
    page.append(&playback);

    let migration = settings_group("Previous Goosic install");
    if facts.legacy_imported {
        migration.append(&dim(
            "Preferences imported. Your old data was left in place.",
        ));
    } else if facts.legacy_available {
        migration.append(&dim("Preferences from a previous install are available."));
        let import = gtk::Button::builder()
            .label("Import previous Goosic preferences")
            .halign(gtk::Align::Start)
            .build();
        import.add_css_class("goosic-glass-pill-action");
        let actions = actions.clone();
        import.connect_clicked(move |_| (actions.import_legacy)());
        migration.append(&import);
    } else {
        migration.append(&dim("No previous install was found."));
    }
    page.append(&migration);

    let connection = settings_group("Connection");
    connection.append(&plain(if facts.connected {
        "Connected"
    } else {
        "Unavailable. Restart Goosic to reconnect."
    }));
    page.append(&connection);
    page
}

fn settings_group(title: &str) -> gtk::Box {
    let group = column(10);
    group.add_css_class("goosic-settings-group");
    group.append(&markup(&format!(
        "<span size='large' weight='bold'>{}</span>",
        glib::markup_escape_text(title)
    )));
    group
}

fn retry_button(label: &str, actions: &Rc<Actions>) -> gtk::Button {
    let button = gtk::Button::builder()
        .label(label)
        .halign(gtk::Align::Start)
        .build();
    button.add_css_class("goosic-glass-pill-action");
    let actions = actions.clone();
    button.connect_clicked(move |_| (actions.retry)());
    button
}

/// Cover-fitted artwork inside a clipped rounded or circular frame.
fn artwork_picture(
    cache: &Rc<ArtworkCache>,
    remote: Option<&str>,
    width: i32,
    height: i32,
    round: bool,
    fallback_icon: &str,
) -> gtk::Frame {
    let icon = gtk::Image::builder()
        .icon_name(fallback_icon)
        .pixel_size((width.min(height) / 3).clamp(14, 42))
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .build();
    icon.add_css_class("dim-label");

    let picture = gtk::Picture::builder()
        .content_fit(gtk::ContentFit::Cover)
        .can_shrink(true)
        .width_request(width)
        .height_request(height)
        .build();
    cache.show_picture(remote, &picture);

    let overlay = gtk::Overlay::builder().child(&icon).build();
    overlay.add_overlay(&picture);

    let frame = gtk::Frame::builder()
        .child(&overlay)
        .width_request(width)
        .height_request(height)
        .valign(gtk::Align::Center)
        .build();
    frame.set_overflow(gtk::Overflow::Hidden);
    frame.add_css_class(if round {
        "goosic-artwork-round"
    } else {
        "goosic-artwork"
    });
    frame
}

fn padded(widget: &impl IsA<gtk::Widget>, top: i32, bottom: i32) -> gtk::Widget {
    widget.set_margin_start(24);
    widget.set_margin_end(24);
    widget.set_margin_top(top);
    widget.set_margin_bottom(bottom);
    widget.clone().upcast()
}

fn column(spacing: i32) -> gtk::Box {
    gtk::Box::new(gtk::Orientation::Vertical, spacing)
}

fn row(spacing: i32) -> gtk::Box {
    gtk::Box::new(gtk::Orientation::Horizontal, spacing)
}

fn plain(text: &str) -> gtk::Label {
    gtk::Label::builder()
        .label(text)
        .xalign(0.0)
        .wrap(true)
        .build()
}

fn wrapped_check(text: &str) -> gtk::CheckButton {
    let check = gtk::CheckButton::new();
    let label = plain(text);
    check.set_child(Some(&label));
    check
}

fn dim(text: &str) -> gtk::Label {
    let label = plain(text);
    label.add_css_class("dim-label");
    label
}

fn markup(markup: &str) -> gtk::Label {
    gtk::Label::builder()
        .label(markup)
        .use_markup(true)
        .xalign(0.0)
        .wrap(true)
        .build()
}

fn bold(text: &str) -> gtk::Label {
    markup(&format!("<b>{}</b>", glib::markup_escape_text(text)))
}

fn single_line(label: gtk::Label) -> gtk::Label {
    label.set_wrap(false);
    label.set_ellipsize(pango::EllipsizeMode::End);
    label
}
