//! Drawing what `pages` decides.
//!
//! Every screen is one `GtkListView` over the rows `Browser::rows` returns. Only the rows on screen
//! have widgets, so a 500-track album costs what a dozen rows cost, and a row's widgets are built
//! when it is bound rather than kept for every row of every page.

use std::rc::Rc;

use goosic_shell_support::catalog::{Card, CardAction, Track};
use goosic_shell_support::navigation::{EntityReference, Route, SearchFilter, Theme};
use gtk::prelude::*;
use gtk::{gio, glib, pango};

use crate::artwork::ArtworkCache;
use crate::pages::{route_title, size_text, LibrarySection, MoreRow, PageRow, ShellFacts};
use crate::theme;
use goosic_protocol::DownloadedTrack;

/// What a row can ask the shell to do, and the artwork cache rows draw from.
pub struct Actions {
    pub open: Box<dyn Fn(EntityReference)>,
    /// Plays a track; the list is what becomes the queue, and may be empty.
    pub play: Box<dyn Fn(Track, Rc<[Track]>)>,
    pub retry: Box<dyn Fn()>,
    pub back: Box<dyn Fn()>,
    pub set_theme: Box<dyn Fn(Theme)>,
    pub set_hide_explicit: Box<dyn Fn(bool)>,
    pub set_start_page: Box<dyn Fn(String)>,
    pub set_reduce_motion: Box<dyn Fn(bool)>,
    pub import_legacy: Box<dyn Fn()>,
    /// Fetches the next part of a page that continues.
    pub load_more: Box<dyn Fn()>,
    pub play_download: Box<dyn Fn(DownloadedTrack)>,
    pub refresh_downloads: Box<dyn Fn()>,
    pub import_downloads: Box<dyn Fn()>,
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
    pub add_to_playlist: Box<dyn Fn(String)>,
    pub can_edit_library: Box<dyn Fn() -> bool>,
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
    // Reaching the bottom fetches the next part, as the previous Goosic's feed did. A page short
    // enough never to scroll still has its Load more row.
    scroller.connect_edge_reached(move |_, edge| {
        if edge == gtk::PositionType::Bottom {
            (at_end.load_more)();
        }
    });
    (scroller, store)
}

fn list_item(object: &glib::Object) -> &gtk::ListItem {
    object
        .downcast_ref::<gtk::ListItem>()
        .expect("the page list has no section headers")
}

