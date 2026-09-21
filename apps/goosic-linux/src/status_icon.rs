//! A status icon, where the desktop has somewhere to put one.
//!
//! The icon is a StatusNotifierItem with a `com.canonical.dbusmenu` menu, spoken over the same gio
//! D-Bus connection the media-player interface uses. Plasma, Xfce and COSMIC show it; GNOME does not
//! without an extension, which is why the icon registers whenever a watcher appears rather than
//! once at startup, and why nothing depends on it — the window, the media panel and Ctrl+Q all do
//! what the menu does.
//!
//! Like the media-player interface it decides nothing. Which items are enabled comes from the
//! shared command availability, and a click is checked again against a snapshot taken at that
//! moment before it reaches the shell.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use goosic_shell_support::media::{CommandAvailability, MediaSnapshot};
use gtk::prelude::*;
use gtk::{gio, glib};

const ITEM_PATH: &str = "/StatusNotifierItem";
const MENU_PATH: &str = "/MenuBar";
const WATCHER: &str = "org.kde.StatusNotifierWatcher";

const INTROSPECTION: &str = r#"<node>
  <interface name='org.kde.StatusNotifierItem'>
    <method name='Activate'>
      <arg type='i' name='x' direction='in'/>
      <arg type='i' name='y' direction='in'/>
    </method>
    <method name='SecondaryActivate'>
      <arg type='i' name='x' direction='in'/>
      <arg type='i' name='y' direction='in'/>
    </method>
    <property name='Category' type='s' access='read'/>
    <property name='Id' type='s' access='read'/>
    <property name='Title' type='s' access='read'/>
    <property name='Status' type='s' access='read'/>
    <property name='IconName' type='s' access='read'/>
    <property name='ItemIsMenu' type='b' access='read'/>
    <property name='Menu' type='o' access='read'/>
  </interface>
  <interface name='com.canonical.dbusmenu'>
    <method name='GetLayout'>
      <arg type='i' name='parentId' direction='in'/>
      <arg type='i' name='recursionDepth' direction='in'/>
      <arg type='as' name='propertyNames' direction='in'/>
      <arg type='u' name='revision' direction='out'/>
      <arg type='(ia{sv}av)' name='layout' direction='out'/>
    </method>
    <method name='GetGroupProperties'>
      <arg type='ai' name='ids' direction='in'/>
      <arg type='as' name='propertyNames' direction='in'/>
      <arg type='a(ia{sv})' name='properties' direction='out'/>
    </method>
    <method name='GetProperty'>
      <arg type='i' name='id' direction='in'/>
      <arg type='s' name='name' direction='in'/>
      <arg type='v' name='value' direction='out'/>
    </method>
    <method name='Event'>
      <arg type='i' name='id' direction='in'/>
      <arg type='s' name='eventId' direction='in'/>
      <arg type='v' name='data' direction='in'/>
      <arg type='u' name='timestamp' direction='in'/>
    </method>
    <method name='EventGroup'>
      <arg type='a(isvu)' name='events' direction='in'/>
      <arg type='ai' name='idErrors' direction='out'/>
    </method>
    <method name='AboutToShow'>
      <arg type='i' name='id' direction='in'/>
      <arg type='b' name='needUpdate' direction='out'/>
    </method>
    <method name='AboutToShowGroup'>
      <arg type='ai' name='ids' direction='in'/>
      <arg type='ai' name='updatesNeeded' direction='out'/>
      <arg type='ai' name='idErrors' direction='out'/>
    </method>
    <signal name='LayoutUpdated'>
      <arg type='u' name='revision'/>
      <arg type='i' name='parent'/>
    </signal>
    <property name='Version' type='u' access='read'/>
    <property name='TextDirection' type='s' access='read'/>
    <property name='Status' type='s' access='read'/>
    <property name='IconThemePath' type='as' access='read'/>
  </interface>
</node>"#;

/// What the menu can ask the shell to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrayAction {
    Show,
    TogglePause,
    Next,
    Previous,
    Quit,
}

#[derive(Debug, Clone, PartialEq)]
struct MenuItem {
    id: i32,
    label: &'static str,
    enabled: bool,
    /// `None` for a separator.
    action: Option<TrayAction>,
}

