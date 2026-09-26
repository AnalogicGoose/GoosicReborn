//! Light, dark, or whatever the desktop says.
//!
//! The shell uses GTK without libadwaita so that it takes each desktop's own theme, which means the
//! choice goes through GTK's own setting rather than a style manager. Since GTK 4.20 that setting
//! is `gtk-interface-color-scheme`: its default follows the desktop through the settings portal,
//! and light and dark override it for this application only. The older
//! `gtk-application-prefer-dark-theme` is deprecated as of 4.20 and is not touched.

use goosic_shell_support::navigation::Theme;
use gtk::prelude::WidgetExt;
use gtk::InterfaceColorScheme;

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
    let light = match theme {
        Theme::Light => true,
        Theme::Dark => false,
        Theme::System => gtk::Settings::default()
            .and_then(|settings| settings.gtk_theme_name())
            .is_some_and(|name| !name.to_ascii_lowercase().contains("dark")),
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
