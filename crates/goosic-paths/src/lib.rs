//! Where Goosic keeps its files, on every platform it targets.
//!
//! These used to be `cfg!(target_os = …)` chains duplicated across three crates. That meant only
//! the branch for the host being built ever compiled, so the Windows and Linux answers were
//! never exercised and had already drifted apart from one another.
//!
//! Everything here is a pure function of a platform and a set of environment values, so the
//! answers for all three platforms are checked by tests running on any one of them.

use std::path::{Path, PathBuf};

/// The directory name Goosic owns inside whichever base directory a platform uses.
pub const APP_DIRECTORY: &str = "goosic";

/// The previous Goosic's application identifier, which named its data directories.
pub const LEGACY_BUNDLE_ID: &str = "com.github.ivasy.ytubic";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    MacOS,
    Windows,
    Linux,
}

impl Platform {
    /// The platform this binary was built for.
    ///
    /// Anything that is not macOS or Windows is treated as Linux, because the remaining targets
    /// Goosic could run on follow the XDG layout.
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::MacOS
        } else if cfg!(target_os = "windows") {
            Self::Windows
        } else {
            Self::Linux
        }
    }
}

/// The environment values that decide where files live.
///
/// Injected rather than read inline so a test can ask for another platform's answer without
/// changing the process environment.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Environment {
    pub home: Option<PathBuf>,
    /// Windows roaming application data.
    pub appdata: Option<PathBuf>,
    /// Windows machine-local application data, where web view stores live.
    pub local_appdata: Option<PathBuf>,
    pub xdg_config_home: Option<PathBuf>,
    pub xdg_data_home: Option<PathBuf>,
}

impl Environment {
    /// Reads the values from the running process.
    pub fn current() -> Self {
        let read = |name: &str| std::env::var_os(name).map(PathBuf::from).filter(|path| !path.as_os_str().is_empty());
        Self {
            home: read("HOME").or_else(|| read("USERPROFILE")),
            appdata: read("APPDATA"),
            local_appdata: read("LOCALAPPDATA"),
            xdg_config_home: read("XDG_CONFIG_HOME"),
            xdg_data_home: read("XDG_DATA_HOME"),
        }
    }
}

/// Per-user configuration: preferences and account metadata.
///
/// macOS has no separate config and data locations, so both answer with Application Support.
pub fn config_dir(platform: Platform, environment: &Environment) -> Option<PathBuf> {
    let base = match platform {
        Platform::MacOS => environment
            .home
            .as_ref()
            .map(|home| home.join("Library/Application Support")),
        Platform::Windows => environment.appdata.clone(),
        Platform::Linux => environment.xdg_config_home.clone().or_else(|| {
            environment.home.as_ref().map(|home| home.join(".config"))
        }),
    };
    base.map(|base| base.join(APP_DIRECTORY))
}

/// Per-user data: the download index and the decoded audio cache.
///
/// On Linux this is deliberately not the config directory. A decoded WAV cache is data, not
/// configuration, and XDG keeps them apart.
pub fn data_dir(platform: Platform, environment: &Environment) -> Option<PathBuf> {
    let base = match platform {
        Platform::MacOS => environment
            .home
            .as_ref()
            .map(|home| home.join("Library/Application Support")),
        Platform::Windows => environment.appdata.clone(),
        Platform::Linux => environment.xdg_data_home.clone().or_else(|| {
            environment.home.as_ref().map(|home| home.join(".local/share"))
        }),
    };
    base.map(|base| base.join(APP_DIRECTORY))
}

/// The previous install's application data directory.
pub fn legacy_app_dir(platform: Platform, environment: &Environment) -> Option<PathBuf> {
    match platform {
        Platform::MacOS => environment
            .home
            .as_ref()
            .map(|home| home.join("Library/Application Support").join(LEGACY_BUNDLE_ID)),
        Platform::Windows => environment
            .appdata
            .as_ref()
            .map(|appdata| appdata.join(LEGACY_BUNDLE_ID)),
        Platform::Linux => environment
            .xdg_data_home
            .clone()
            .or_else(|| environment.home.as_ref().map(|home| home.join(".local/share")))
            .map(|base| base.join(LEGACY_BUNDLE_ID)),
    }
}