/// Replaces what the list shows, in one change. Only the rows after the last unchanged one are
/// replaced, so a page that grew at its end keeps its scroll position and the rows on screen keep
/// their widgets.
pub fn set_rows(store: &gio::ListStore, rows: Vec<PageRow>) {
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

/// The sidebar: the routes, and below them whether the service is there.
pub struct Sidebar {
    pub root: gtk::Box,
    pub routes: gtk::ListBox,
    pub account: gtk::Label,
    pub avatar: gtk::Label,
    pub connection: gtk::Label,
    pub status: gtk::Label,
}

const SIDEBAR_ROUTES: [Option<Route>; 12] = [
    Some(Route::Home),
    None,
    Some(Route::Explore),
    Some(Route::Charts),
    Some(Route::MoodsAndGenres),
    Some(Route::NewReleases),
    None,
    Some(Route::Library),
    Some(Route::Downloads),
    Some(Route::Settings),
    None,
    Some(Route::Library),
];

pub fn sidebar(
    on_select: impl Fn(Route) + 'static,
    on_search: impl Fn(String) + 'static,
) -> Sidebar {
    let on_select = Rc::new(on_select);
    let routes = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::Single)
        .vexpand(true)
        .build();
    routes.add_css_class("navigation-sidebar");
    for (index, route) in SIDEBAR_ROUTES.into_iter().enumerate() {
        let Some(route) = route else {
            let heading = match index {
                1 => "Discover",
                6 => "Library",
                10 => "Playlists",
                _ => unreachable!("only section headers have no route"),
            };
            let label = plain(heading);
            label.add_css_class("goosic-sidebar-section");
            label.set_margin_start(10);
            label.set_margin_top(16);
            label.set_margin_bottom(4);
            let header = gtk::ListBoxRow::new();
            header.set_selectable(false);
            header.set_activatable(false);
            header.set_child(Some(&label));
            routes.append(&header);
            continue;
        };
        let icon = gtk::Label::new(Some(if index == 11 {
            "≡"
        } else {
            route_glyph(route)
        }));
        icon.add_css_class("goosic-route-icon");
        icon.set_width_chars(2);
        let label = plain(if index == 11 {
            "All Playlists"
        } else {
            route_title(route)
        });
        let item = row(10);
        item.set_margin_top(5);
        item.set_margin_bottom(5);
        item.set_margin_start(10);
        item.append(&icon);
        item.append(&label);
        routes.append(&item);
    }
    select_route(&routes, Route::Home);
    // Activated rather than selected, so choosing the route already selected still leaves a detail
    // page and goes back to it.
    let select_route = on_select.clone();
    routes.connect_row_activated(move |_, row| {
        if let Some(route) = usize::try_from(row.index())
            .ok()
            .and_then(|index| SIDEBAR_ROUTES.get(index))
            .and_then(|route| *route)
        {
            select_route(route);
        }
    });

    let connection = dim("Connecting…");
    connection.add_css_class("goosic-account-connection");
    let status = dim("");
    let account = plain("Browsing as guest");
    account.add_css_class("goosic-account-name");
    let avatar = plain("G");
    avatar.add_css_class("goosic-account-avatar");
    let account_text = column(1);
    account_text.set_valign(gtk::Align::Center);
    account_text.append(&account);
    account_text.append(&connection);
    let account_contents = row(10);
    account_contents.append(&avatar);
    account_contents.append(&account_text);
    let account_button = gtk::Button::new();
    account_button.set_child(Some(&account_contents));
    account_button.add_css_class("goosic-account-row");
    account_button.connect_clicked(move |_| on_select(Route::Settings));
    let root = column(8);
    root.set_width_request(280);
    let titlebar_space = gtk::Box::new(gtk::Orientation::Vertical, 0);
    titlebar_space.set_height_request(38);
    root.append(&titlebar_space);
    let search = gtk::SearchEntry::builder()
        .placeholder_text("Search")
        .build();
    search.connect_activate(move |entry| on_search(entry.text().to_string()));
    root.append(&search);
    root.append(&routes);
    root.append(&account_button);
    root.append(&status);
    Sidebar {
        root,
        routes,
        account,
        avatar,
        connection,
        status,
    }
}

fn route_glyph(route: Route) -> &'static str {
    match route {
        Route::Home => "⌂",
        Route::Explore => "◈",
        Route::Search => "⌕",
        Route::Library => "▣",
        Route::Charts => "≋",
        Route::MoodsAndGenres => "◎",
        Route::NewReleases => "✦",
        Route::Downloads => "⇩",
        Route::Settings => "⚙",
    }
}

/// Marks `route` as the one on screen, for when the shell chose it rather than the user.
pub fn select_route(routes: &gtk::ListBox, route: Route) {
    let index = SIDEBAR_ROUTES
        .iter()
        .position(|candidate| *candidate == Some(route));
    let row = index.and_then(|index| routes.row_at_index(i32::try_from(index).ok()?));
    routes.select_row(row.as_ref());
}