/// The menu for one snapshot: what it offers is what the app would do.
fn menu(snapshot: &MediaSnapshot) -> Vec<MenuItem> {
    let allowed = CommandAvailability::from_snapshot(snapshot);
    let item = |id, label, enabled, action| MenuItem {
        id,
        label,
        enabled,
        action: Some(action),
    };
    vec![
        item(1, "Show Goosic", true, TrayAction::Show),
        item(
            2,
            if allowed.pause { "Pause" } else { "Play" },
            allowed.toggle_play_pause,
            TrayAction::TogglePause,
        ),
        item(3, "Next", allowed.next, TrayAction::Next),
        item(4, "Previous", allowed.previous, TrayAction::Previous),
        MenuItem {
            id: 5,
            label: "",
            enabled: true,
            action: None,
        },
        item(6, "Quit Goosic", true, TrayAction::Quit),
    ]
}

pub struct TrayHandlers {
    pub snapshot: Box<dyn Fn() -> MediaSnapshot>,
    pub show: Box<dyn Fn()>,
    pub toggle_pause: Box<dyn Fn()>,
    pub next: Box<dyn Fn()>,
    pub previous: Box<dyn Fn()>,
    pub quit: Box<dyn Fn()>,
}

pub struct StatusIcon {
    handlers: TrayHandlers,
    bus_name: String,
    revision: Cell<u32>,
    shown: RefCell<Vec<MenuItem>>,
    connection: RefCell<Option<gio::DBusConnection>>,
    owner: RefCell<Option<gio::OwnerId>>,
    /// The watch on the watcher's name lasts as long as Goosic runs, so it is never removed.
    watching: Cell<bool>,
}

impl StatusIcon {
    pub fn start(handlers: TrayHandlers) -> Rc<StatusIcon> {
        let icon = Rc::new(StatusIcon {
            handlers,
            bus_name: format!("org.kde.StatusNotifierItem-{}-1", std::process::id()),
            revision: Cell::new(1),
            shown: RefCell::new(Vec::new()),
            connection: RefCell::new(None),
            owner: RefCell::new(None),
            watching: Cell::new(false),
        });
        let (acquired, named) = (Rc::downgrade(&icon), Rc::downgrade(&icon));
        let owner = gio::bus_own_name(
            gio::BusType::Session,
            &icon.bus_name,
            gio::BusNameOwnerFlags::NONE,
            move |connection, _| {
                if let Some(icon) = acquired.upgrade() {
                    icon.register(connection);
                }
            },
            move |_, _| {
                if let Some(icon) = named.upgrade() {
                    icon.watch_for_watcher();
                }
            },
            |_, _| {},
        );
        *icon.owner.borrow_mut() = Some(owner);
        icon
    }

    /// Registers with the watcher whenever one appears, so a panel that starts after Goosic, or
    /// restarts, still shows the icon.
    fn watch_for_watcher(self: &Rc<Self>) {
        if self.watching.replace(true) {
            return;
        }
        let appeared = Rc::downgrade(self);
        let _ = gio::bus_watch_name(
            gio::BusType::Session,
            WATCHER,
            gio::BusNameWatcherFlags::NONE,
            move |connection, _, _| {
                if let Some(icon) = appeared.upgrade() {
                    icon.register_with(&connection);
                }
            },
            |_, _| {},
        );
    }

