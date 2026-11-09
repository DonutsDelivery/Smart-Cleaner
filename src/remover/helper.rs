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

/// True when the polkit policy that names the helper is installed.
pub fn policy_installed() -> bool {
    Path::new(POLICY_PATH).is_file()
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

/// Quote one word for a POSIX shell.
fn shell_quote(word: &str) -> String {
    format!("'{}'", word.replace('\'', "'\\''"))
}

/// The commands that put the helper and the policy in place. `appimage` is the path
/// of the running AppImage, if there is one: the two files travel inside it and
/// `--appimage-extract` pulls them out. Otherwise they come from the release tarball.
pub fn install_steps(appimage: Option<&str>) -> String {
    let install = format!(
        "sudo install -Dm755 {helper_src} {HELPER_PATH}\nsudo install -Dm644 {policy_src} {POLICY_PATH}",
        helper_src = if appimage.is_some() {
            "squashfs-root/usr/lib/donutcleaner/donutcleaner-helper"
        } else {
            "donutcleaner-helper"
        },
        policy_src = if appimage.is_some() {
            "squashfs-root/usr/share/polkit-1/actions/online.donutsdelivery.DonutCleaner.helper.policy"
        } else {
            "online.donutsdelivery.DonutCleaner.helper.policy"
        },
    );
    match appimage {
        Some(path) => {
            let app = shell_quote(path);
            format!(
                "cd \"$(mktemp -d)\"\n{app} --appimage-extract 'usr/lib/donutcleaner/*'\n\
                 {app} --appimage-extract 'usr/share/polkit-1/actions/*'\n{install}"
            )
        }
        None => format!(
            "# Run in the folder you unpacked donutcleaner-x86_64.tar.gz into.\n{install}"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_quote_survives_quotes_and_spaces() {
        assert_eq!(shell_quote("/tmp/a b"), "'/tmp/a b'");
        assert_eq!(shell_quote("/tmp/it's"), "'/tmp/it'\\''s'");
        assert_eq!(shell_quote("$(id)"), "'$(id)'");
    }

    #[test]
    fn appimage_steps_extract_then_install() {
        let steps = install_steps(Some("/home/me/My Apps/DonutCleaner.AppImage"));
        assert!(steps.contains("'/home/me/My Apps/DonutCleaner.AppImage' --appimage-extract"));
        assert!(steps.contains("sudo install -Dm755 squashfs-root/usr/lib/donutcleaner/donutcleaner-helper /usr/lib/donutcleaner/donutcleaner-helper"));
        assert!(steps.contains(POLICY_PATH));
    }

    #[test]
    fn tarball_steps_install_from_the_current_folder() {
        let steps = install_steps(None);
        assert!(steps.contains("sudo install -Dm755 donutcleaner-helper /usr/lib/donutcleaner/donutcleaner-helper"));
        assert!(!steps.contains("appimage-extract"));
    }
}
