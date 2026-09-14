//! Light, dark, or whatever the desktop says.
//!
//! The shell uses GTK without libadwaita so that it takes each desktop's own theme, which means the
//! choice goes through GTK's own setting rather than a style manager. Since GTK 4.20 that setting
//! is `gtk-interface-color-scheme`: its default follows the desktop through the settings portal,
//! and light and dark override it for this application only. The older
//! `gtk-application-prefer-dark-theme` is deprecated as of 4.20 and is not touched.

use goosic_shell_support::navigation::Theme;
use gtk::InterfaceColorScheme;

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