    fn register_with(&self, connection: &gio::DBusConnection) {
        connection.call(
            Some(WATCHER),
            "/StatusNotifierWatcher",
            WATCHER,
            "RegisterStatusNotifierItem",
            Some(&(self.bus_name.as_str(),).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            -1,
            None::<&gio::Cancellable>,
            |_| {},
        );
    }

    fn register(self: &Rc<Self>, connection: gio::DBusConnection) {
        let Ok(node) = gio::DBusNodeInfo::for_xml(INTROSPECTION) else {
            return;
        };
        for (path, name) in [
            (ITEM_PATH, "org.kde.StatusNotifierItem"),
            (MENU_PATH, "com.canonical.dbusmenu"),
        ] {
            let Some(info) = node.lookup_interface(name) else {
                continue;
            };
            let (call, get) = (Rc::downgrade(self), Rc::downgrade(self));
            let _ = connection
                .register_object(path, &info)
                .method_call(move |_, _, _, _, method, parameters, invocation| {
                    let reply = call
                        .upgrade()
                        .and_then(|icon| icon.invoke(method, &parameters));
                    invocation.return_value(reply.as_ref());
                })
                .property(move |_, _, _, interface, property| match get.upgrade() {
                    Some(icon) => icon.property(interface, property),
                    None => false.to_variant(),
                })
                .build();
        }
        *self.connection.borrow_mut() = Some(connection);
    }

    /// Tells the panel the menu changed, when it did.
    pub fn update(&self, snapshot: &MediaSnapshot) {
        let items = menu(snapshot);
        if *self.shown.borrow() == items {
            return;
        }
        *self.shown.borrow_mut() = items;
        let revision = self.revision.get() + 1;
        self.revision.set(revision);
        let connection = self.connection.borrow().clone();
        if let Some(connection) = connection {
            let _ = connection.emit_signal(
                None,
                MENU_PATH,
                "com.canonical.dbusmenu",
                "LayoutUpdated",
                Some(&(revision, 0_i32).to_variant()),
            );
        }
    }

    fn property(&self, interface: &str, name: &str) -> glib::Variant {
        if interface == "com.canonical.dbusmenu" {
            return match name {
                "Version" => 3_u32.to_variant(),
                "TextDirection" => "ltr".to_variant(),
                "Status" => "normal".to_variant(),
                _ => Vec::<String>::new().to_variant(),
            };
        }
        match name {
            "Category" => "ApplicationStatus".to_variant(),
            "Id" => "goosic".to_variant(),
            "Title" => "Goosic".to_variant(),
            "Status" => "Active".to_variant(),
            "IconName" => "multimedia-player".to_variant(),
            "ItemIsMenu" => false.to_variant(),
            _ => glib::variant::ObjectPath::try_from(MENU_PATH.to_owned())
                .expect("the menu path is a valid object path")
                .to_variant(),
        }
    }

    fn invoke(&self, method: &str, parameters: &glib::Variant) -> Option<glib::Variant> {
        let int = |index| {
            parameters
                .try_child_value(index)
                .and_then(|value| value.get::<i32>())
        };
        match method {
            "Activate" => {
                self.dispatch(TrayAction::Show);
                None
            }
            "SecondaryActivate" => {
                self.dispatch(TrayAction::TogglePause);
                None
            }
            "GetLayout" => {
                let items = self.current();
                let layout = match int(0).unwrap_or(0) {
                    0 => root_layout(&items),
                    id => items
                        .iter()
                        .find(|item| item.id == id)
                        .map_or_else(|| root_layout(&items), node),
                };
                Some(glib::Variant::tuple_from_iter([
                    self.revision.get().to_variant(),
                    layout,
                ]))
            }
            "GetGroupProperties" => {
                let items = self.current();
                let wanted: Vec<i32> = parameters
                    .try_child_value(0)
                    .and_then(|ids| ids.get::<Vec<i32>>())
                    .unwrap_or_default();
                let entries = items
                    .iter()
                    .filter(|item| wanted.is_empty() || wanted.contains(&item.id))
                    .map(|item| {
                        glib::Variant::tuple_from_iter([item.id.to_variant(), properties(item)])
                    });
                let array = glib::Variant::array_from_iter_with_type(
                    glib::VariantTy::new("(ia{sv})").expect("a valid type"),
                    entries,
                );
                Some(glib::Variant::tuple_from_iter([array]))
            }
            "GetProperty" => {
                let (id, name) = (
                    int(0).unwrap_or(0),
                    parameters
                        .try_child_value(1)
                        .and_then(|name| name.get::<String>())
                        .unwrap_or_default(),
                );
                let item = self.current().into_iter().find(|item| item.id == id);
                let value = match (item, name.as_str()) {
                    (Some(item), "label") => item.label.to_variant(),
                    (Some(item), "enabled") => item.enabled.to_variant(),
                    _ => "".to_variant(),
                };
                Some(glib::Variant::tuple_from_iter([
                    glib::Variant::from_variant(&value),
                ]))
            }
            "Event" => {
                let event = parameters
                    .try_child_value(1)
                    .and_then(|event| event.get::<String>());
                if event.as_deref() == Some("clicked") {
                    self.click(int(0).unwrap_or(0));
                }
                None
            }
            "EventGroup" => {
                if let Some(events) = parameters.try_child_value(0) {
                    for event in events.iter() {
                        let id = event.try_child_value(0).and_then(|id| id.get::<i32>());
                        let kind = event
                            .try_child_value(1)
                            .and_then(|kind| kind.get::<String>());
                        if let (Some(id), Some("clicked")) = (id, kind.as_deref()) {
                            self.click(id);
                        }
                    }
                }
                Some((Vec::<i32>::new(),).to_variant())
            }
            "AboutToShow" => Some((false,).to_variant()),
            "AboutToShowGroup" => Some((Vec::<i32>::new(), Vec::<i32>::new()).to_variant()),
            _ => None,
        }
    }

    fn current(&self) -> Vec<MenuItem> {
        menu(&(self.handlers.snapshot)())
    }

    /// A click, checked against the menu as it stands now rather than as the panel last drew it.
    fn click(&self, id: i32) {
        let action = self
            .current()
            .into_iter()
            .find(|item| item.id == id && item.enabled)
            .and_then(|item| item.action);
        if let Some(action) = action {
            self.dispatch(action);
        }
    }

    fn dispatch(&self, action: TrayAction) {
        let handler = match action {
            TrayAction::Show => &self.handlers.show,
            TrayAction::TogglePause => {
                if !CommandAvailability::from_snapshot(&(self.handlers.snapshot)())
                    .toggle_play_pause
                {
                    return;
                }
                &self.handlers.toggle_pause
            }
            TrayAction::Next => &self.handlers.next,
            TrayAction::Previous => &self.handlers.previous,
            TrayAction::Quit => &self.handlers.quit,
        };
        handler();
    }
}

impl Drop for StatusIcon {
    fn drop(&mut self) {
        if let Some(owner) = self.owner.borrow_mut().take() {
            gio::bus_unown_name(owner);
        }
    }
}

fn properties(item: &MenuItem) -> glib::Variant {
    let dict = glib::VariantDict::new(None);
    match item.action {
        None => dict.insert_value("type", &"separator".to_variant()),
        Some(_) => {
            dict.insert_value("label", &item.label.to_variant());
            dict.insert_value("enabled", &item.enabled.to_variant());
        }
    }
    dict.end()
}

fn node(item: &MenuItem) -> glib::Variant {
    glib::Variant::tuple_from_iter([
        item.id.to_variant(),
        properties(item),
        glib::Variant::array_from_iter_with_type(
            glib::VariantTy::VARIANT,
            std::iter::empty::<glib::Variant>(),
        ),
    ])
}

fn root_layout(items: &[MenuItem]) -> glib::Variant {
    let root = glib::VariantDict::new(None);
    root.insert_value("children-display", &"submenu".to_variant());
    let children = items
        .iter()
        .map(|item| glib::Variant::from_variant(&node(item)));
    glib::Variant::tuple_from_iter([
        0_i32.to_variant(),
        root.end(),
        glib::Variant::array_from_iter_with_type(glib::VariantTy::VARIANT, children),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use goosic_protocol::Owner;
    use goosic_shell_support::catalog::Track;
    use goosic_shell_support::navigation::PlaybackTransition;

    fn snapshot(ready: bool, paused: bool, advertisement: bool) -> MediaSnapshot {
        MediaSnapshot {
            track: Some(Track {
                id: "a".into(),
                title: "Song".into(),
                subtitle: String::new(),
                artist: "Artist".into(),
                artist_id: None,
                album: String::new(),
                album_id: None,
                duration: String::new(),
                video_id: "a".into(),
                explicit: false,
                thumbnail: None,
            }),
            current_time: 10.0,
            duration: 200.0,
            is_paused: paused,
            owner: Owner::OfficialWebView,
            is_advertisement: advertisement,
            has_queue: true,
            transition: PlaybackTransition::Idle,
            volume: 1.0,
            is_muted: false,
            is_ready: ready,
        }
    }

    fn find(items: &[MenuItem], id: i32) -> &MenuItem {
        items.iter().find(|item| item.id == id).unwrap()
    }

    #[test]
    fn the_menu_offers_what_the_app_would_do() {
        let playing = menu(&snapshot(true, false, false));
        assert_eq!(find(&playing, 2).label, "Pause");
        assert!(find(&playing, 2).enabled);
        assert!(find(&playing, 3).enabled, "next");

        let paused = menu(&snapshot(true, true, false));
        assert_eq!(find(&paused, 2).label, "Play");
    }

    #[test]
    fn nothing_confirmed_means_only_show_and_quit_work() {
        let idle = menu(&snapshot(false, true, false));
        assert!(find(&idle, 1).enabled && find(&idle, 6).enabled);
        assert!(!find(&idle, 2).enabled && !find(&idle, 3).enabled && !find(&idle, 4).enabled);
    }

    #[test]
    fn an_advertisement_cannot_be_skipped_from_the_menu_either() {
        let advertisement = menu(&snapshot(true, false, true));
        assert!(!find(&advertisement, 3).enabled, "next");
        assert!(!find(&advertisement, 4).enabled, "previous");
    }

    #[test]
    fn a_separator_carries_no_action() {
        assert_eq!(find(&menu(&snapshot(true, false, false)), 5).action, None);
    }
}
