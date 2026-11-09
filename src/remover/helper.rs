//! Runs root operations through `donutcleaner-helper`.
//!
//! The app builds a typed [`Op`] and starts `pkexec <helper> <op> <args>`. The
//! helper validates the request again on its own side. Nothing else in the app
//! calls `pkexec`.

use std::path::Path;
use std::process::Command;

use donutcleaner_ops::{HELPER_PATH, Op, POLICY_PATH};

/// True when the helper binary is installed where the polkit policy expects it.
pub fn helper_installed() -> bool {
    Path::new(HELPER_PATH).is_file()
}

/// What to tell the user when the helper is missing.
pub fn missing_message() -> String {
    format!(
        "The administrator helper is not installed at {HELPER_PATH}. Install DonutCleaner from \
         a package, or copy donutcleaner-helper to that path and the polkit policy to {POLICY_PATH} \
         as root."
    )
}

/// The command line as shown to the user.
pub fn display(op: &Op) -> String {
    let mut parts = vec!["pkexec".to_string(), HELPER_PATH.to_string()];
    parts.extend(op.to_args());
    parts.join(" ")
}

/// Run one helper operation. Returns success plus the helper's output.
pub fn run(op: &Op) -> (bool, String) {
    if !helper_installed() {
        return (false, missing_message());
    }
    eprintln!("CLEAN [root]: {}", display(op));
    match Command::new("pkexec").arg(HELPER_PATH).args(op.to_args()).output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let text = format!("{stdout}{stderr}").trim().to_string();
            // pkexec exits 126 when the user dismisses the prompt, 127 when not authorized.
            match output.status.code() {
                Some(126) | Some(127) => (false, "Authorization cancelled".into()),
                _ => (output.status.success(), text),
            }
        }
        Err(e) => (false, format!("cannot run pkexec: {e}")),
    }
}
