//! Guards for destructive commands.
//!
//! The app never builds a shell string out of a path or a package name. Commands
//! are argv lists, privileged ones go through `pkexec <absolute program> <args>`,
//! and every path handed to `rm` is resolved and checked first.

use std::fs;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};

pub const PKEXEC: &str = "pkexec";
/// Absolute program paths, so pkexec always runs the system binary.
pub const RM: &str = "/usr/bin/rm";
pub const PACMAN: &str = "/usr/bin/pacman";
pub const PACCACHE: &str = "/usr/bin/paccache";
pub const JOURNALCTL: &str = "/usr/bin/journalctl";

/// Directories that are never removed, whatever asks for it.
const PROTECTED_DIRS: &[&str] = &[
    "/",
    "/bin",
    "/boot",
    "/dev",
    "/etc",
    "/home",
    "/lib",
    "/lib64",
    "/mnt",
    "/opt",
    "/proc",
    "/root",
    "/run",
    "/sbin",
    "/srv",
    "/sys",
    "/tmp",
    "/usr",
    "/usr/bin",
    "/usr/sbin",
    "/usr/lib",
    "/usr/lib64",
    "/usr/share",
    "/usr/local",
    "/usr/local/bin",
    "/var",
    "/var/cache",
    "/var/lib",
    "/var/log",
];

fn depth(path: &Path) -> usize {
    path.components()
        .filter(|c| matches!(c, Component::Normal(_)))
        .count()
}

/// True for system directories and for the home directory or anything above it.
pub fn is_protected_location(path: &Path) -> bool {
    if PROTECTED_DIRS.iter().any(|p| path == Path::new(p)) {
        return true;
    }
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        if home.starts_with(path) {
            return true;
        }
        if let Ok(canon) = home.canonicalize() {
            if canon.starts_with(path) {
                return true;
            }
        }
    }
    false
}

/// Resolve the directory part of `path` and keep the final name as it is, so a
/// symlink is removed as a link and never followed to its target.
/// Returns `Ok(None)` when the entry does not exist.
pub fn resolve_entry(path: &Path) -> Result<Option<PathBuf>, String> {
    if !path.is_absolute() {
        return Err(format!("not an absolute path: {}", path.display()));
    }
    let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
        return Err(format!("no file name in path: {}", path.display()));
    };
    let parent = match parent.canonicalize() {
        Ok(p) => p,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("cannot resolve {}: {e}", parent.display())),
    };
    let resolved = parent.join(name);
    match fs::symlink_metadata(&resolved) {
        Ok(_) => Ok(Some(resolved)),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("cannot stat {}: {e}", resolved.display())),
    }
}

/// Check that `path` sits strictly inside `root` once both are resolved.
pub fn check_inside(root: &Path, path: &Path) -> Result<Option<PathBuf>, String> {
    let root = match root.canonicalize() {
        Ok(r) => r,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("cannot resolve {}: {e}", root.display())),
    };
    let Some(resolved) = resolve_entry(path)? else {
        return Ok(None);
    };
    if resolved == root || !resolved.starts_with(&root) || is_protected_location(&resolved) {
        return Err(format!(
            "refusing to remove {} (not inside {})",
            resolved.display(),
            root.display()
        ));
    }
    Ok(Some(resolved))
}

/// Check a standalone app path (AppImage, Wine prefix, desktop entry binary).
/// There is no cache directory to anchor it to, so refuse system directories,
/// the home directory and anything shallower than `/x/y`.
pub fn check_app_target(path: &Path) -> Result<PathBuf, String> {
    match resolve_entry(path)? {
        None => Err(format!("{} no longer exists", path.display())),
        Some(resolved) if depth(&resolved) < 2 || is_protected_location(&resolved) => Err(format!(
            "refusing to remove protected location {}",
            resolved.display()
        )),
        Some(resolved) => Ok(resolved),
    }
}

/// Check a directory whose children are about to be removed. The directory is
/// kept. A symlinked directory is refused so the clean cannot be redirected.
pub fn contents_of(dir: &Path) -> Result<Vec<PathBuf>, String> {
    if !dir.is_absolute() {
        return Err(format!("not an absolute path: {}", dir.display()));
    }
    match fs::symlink_metadata(dir) {
        Ok(meta) if meta.file_type().is_symlink() => {
            return Err(format!(
                "refusing to clean through symlink {}",
                dir.display()
            ));
        }
        Ok(_) => {}
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("cannot stat {}: {e}", dir.display())),
    }
    let canon = dir
        .canonicalize()
        .map_err(|e| format!("cannot resolve {}: {e}", dir.display()))?;
    if depth(&canon) < 3 || is_protected_location(&canon) {
        return Err(format!("refusing to clean {}", canon.display()));
    }
    let entries =
        fs::read_dir(&canon).map_err(|e| format!("cannot read {}: {e}", canon.display()))?;
    Ok(entries.flatten().map(|e| e.path()).collect())
}

/// Program and arguments for `rm -rf -- <paths>`, one argument per path.
pub fn rm_command(privileged: bool, paths: &[PathBuf]) -> (String, Vec<String>) {
    let mut args = vec!["-rf".to_string(), "--".to_string()];
    args.extend(paths.iter().map(|p| p.to_string_lossy().into_owned()));
    let program = if privileged { RM } else { "rm" };
    (program.to_string(), args)
}

/// Final argv for a command: `pkexec <program> <args>` when privileged.
pub fn argv(privileged: bool, program: &str, args: &[String]) -> (String, Vec<String>) {
    if privileged {
        let mut full = vec![program.to_string()];
        full.extend(args.iter().cloned());
        (PKEXEC.to_string(), full)
    } else {
        (program.to_string(), args.to_vec())
    }
}

/// Build the process for a command from [`argv`].
pub fn command(privileged: bool, program: &str, args: &[String]) -> std::process::Command {
    let (program, args) = argv(privileged, program, args);
    let mut cmd = std::process::Command::new(program);
    cmd.args(args);
    cmd
}
