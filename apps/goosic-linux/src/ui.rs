//! Drawing what `pages` decides.
//!
//! Every screen is one `GtkListView` over the rows `Browser::rows` returns. Only the rows on screen
//! have widgets, so a 500-track album costs what a dozen rows cost, and a row's widgets are built
//! when it is bound rather than kept for every row of every page.

use std::rc::Rc;

use goosic_shell_support::catalog::{Card, CardAction, Track};
use goosic_shell_support::navigation::{EntityReference, Route, SearchFilter};
use gtk::prelude::*;
use gtk::{gio, glib, pango};

use crate::pages::{route_title, PageRow};

const NO_PLAYER_YET: &str =
    "Playback arrives with the player, in a later slice of the Linux shell.";

/// What a row can ask the shell to do.
pub struct Actions {
    pub open: Box<dyn Fn(EntityReference)>,
    pub retry: Box<dyn Fn()>,
    pub back: Box<dyn Fn()>,
}

/// A scrolled list that draws `PageRow`s, and the store that feeds it.
pub fn page_list(actions: Rc<Actions>) -> (gtk::ScrolledWindow, gio::ListStore) {
    let store = gio::ListStore::new::<glib::BoxedAnyObject>();
    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(|_, item| {
        item.set_activatable(false);
        item.set_selectable(false);
        item.set_child(Some(&gtk::Box::new(gtk::Orientation::Vertical, 0)));
    });
    factory.connect_bind(move |_, item| {
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
    (scroller, store)
}

/// Replaces what the list shows, in one change rather than one per row.
pub fn set_rows(store: &gio::ListStore, rows: Vec<PageRow>) {
    let objects: Vec<glib::BoxedAnyObject> =
        rows.into_iter().map(glib::BoxedAnyObject::new).collect();
    store.splice(0, store.n_items(), &objects);
}

/// The sidebar: the routes, and below them whether the service is there.
pub struct Sidebar {
    pub root: gtk::Box,
    pub connection: gtk::Label,
    pub status: gtk::Label,
}

pub fn sidebar(on_select: impl Fn(Route) + 'static) -> Sidebar {
    let routes = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::Single)
        .vexpand(true)
        .build();
    routes.add_css_class("navigation-sidebar");
    for route in Route::ALL {
        let label = plain(route_title(route));
        label.set_margin_top(6);
        label.set_margin_bottom(6);
        label.set_margin_start(6);
        routes.append(&label);
    }
    routes.select_row(routes.row_at_index(0).as_ref());
    // Activated rather than selected, so choosing the route already selected still leaves a detail
    // page and goes back to it.
    routes.connect_row_activated(move |_, row| {
        if let Some(route) = usize::try_from(row.index())
            .ok()
            .and_then(|index| Route::ALL.get(index))
        {
            on_select(*route);
        }
    });

    let connection = plain("○ Connecting…");
    let status = dim("");
    let root = column(8);
    root.set_width_request(220);
    root.set_margin_top(16);
    root.set_margin_bottom(16);
    root.set_margin_start(12);
    root.set_margin_end(12);
    root.append(&markup("<b>GOOSIC</b>"));
    root.append(&dim("Your music, in motion"));
    root.append(&routes);
    root.append(&connection);
    root.append(&status);
    Sidebar {
        root,
        connection,
        status,
    }
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
            header.append(&markup(&format!(
                "<span size='xx-large' weight='bold'>{}</span>",
                glib::markup_escape_text(title)
            )));
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
        PageRow::Track(track) => padded(&track_row(track), 4, 4),
        PageRow::ShelfTitle(title) => padded(
            &markup(&format!(
                "<span size='large' weight='bold'>{}</span>",
                glib::markup_escape_text(title)
            )),
            18,
            6,
        ),
        PageRow::Cards(cards) => padded(&card_strip(cards, actions), 0, 8),
        PageRow::Truncated => padded(
            &dim("This page was long, so only the first part is shown."),
            4,
            18,
        ),
    }
}

fn track_row(track: &Track) -> gtk::Box {
    let line = row(10);
    line.append(&artwork_placeholder("♪", 34, 34));

    let text = column(2);
    text.set_hexpand(true);
    let title_line = row(6);
    title_line.append(&single_line(plain(&track.title)));
    if track.explicit {
        title_line.append(&dim("E"));
    }
    text.append(&title_line);
    text.append(&single_line(dim(&track.secondary_text())));
    line.append(&text);

    line.append(&dim(&track.duration));
    let play = gtk::Button::builder()
        .label("Play")
        .valign(gtk::Align::Center)
        .sensitive(false)
        .tooltip_text(NO_PLAYER_YET)
        .build();
    line.append(&play);
    line
}

fn card_strip(cards: &[Card], actions: &Rc<Actions>) -> gtk::ScrolledWindow {
    let strip = row(10);
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
    body.set_size_request(148, -1);
    body.append(&artwork_placeholder(glyph, 148, 82));
    let title = single_line(plain(&card.title));
    title.set_max_width_chars(18);
    let subtitle = single_line(dim(&card.subtitle));
    subtitle.set_max_width_chars(18);
    body.append(&title);
    body.append(&subtitle);

    let button = gtk::Button::builder().child(&body).build();
    button.add_css_class("flat");
    match &card.action {
        Some(CardAction::Show(entity)) => {
            let entity = entity.clone();
            let actions = actions.clone();
            button.connect_clicked(move |_| (actions.open)(entity.clone()));
        }
        Some(CardAction::Play(_)) => {
            button.set_sensitive(false);
            button.set_tooltip_text(Some(NO_PLAYER_YET));
        }
        // A row this build does not understand stays visible but inert.
        None => button.set_sensitive(false),
    }
    button
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

/// Where artwork will go once the shell fetches it; a glyph until then.
fn artwork_placeholder(glyph: &str, width: i32, height: i32) -> gtk::Frame {
    gtk::Frame::builder()
        .child(&gtk::Label::new(Some(glyph)))
        .width_request(width)
        .height_request(height)
        .build()
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
