//! Light, dark, or whatever the desktop says.
//!
//! The shell uses GTK without libadwaita so that it takes each desktop's own theme, which means the
//! choice goes through GTK's own setting rather than a style manager. Since GTK 4.20 that setting
//! is `gtk-interface-color-scheme`: its default follows the desktop through the settings portal,
//! and light and dark override it for this application only. The older
//! `gtk-application-prefer-dark-theme` is deprecated as of 4.20 and is not touched.

use goosic_shell_support::navigation::Theme;
use gtk::glib::variant::ToVariant;
use gtk::prelude::WidgetExt;
use gtk::{gio, glib, InterfaceColorScheme};
use std::cell::Cell;
use std::rc::Rc;

thread_local! {
    static SYSTEM_DARK: Cell<Option<bool>> = const { Cell::new(None) };
    static SYSTEM_CONTRAST: Cell<bool> = const { Cell::new(false) };
}

type AppearanceUpdate = Rc<dyn Fn(&str, glib::Variant)>;

/// Watch the desktop preference rather than guessing from the GTK theme's name.
pub fn watch_system(changed: impl Fn() + 'static) -> Option<gio::SignalSubscription> {
    let connection = gio::bus_get_sync(gio::BusType::Session, None::<&gio::Cancellable>).ok()?;
    let changed: Rc<dyn Fn()> = Rc::new(changed);
    let update: AppearanceUpdate = Rc::new(move |key, value| {
        if let Some(scheme) = value.get::<u32>() {
            match key {
                "color-scheme" => SYSTEM_DARK.set(match scheme {
                    1 => Some(true),
                    2 => Some(false),
                    _ => None,
                }),
                "contrast" => SYSTEM_CONTRAST.set(scheme == 1),
                _ => return,
            }
            changed();
        }
    });
    let on_signal = update.clone();
    let subscription = connection.subscribe_to_signal(
        Some("org.freedesktop.portal.Desktop"),
        Some("org.freedesktop.portal.Settings"),
        Some("SettingChanged"),
        Some("/org/freedesktop/portal/desktop"),
        Some("org.freedesktop.appearance"),
        gio::DBusSignalFlags::NONE,
        move |signal| {
            if let Some((namespace, key, value)) =
                signal.parameters.get::<(String, String, glib::Variant)>()
            {
                if namespace == "org.freedesktop.appearance" {
                    on_signal(&key, value);
                }
            }
        },
    );
    for key in ["color-scheme", "contrast"] {
        let update = update.clone();
        connection.call(
            Some("org.freedesktop.portal.Desktop"),
            "/org/freedesktop/portal/desktop",
            "org.freedesktop.portal.Settings",
            "ReadOne",
            Some(&("org.freedesktop.appearance", key).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            5000,
            None::<&gio::Cancellable>,
            move |answer| {
                if let Ok(answer) = answer {
                    if let Some((value,)) = answer.get::<(glib::Variant,)>() {
                        update(key, value);
                    }
                }
            },
        );
    }
    Some(subscription)
}

/// Install the shell's surface styles once for the active GTK display.
pub fn install() {
    let Some(display) = gtk::gdk::Display::default() else {
        return;
    };
    let provider = gtk::CssProvider::new();
    provider.load_from_string(include_str!("ui.css"));
    gtk::style_context_add_provider_for_display(
        &display,
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

pub fn color_scheme(theme: Theme) -> InterfaceColorScheme {
    match theme {
        Theme::System => InterfaceColorScheme::Default,
        Theme::Light => InterfaceColorScheme::Light,
        Theme::Dark => InterfaceColorScheme::Dark,
    }
}

pub fn apply(theme: Theme) {
    if let Some(settings) = gtk::Settings::default() {
        settings.set_gtk_interface_color_scheme(color_scheme(theme));
    }
}

pub fn apply_window(window: &gtk::ApplicationWindow, theme: Theme) {
    let high_contrast = SYSTEM_CONTRAST.get()
        || gtk::Settings::default()
            .and_then(|s| s.gtk_theme_name())
            .is_some_and(|name| name.to_ascii_lowercase().contains("highcontrast"));
    if high_contrast {
        window.add_css_class("goosic-high-contrast");
    } else {
        window.remove_css_class("goosic-high-contrast");
    }
    let light = match theme {
        Theme::Light => true,
        Theme::Dark => false,
        Theme::System => SYSTEM_DARK.get().map(|dark| !dark).unwrap_or_else(|| {
            gtk::Settings::default()
                .and_then(|settings| settings.gtk_theme_name())
                .is_some_and(|name| !name.to_ascii_lowercase().contains("dark"))
        }),
    };
    if light {
        window.add_css_class("goosic-light");
    } else {
        window.remove_css_class("goosic-light");
    }
}

pub fn label(theme: Theme) -> &'static str {
    match theme {
        Theme::System => "System",
        Theme::Light => "Light",
        Theme::Dark => "Dark",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_hands_the_choice_back_to_the_desktop() {
        assert_eq!(color_scheme(Theme::System), InterfaceColorScheme::Default);
        assert_eq!(color_scheme(Theme::Light), InterfaceColorScheme::Light);
        assert_eq!(color_scheme(Theme::Dark), InterfaceColorScheme::Dark);
    }
}