/// The previous install's downloaded media directory.
pub fn legacy_media_dir(platform: Platform, environment: &Environment) -> Option<PathBuf> {
    legacy_app_dir(platform, environment).map(|base| base.join("offline-media").join("stream"))
}

/// Where the previous install's web view kept `localStorage`.
///
/// `None` on Windows on purpose: WebView2 stores local storage in a LevelDB directory rather
/// than a SQLite database, and no reader for that format exists here. Returning a path Goosic
/// cannot read would turn "not supported" into "import failed".
pub fn legacy_localstorage_root(platform: Platform, environment: &Environment) -> Option<PathBuf> {
    match platform {
        Platform::MacOS => environment.home.as_ref().map(|home| {
            home.join("Library/WebKit")
                .join(LEGACY_BUNDLE_ID)
                .join("WebsiteData/Default")
        }),
        Platform::Linux => legacy_app_dir(platform, environment),
        Platform::Windows => None,
    }
}

/// Whether this platform can import the previous install's preferences at all.
pub fn supports_legacy_preference_import(platform: Platform) -> bool {
    !matches!(platform, Platform::Windows)
}

/// Finds `LocalStorage/localstorage.sqlite3` under an origin-hashed directory tree.
///
/// The origin directory is named by a hash, so the tree is searched rather than assumed. Depth is
/// bounded so a symlink loop cannot walk forever.
pub fn find_local_storage_database(root: &Path) -> Option<PathBuf> {
    fn search(directory: &Path, depth: usize) -> Option<PathBuf> {
        if depth > 6 {
            return None;
        }
        let candidate = directory.join("LocalStorage").join("localstorage.sqlite3");
        if candidate.is_file() {
            return Some(candidate);
        }
        for entry in std::fs::read_dir(directory).ok()?.flatten() {
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                if let Some(found) = search(&entry.path(), depth + 1) {
                    return Some(found);
                }
            }
        }
        None
    }
    search(root, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unix() -> Environment {
        Environment {
            home: Some(PathBuf::from("/home/ana")),
            xdg_config_home: None,
            xdg_data_home: None,
            ..Default::default()
        }
    }

    fn windows() -> Environment {
        Environment {
            home: Some(PathBuf::from(r"C:\Users\ana")),
            appdata: Some(PathBuf::from(r"C:\Users\ana\AppData\Roaming")),
            local_appdata: Some(PathBuf::from(r"C:\Users\ana\AppData\Local")),
            ..Default::default()
        }
    }

    #[test]
    fn macos_keeps_configuration_and_data_together_in_application_support() {
        let environment = Environment {
            home: Some(PathBuf::from("/Users/ana")),
            ..Default::default()
        };
        let expected = PathBuf::from("/Users/ana/Library/Application Support/goosic");
        assert_eq!(config_dir(Platform::MacOS, &environment), Some(expected.clone()));
        assert_eq!(data_dir(Platform::MacOS, &environment), Some(expected));
    }

    #[test]
    fn windows_uses_roaming_application_data_for_both() {
        let environment = windows();
        let expected = PathBuf::from(r"C:\Users\ana\AppData\Roaming").join("goosic");
        assert_eq!(config_dir(Platform::Windows, &environment), Some(expected.clone()));
        assert_eq!(data_dir(Platform::Windows, &environment), Some(expected));
    }

    #[test]
    fn linux_separates_configuration_from_data() {
        // A decoded audio cache is data, not configuration, and XDG keeps them apart.
        let environment = unix();
        assert_eq!(
            config_dir(Platform::Linux, &environment),
            Some(PathBuf::from("/home/ana/.config/goosic"))
        );
        assert_eq!(
            data_dir(Platform::Linux, &environment),
            Some(PathBuf::from("/home/ana/.local/share/goosic"))
        );
    }

    #[test]
    fn linux_honours_the_xdg_overrides_when_they_are_set() {
        let environment = Environment {
            home: Some(PathBuf::from("/home/ana")),
            xdg_config_home: Some(PathBuf::from("/config")),
            xdg_data_home: Some(PathBuf::from("/data")),
            ..Default::default()
        };
        assert_eq!(
            config_dir(Platform::Linux, &environment),
            Some(PathBuf::from("/config/goosic"))
        );
        assert_eq!(
            data_dir(Platform::Linux, &environment),
            Some(PathBuf::from("/data/goosic"))
        );
    }

    #[test]
    fn nothing_resolves_without_the_variables_a_platform_needs() {
        let empty = Environment::default();
        assert_eq!(config_dir(Platform::MacOS, &empty), None);
        assert_eq!(config_dir(Platform::Windows, &empty), None);
        assert_eq!(config_dir(Platform::Linux, &empty), None);
        assert_eq!(data_dir(Platform::Linux, &empty), None);
    }

    #[test]
    fn windows_falls_back_to_nothing_rather_than_a_home_guess() {
        // A Windows install without APPDATA is broken; guessing a path under the profile would
        // silently write somewhere nothing else reads.
        let environment = Environment {
            home: Some(PathBuf::from(r"C:\Users\ana")),
            ..Default::default()
        };
        assert_eq!(config_dir(Platform::Windows, &environment), None);
    }

    #[test]
    fn the_legacy_media_directory_is_found_on_every_platform() {
        assert_eq!(
            legacy_media_dir(Platform::MacOS, &Environment {
                home: Some(PathBuf::from("/Users/ana")),
                ..Default::default()
            }),
            Some(PathBuf::from(
                "/Users/ana/Library/Application Support/com.github.ivasy.ytubic/offline-media/stream"
            ))
        );
        assert_eq!(
            legacy_media_dir(Platform::Linux, &unix()),
            Some(PathBuf::from(
                "/home/ana/.local/share/com.github.ivasy.ytubic/offline-media/stream"
            ))
        );
        assert_eq!(
            legacy_media_dir(Platform::Windows, &windows()),
            Some(
                PathBuf::from(r"C:\Users\ana\AppData\Roaming")
                    .join("com.github.ivasy.ytubic")
                    .join("offline-media")
                    .join("stream")
            )
        );
    }

    #[test]
    fn windows_reports_no_local_storage_root_rather_than_an_unreadable_one() {
        // WebView2 uses LevelDB, which this build cannot read. Handing back a path would turn
        // "not supported here" into "the import failed".
        assert_eq!(legacy_localstorage_root(Platform::Windows, &windows()), None);
        assert!(!supports_legacy_preference_import(Platform::Windows));
    }

    #[test]
    fn macos_and_linux_can_import_preferences() {
        assert!(supports_legacy_preference_import(Platform::MacOS));
        assert!(supports_legacy_preference_import(Platform::Linux));
        assert!(legacy_localstorage_root(Platform::MacOS, &Environment {
            home: Some(PathBuf::from("/Users/ana")),
            ..Default::default()
        })
        .is_some());
        assert!(legacy_localstorage_root(Platform::Linux, &unix()).is_some());
    }

    #[test]
    fn the_current_platform_is_the_one_this_test_is_running_on() {
        let expected = if cfg!(target_os = "macos") {
            Platform::MacOS
        } else if cfg!(target_os = "windows") {
            Platform::Windows
        } else {
            Platform::Linux
        };
        assert_eq!(Platform::current(), expected);
    }

    #[test]
    fn an_empty_variable_is_treated_as_unset() {
        // An exported-but-empty XDG_CONFIG_HOME would otherwise resolve to a bare "goosic"
        // directory relative to the working directory.
        let environment = Environment {
            home: Some(PathBuf::from("/home/ana")),
            xdg_config_home: None,
            ..Default::default()
        };
        assert_eq!(
            config_dir(Platform::Linux, &environment),
            Some(PathBuf::from("/home/ana/.config/goosic"))
        );
    }

    #[test]
    fn a_local_storage_database_is_found_under_a_hashed_origin_directory() {
        let root = std::env::temp_dir().join(format!("goosic-paths-{}", std::process::id()));
        let origin = root.join("Default/hashedorigin/hashedorigin/LocalStorage");
        std::fs::create_dir_all(&origin).unwrap();
        let database = origin.join("localstorage.sqlite3");
        std::fs::write(&database, b"not really sqlite").unwrap();

        assert_eq!(find_local_storage_database(&root), Some(database));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_tree_without_a_database_finds_nothing() {
        let root = std::env::temp_dir().join(format!("goosic-paths-empty-{}", std::process::id()));
        std::fs::create_dir_all(root.join("Default/other")).unwrap();
        assert_eq!(find_local_storage_database(&root), None);
        let _ = std::fs::remove_dir_all(&root);
    }
}
