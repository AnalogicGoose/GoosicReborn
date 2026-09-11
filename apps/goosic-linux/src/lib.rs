//! Goosic for Linux: a GTK 4 shell over the Rust playback authority.
//!
//! The shell renders and asks; `goosic-service` decides. Everything this program decides that has
//! no machine in it comes from `goosic-shell-support`, and everything it knows about GTK stays
//! here. See `docs/LINUX_SHELL.md` for why it is built this way.

pub mod bridge;
pub mod pages;
pub mod service;
pub mod shell;
mod ui;

/// The application ID. It makes a second launch activate the running instance instead of starting
/// another, and it is how the desktop, its portals and the media panel recognise this program.
pub const APP_ID: &str = "io.github.analogicgoose.Goosic";
