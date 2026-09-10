//! Finding and starting the service this shell ships with.
//!
//! The shell runs the `goosic-service` installed beside its own executable — in the Flatpak that
//! is `/app/bin` — and never searches `PATH`, so a packaged shell always talks to the service it
//! was built with. `GOOSIC_SERVICE_PATH` overrides that for development, where the root workspace
//! builds the service and this workspace builds the shell.

use std::ffi::OsString;
use std::fmt::{self, write};
use std::path::{Path, PathBuf};

use goosic_shell_support::{ServiceClient, ServiceClientBuilder, TransportError};

const SERVICE_BINARY: &str = "goosic-service";

/// Where the service should be: the override when one is set and not empty, otherwise beside
/// `current_exe`.
pub fn resolve(override_path: Option<OsString>, current_exe: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = override_path.filter(|path| !path.is_empty()) {
        return Some(PathBuf::from(path));
    }
    Some(current_exe?.parent()?.join(SERVICE_BINARY))
}

/// Starts the service as this shell's private child.
pub fn launch() -> Result<ServiceClient, LaunchError> {
    let current_exe = std::env::current_exe().ok();
    let path = resolve(
        std::env::var_os("GOOSIC_SERVICE_PATH"),
        current_exe.as_deref(),
    )
    .ok_or(LaunchError::NoLocation)?;
    ServiceClientBuilder::new(path.clone())
        .request_id_prefix("linux")
        .spawn()
        .map_err(|error| LaunchError::Spawn { path, error })
}

/// Why the service could not be started.
#[derive(Debug)]
pub enum LaunchError {
    /// The shell could not tell where it is installed, so it had nowhere to look.
    NoLocation,
    Spawn {
        path: PathBuf,
        error: TransportError,
    },
}

impl fmt::Display for LaunchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LaunchError::NoLocation => write!(
                f,
                "could not tell where this program is installed, so {SERVICE_BINARY} could not \
                be found; set GOOSIC_SERVICE_PATH"
            ),
            LaunchError::Spawn { path, error } => write!(
                f,
                "{error} (looked for {}; set GOOSIC_SERVICE_PATH to use another)",
                path.display()
            ),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn an_override_wins_over_the_service_beside_the_shell() {
        let resolved = resolve(
            Some("/opt/dev/goosic-service".into()),
            Some(Path::new("/app/bin/goosic-linux")),
        );
        assert_eq!(resolved, Some(PathBuf::from("/opt/dev/goosic-service")));
    }

    #[test]
    fn without_an_override_the_service_is_the_one_beside_the_shell() {
        let resolved = resolve(None, Some(Path::new("/app/bin/goosic-linux")));
        assert_eq!(resolved, Some(PathBuf::from("/app/bin/goosic-service")));
    }

    /// An empty variable is what `GOOSIC_SERVICE_PATH= goosic-linux` produces, and it means unset,
    /// not "run the program called nothing".
    #[test]
    fn an_empty_override_is_ignored() {
        let resolved = resolve(
            Some(OsString::new()),
            Some(Path::new("/app/bin/goosic-linux")),
        );
        assert_eq!(resolved, Some(PathBuf::from("/app/bin/goosic-service")));
    }

    /// `PATH` is never consulted: with no override and no known location there is no answer,
    /// rather than whichever `goosic-service` happens to be installed.
    #[test]
    fn with_nothing_to_go_on_there_is_no_location() {
        assert_eq!(resolve(None, None), None);
    }
}