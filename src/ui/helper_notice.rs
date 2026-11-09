//! A one-time notice when the administrator helper or its polkit policy is missing.
//!
//! An AppImage, the standalone binary and an unpacked tarball cannot write to
//! /usr, so they cannot install the helper themselves. The dialog shows the exact
//! commands instead. A marker file in the config folder keeps it to the first run.

use std::path::PathBuf;

use adw::prelude::*;

use crate::remover::helper;
use crate::ui::window::Window;

const BODY: &str = "Package removal and system cleanup go through a small helper program and a \
polkit policy. Both belong in system folders, and this copy has no write access there. \
Scanning and cleaning inside your home folder work as they are. For the rest, run these \
commands once in a terminal.";

fn marker_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("donutcleaner").join("helper-notice-shown"))
}

fn already_shown() -> bool {
    marker_path().is_some_and(|p| p.exists())
}

fn mark_shown() {
    if let Some(path) = marker_path() {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, b"");
    }
}

/// Show the notice on the first run when the helper or the policy is missing.
pub fn maybe_show(window: &Window) {
    if (helper::helper_installed() && helper::policy_installed()) || already_shown() {
        return;
    }
    // The AppImage runtime sets APPIMAGE to the path of the file being run.
    let appimage = std::env::var("APPIMAGE").ok().filter(|p| !p.is_empty());
    let steps = helper::install_steps(appimage.as_deref());

    let dialog = adw::AlertDialog::new(Some("Set up administrator access"), Some(BODY));
    let text_view = gtk::TextView::new();
    text_view.set_editable(false);
    text_view.set_monospace(true);
    text_view.set_wrap_mode(gtk::WrapMode::WordChar);
    text_view.add_css_class("card");
    text_view.buffer().set_text(&steps);
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_child(Some(&text_view));
    scroll.set_min_content_height(190);
    scroll.set_min_content_width(620);
    dialog.set_extra_child(Some(&scroll));
    dialog.add_response("copy", "Copy commands");
    dialog.add_response("close", "Close");
    dialog.set_default_response(Some("close"));
    dialog.set_close_response("close");
    dialog.connect_response(None, move |dialog, response| {
        if response == "copy" {
            dialog.clipboard().set_text(&steps);
        }
    });
    mark_shown();
    dialog.present(Some(window));
}