/// The search field and its filter tabs, shown only on the Search route.
pub fn search_bar(
    on_submit: impl Fn(String) + 'static,
    on_filter: impl Fn(SearchFilter) + 'static,
) -> gtk::Box {
    let on_submit = Rc::new(on_submit);
    let entry = gtk::SearchEntry::builder()
        .placeholder_text("Search music")
        .hexpand(true)
        .build();
    let search = gtk::Button::with_label("Search");
    {
        let on_submit = on_submit.clone();
        entry.connect_activate(move |entry| on_submit(entry.text().to_string()));
    }
    {
        let entry = entry.clone();
        search.connect_clicked(move |_| on_submit(entry.text().to_string()));
    }
    let query = row(8);
    query.append(&entry);
    query.append(&search);

    let on_filter = Rc::new(on_filter);
    let filters = row(6);
    let mut first: Option<gtk::ToggleButton> = None;
    for filter in SearchFilter::ALL {
        let toggle = gtk::ToggleButton::with_label(filter_label(filter));
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
        filters.append(&toggle);
    }

    let root = column(10);
    root.set_margin_top(16);
    root.set_margin_start(24);
    root.set_margin_end(24);
    root.append(&query);
    root.append(&filters);
    root
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
        PageRow::Back => {
            let back = gtk::Button::builder()
                .label("‹ Back")
                .halign(gtk::Align::Start)
                .build();
            let actions = actions.clone();
            back.connect_clicked(move |_| (actions.back)());
            padded(&back, 16, 0)
        }
        PageRow::Header { title, subtitle } => {
            let header = column(4);
            let heading = plain(title);
            heading.add_css_class("goosic-page-title");
            header.append(&heading);
            header.append(&dim(subtitle));
            padded(&header, 16, 8)
        }
        PageRow::GuestNotice => {
            let notice = row(8);
            notice.append(&bold("GUEST"));
            notice.append(&dim(
                "Live YouTube Music catalog, browsed without an account",
            ));
            padded(&notice, 0, 8)
        }
        PageRow::LibrarySections(active) => {
            let tabs = row(6);
            for section in LibrarySection::ALL {
                let button = gtk::Button::with_label(section.title());
                button.add_css_class(if section == *active {
                    "suggested-action"
                } else {
                    "flat"
                });
                let actions = actions.clone();
                button.connect_clicked(move |_| (actions.select_library_section)(section));
                tabs.append(&button);
            }
            padded(&tabs, 4, 12)
        }
        PageRow::LibraryActions => {
            let button = gtk::Button::with_label("Create playlist");
            button.set_halign(gtk::Align::Start);
            let actions = actions.clone();
            button.connect_clicked(move |_| (actions.create_playlist)());
            padded(&button, 4, 12)
        }
        PageRow::PlaylistActions(id) => {
            let controls = row(6);
            let save = gtk::Button::with_label("Save to library");
            let remove = gtk::Button::with_label("Remove from library");
            let manage = gtk::Button::with_label("Manage playlist");
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
                .label("Play all")
                .halign(gtk::Align::Start)
                .build();
            let (actions, tracks) = (actions.clone(), tracks.clone());
            play_all.connect_clicked(move |_| {
                if let Some(first) = tracks.first() {
                    (actions.play)(first.clone(), tracks.clone());
                }
            });
            padded(&play_all, 4, 8)
        }
        PageRow::Track { track, context } => padded(&track_row(track, context, actions), 4, 4),
        PageRow::ShelfTitle(title) => {
            let heading = plain(title);
            heading.add_css_class("goosic-shelf-title");
            padded(&heading, 18, 6)
        }
        PageRow::TrackShelf(tracks) => padded(&track_shelf(tracks, actions), 0, 8),
        PageRow::Cards(cards) => padded(&card_strip(cards, actions), 0, 8),
        PageRow::Truncated => padded(
            &dim("This page was long, so only the first part is shown."),
            4,
            18,
        ),
        PageRow::Settings(facts) => padded(&settings_page(facts, actions), 8, 24),
        PageRow::DownloadsToolbar { busy } => {
            let toolbar = column(8);
            let buttons = row(8);
            let refresh = gtk::Button::with_label("Refresh");
            let import = gtk::Button::with_label("Import previous Goosic files");
            refresh.set_sensitive(!busy);
            import.set_sensitive(!busy);
            {
                let actions = actions.clone();
                refresh.connect_clicked(move |_| (actions.refresh_downloads)());
            }
            {
                let actions = actions.clone();
                import.connect_clicked(move |_| (actions.import_downloads)());
            }
            buttons.append(&refresh);
            buttons.append(&import);
            if *busy {
                buttons.append(&gtk::Spinner::builder().spinning(true).build());
            }
            toolbar.append(&buttons);
            toolbar.append(&dim(
                "Goosic plays finalized files already on disk. It never starts a downloader and \
                 never reads account cookies.",
            ));
            padded(&toolbar, 0, 12)
        }
        PageRow::Download(track) => padded(&download_row(track, actions), 4, 4),
        PageRow::More(more) => {
            let line = row(8);
            let action_button = |label: &str| {
                let button = gtk::Button::with_label(label);
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

fn track_row(track: &Track, context: &Rc<[Track]>, actions: &Rc<Actions>) -> gtk::Box {
    let line = row(10);
    line.append(&artwork(actions, track.thumbnail.as_deref(), "♪", 40));

    let text = column(2);
    text.set_hexpand(true);
    text.set_valign(gtk::Align::Center);
    let title_line = row(6);
    let title = single_line(plain(&track.title));
    title.add_css_class("goosic-track-title");
    title_line.append(&title);
    if track.explicit {
        title_line.append(&dim("E"));
    }
    text.append(&title_line);
    let subtitle = single_line(dim(&track.secondary_text()));
    subtitle.add_css_class("goosic-track-subtitle");
    text.append(&subtitle);
    line.append(&text);

    let duration = dim(&track.duration);
    duration.set_valign(gtk::Align::Center);
    line.append(&duration);
    let button = gtk::Button::builder()
        .child(&line)
        .tooltip_text("Play")
        .build();
    button.add_css_class("flat");
    button.add_css_class("goosic-track-row");
    let (selected, context, play_actions) = (track.clone(), context.clone(), actions.clone());
    button.connect_clicked(move |_| (play_actions.play)(selected.clone(), context.clone()));
    let outer = row(4);
    button.set_hexpand(true);
    outer.append(&button);
    if (actions.can_edit_library)() {
        outer.append(&track_menu(&track.video_id, actions));
    }
    outer
}

fn track_menu(video_id: &str, actions: &Rc<Actions>) -> gtk::MenuButton {
    let popover = gtk::Popover::new();
    let menu = column(4);
    menu.set_margin_start(8);
    menu.set_margin_end(8);
    menu.set_margin_top(8);
    menu.set_margin_bottom(8);
    let like = gtk::Button::with_label("Like song");
    let unlike = gtk::Button::with_label("Remove like");
    let add = gtk::Button::with_label("Add to playlist");
    let id = video_id.to_owned();
    let action = actions.clone();
    let close = popover.clone();
    like.connect_clicked(move |_| {
        close.popdown();
        (action.like_track)(id.clone());
    });
    let id = video_id.to_owned();
    let action = actions.clone();
    let close = popover.clone();
    unlike.connect_clicked(move |_| {
        close.popdown();
        (action.unlike_track)(id.clone());
    });
    let id = video_id.to_owned();
    let action = actions.clone();
    let close = popover.clone();
    add.connect_clicked(move |_| {
        close.popdown();
        (action.add_to_playlist)(id.clone());
    });
    menu.append(&like);
    menu.append(&unlike);
    menu.append(&add);
    popover.set_child(Some(&menu));
    gtk::MenuButton::builder()
        .icon_name("view-more-symbolic")
        .tooltip_text("Song actions")
        .popover(&popover)
        .build()
}

fn track_shelf(tracks: &Rc<[Track]>, actions: &Rc<Actions>) -> gtk::ScrolledWindow {
    let columns = row(22);
    columns.set_margin_bottom(12);
    for group in tracks.chunks(4) {
        let track_column = column(0);
        track_column.set_width_request(330);
        for track in group {
            let line = row(12);
            line.append(&artwork(actions, track.thumbnail.as_deref(), "♪", 48));
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
            let video_id = track.video_id.clone();
            let menu_actions = actions.clone();
            button.connect_clicked(move |_| (actions.play)(track.clone(), context.clone()));
            let item = row(4);
            button.set_hexpand(true);
            item.append(&button);
            if (menu_actions.can_edit_library)() {
                item.append(&track_menu(&video_id, &menu_actions));
            }
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

fn download_row(track: &DownloadedTrack, actions: &Rc<Actions>) -> gtk::Box {
    let line = row(10);
    line.append(&artwork(actions, None, "♪", 40));

    let text = column(2);
    text.set_hexpand(true);
    text.set_valign(gtk::Align::Center);
    text.append(&single_line(plain(&track.title)));
    let mut detail = vec![track.artist.clone(), size_text(track.bytes)];
    if track.imported {
        detail.push("From the previous Goosic".to_owned());
    }
    if !track.available {
        detail.push("Missing from disk".to_owned());
    }
    detail.retain(|part| !part.is_empty());
    text.append(&single_line(dim(&detail.join(" · "))));
    line.append(&text);

    let play = gtk::Button::builder()
        .icon_name("media-playback-start-symbolic")
        .tooltip_text(if track.available {
            "Play the downloaded file"
        } else {
            "This file is missing from disk"
        })
        .valign(gtk::Align::Center)
        .sensitive(track.available)
        .build();
    let (track, actions) = (track.clone(), actions.clone());
    play.connect_clicked(move |_| (actions.play_download)(track.clone()));
    line.append(&play);
    line
}

fn card_strip(cards: &[Card], actions: &Rc<Actions>) -> gtk::ScrolledWindow {
    let strip = row(22);
    // Room below the cards for the overlay scrollbar, so it never covers the last line of text.
    strip.set_margin_bottom(12);
    for card in cards {
        strip.append(&card_widget(card, actions));
    }
    gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Never)
        .propagate_natural_height(true)
        .child(&strip)
        .build()
}

fn card_widget(card: &Card, actions: &Rc<Actions>) -> gtk::Button {
    let glyph = match card.action {
        Some(CardAction::Play(_)) => "▶",
        _ => "♪",
    };
    let body = column(5);
    body.set_size_request(196, -1);
    body.append(&artwork(actions, card.thumbnail.as_deref(), glyph, 196));
    let title = single_line(plain(&card.title));
    title.add_css_class("goosic-card-title");
    title.set_max_width_chars(24);
    let subtitle = single_line(dim(&card.subtitle));
    subtitle.add_css_class("goosic-card-subtitle");
    subtitle.set_max_width_chars(24);
    body.append(&title);
    body.append(&subtitle);

    let button = gtk::Button::builder().child(&body).build();
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
        // A row this build does not understand stays visible but inert.
        None => button.set_sensitive(false),
    }
    button
}

fn settings_page(facts: &ShellFacts, actions: &Rc<Actions>) -> gtk::Box {
    let page = column(14);

    let appearance = settings_group("Appearance");
    appearance.append(&dim("Choose how Goosic follows your desktop."));
    let themes = row(6);
    let mut first: Option<gtk::ToggleButton> = None;
    for option in Theme::ALL {
        let toggle = gtk::ToggleButton::with_label(theme::label(option));
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
    page.append(&appearance);

    let content = settings_group("Content");
    let explicit = gtk::CheckButton::with_label("Hide explicit songs");
    explicit.set_active(facts.hide_explicit);
    {
        let actions = actions.clone();
        explicit.connect_toggled(move |button| (actions.set_hide_explicit)(button.is_active()));
    }
    content.append(&explicit);
    let start = row(8);
    start.append(&plain("Start page"));
    let mut first_start: Option<gtk::ToggleButton> = None;
    for (value, label) in [
        ("home", "Home"),
        ("library", "Library"),
        ("liked", "Liked songs"),
        ("last", "Last page"),
    ] {
        let button = gtk::ToggleButton::with_label(label);
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
        start.append(&button);
    }
    content.append(&start);
    page.append(&content);

    let motion = settings_group("Motion");
    let reduce = gtk::CheckButton::with_label("Reduce decorative motion");
    reduce.set_active(facts.reduce_motion);
    {
        let actions = actions.clone();
        reduce.connect_toggled(move |button| (actions.set_reduce_motion)(button.is_active()));
    }
    motion.append(&reduce);
    page.append(&motion);

    let accounts = settings_group("Account");
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
    let actions = actions.clone();
    button.connect_clicked(move |_| (actions.retry)());
    button
}

/// A square of artwork: the glyph until the image arrives, and for good if it never does.
fn artwork(actions: &Actions, remote: Option<&str>, glyph: &str, size: i32) -> gtk::Frame {
    // An image draws its content at a fixed size, whatever the thumbnail's own dimensions.
    let image = gtk::Image::builder().pixel_size(size).build();
    actions.artwork.show(remote, &image);
    let overlay = gtk::Overlay::builder()
        .child(&gtk::Label::new(Some(glyph)))
        .build();
    overlay.add_overlay(&image);
    let frame = gtk::Frame::builder()
        .child(&overlay)
        .width_request(size)
        .height_request(size)
        .valign(gtk::Align::Center)
        .build();
    frame.set_overflow(gtk::Overflow::Hidden);
    frame.add_css_class("goosic-artwork");
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
