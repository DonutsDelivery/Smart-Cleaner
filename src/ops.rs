//! The operations the root helper (`donutcleaner-helper`) accepts.
//!
//! The app builds an [`Op`] and turns it into argv with [`Op::to_args`]. The
//! helper parses that argv back with [`Op::parse`] and refuses anything that is
//! not in the list below. There is no free-form command string, no shell, and no
//! path outside a small allowlist. This file uses only `std`, so the helper
//! links nothing else.

use std::fs;
use std::io::ErrorKind;
use std::os::unix::fs::MetadataExt;
use std::path::{Component, Path, PathBuf};

/// Where the packages install the helper. The polkit policy names this exact path.
pub const HELPER_PATH: &str = "/usr/lib/donutcleaner/donutcleaner-helper";
pub const POLKIT_ACTION_ID: &str = "online.donutsdelivery.DonutCleaner.helper";
pub const POLICY_PATH: &str =
    "/usr/share/polkit-1/actions/online.donutsdelivery.DonutCleaner.helper.policy";

/// Most package names or paths one call may carry.
pub const MAX_PACKAGES: usize = 200;
pub const MAX_PATHS: usize = 64;
const MAX_NAME_LEN: usize = 255;
const MAX_PATH_LEN: usize = 4096;

/// Packages the helper never removes, whatever the app asks for.
pub const PROTECTED_PACKAGES: &[&str] = &[
    "base",
    "filesystem",
    "glibc",
    "bash",
    "coreutils",
    "systemd",
    "pacman",
    "polkit",
    "linux",
    "linux-lts",
    "linux-zen",
    "linux-hardened",
    "linux-firmware",
    "donutcleaner",
    "donutcleaner-bin",
];

/// A directory whose contents the helper empties. The app names it by key, never by path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CleanDir {
    DebtapCache,
    Coredumps,
}

impl CleanDir {
    pub fn key(self) -> &'static str {
        match self {
            CleanDir::DebtapCache => "debtap-cache",
            CleanDir::Coredumps => "coredumps",
        }
    }

    pub fn path(self) -> &'static str {
        match self {
            CleanDir::DebtapCache => "/var/cache/debtap",
            CleanDir::Coredumps => "/var/lib/systemd/coredump",
        }
    }

    fn from_key(key: &str) -> Option<Self> {
        [CleanDir::DebtapCache, CleanDir::Coredumps]
            .into_iter()
            .find(|d| d.key() == key)
    }
}

/// A directory the helper may remove entries from, one level deep.
#[derive(Debug, Clone)]
pub struct RemoveRoot {
    pub dir: PathBuf,
    /// False for a directory that holds single executables, so only files and links go.
    pub allow_dirs: bool,
}

/// The directories `remove-paths` accepts children of. Each entry must be a direct
/// child, so `/opt/foo` is fine and `/opt/foo/bar` or `/opt` itself is not.
pub fn system_remove_roots() -> Vec<RemoveRoot> {
    vec![
        RemoveRoot {
            dir: PathBuf::from("/opt"),
            allow_dirs: true,
        },
        RemoveRoot {
            dir: PathBuf::from("/usr/local/bin"),
            allow_dirs: false,
        },
        RemoveRoot {
            dir: PathBuf::from("/usr/local/sbin"),
            allow_dirs: false,
        },
    ]
}

/// One request to the helper.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    /// `paccache -rk0`: drop every cached package file.
    PaccacheAll,
    /// `paccache -ruk0`: drop cached files of uninstalled packages.
    PaccacheUninstalled,
    /// `journalctl --vacuum-time=1week`.
    JournalVacuum,
    /// Empty one of the fixed cache directories.
    ClearDir(CleanDir),
    /// `pacman -Rns` on whatever `pacman -Qdtq` lists when the helper runs.
    RemoveOrphans,
    /// `pacman -Rns` on these installed packages.
    RemovePackages(Vec<String>),
    /// Remove these direct children of an allowlisted directory.
    RemovePaths(Vec<String>),
}

impl Op {
    pub fn name(&self) -> &'static str {
        match self {
            Op::PaccacheAll => "paccache-all",
            Op::PaccacheUninstalled => "paccache-uninstalled",
            Op::JournalVacuum => "journal-vacuum",
            Op::ClearDir(_) => "clear-dir",
            Op::RemoveOrphans => "remove-orphans",
            Op::RemovePackages(_) => "remove-packages",
            Op::RemovePaths(_) => "remove-paths",
        }
    }

    /// Arguments after the helper's own path.
    pub fn to_args(&self) -> Vec<String> {
        let mut args = vec![self.name().to_string()];
        match self {
            Op::ClearDir(dir) => args.push(dir.key().to_string()),
            Op::RemovePackages(names) | Op::RemovePaths(names) => args.extend(names.iter().cloned()),
            _ => {}
        }
        args
    }

    /// Parse and validate the helper's argv (without the program name).
    pub fn parse(args: &[String]) -> Result<Op, String> {
        let (name, rest) = args.split_first().ok_or("no operation given")?;
        let none = |op: Op| {
            if rest.is_empty() {
                Ok(op)
            } else {
                Err(format!("{name} takes no arguments"))
            }
        };
        match name.as_str() {
            "paccache-all" => none(Op::PaccacheAll),
            "paccache-uninstalled" => none(Op::PaccacheUninstalled),
            "journal-vacuum" => none(Op::JournalVacuum),
            "remove-orphans" => none(Op::RemoveOrphans),
            "clear-dir" => match rest {
                [key] => CleanDir::from_key(key)
                    .map(Op::ClearDir)
                    .ok_or_else(|| format!("unknown directory key: {key}")),
                _ => Err("clear-dir takes exactly one directory key".to_string()),
            },
            "remove-packages" => {
                check_count(rest.len(), MAX_PACKAGES, "package")?;
                for pkg in rest {
                    check_package_name(pkg)?;
                }
                check_no_duplicates(rest)?;
                Ok(Op::RemovePackages(rest.to_vec()))
            }
            "remove-paths" => {
                check_count(rest.len(), MAX_PATHS, "path")?;
                let roots = system_remove_roots();
                for path in rest {
                    split_removal_path(path, &roots)?;
                }
                check_no_duplicates(rest)?;
                Ok(Op::RemovePaths(rest.to_vec()))
            }
            other => Err(format!("unknown operation: {other}")),
        }
    }
}

fn check_count(n: usize, max: usize, what: &str) -> Result<(), String> {
    if n == 0 {
        Err(format!("no {what} given"))
    } else if n > max {
        Err(format!("too many {what}s (limit {max})"))
    } else {
        Ok(())
    }
}

fn check_no_duplicates(items: &[String]) -> Result<(), String> {
    let mut seen = std::collections::HashSet::new();
    for item in items {
        if !seen.insert(item.as_str()) {
            return Err(format!("duplicate argument: {item}"));
        }
    }
    Ok(())
}

/// A pacman package name: letters, digits and `@ . _ + -`, not starting with `-` or `.`.
/// This keeps option flags, repository prefixes (`core/foo`), globs and paths out.
pub fn check_package_name(name: &str) -> Result<(), String> {
    let mut chars = name.chars();
    let first_ok = chars
        .next()
        .is_some_and(|c| c.is_ascii_alphanumeric() || matches!(c, '@' | '_' | '+'));
    let rest_ok = name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '@' | '.' | '_' | '+' | '-'));
    if first_ok && rest_ok && name.len() <= MAX_NAME_LEN {
        Ok(())
    } else {
        Err(format!("not a valid package name: {name:?}"))
    }
}

pub fn is_protected_package(name: &str) -> bool {
    PROTECTED_PACKAGES.contains(&name)
}

/// Check the text of a path for `remove-paths` and split it into the matching root
/// (an index into `roots`) and the entry name. Purely lexical: the path must be
/// absolute, already normalized (no `..`, `.`, `//` or trailing `/`), and a direct
/// child of one of the roots.
pub fn split_removal_path<'a>(
    text: &'a str,
    roots: &[RemoveRoot],
) -> Result<(usize, &'a str), String> {
    if text.is_empty() || text.len() > MAX_PATH_LEN || text.contains('\0') {
        return Err(format!("not an acceptable path: {text:?}"));
    }
    let path = Path::new(text);
    let mut normal = String::new();
    for (i, comp) in path.components().enumerate() {
        match (i, comp) {
            (0, Component::RootDir) => {}
            (_, Component::Normal(part)) => {
                normal.push('/');
                normal.push_str(part.to_str().ok_or("path is not valid UTF-8")?);
            }
            _ => return Err(format!("path is not absolute and normalized: {text:?}")),
        }
    }
    if normal != text {
        return Err(format!("path is not absolute and normalized: {text:?}"));
    }
    let (parent, name) = text.rsplit_once('/').ok_or("path has no parent")?;
    if name.is_empty() {
        return Err(format!("path has no entry name: {text:?}"));
    }
    match roots.iter().position(|r| r.dir == Path::new(parent)) {
        Some(i) => Ok((i, name)),
        None => Err(format!("outside the allowed directories: {text}")),
    }
}

/// Outcome of removing one entry.
#[derive(Debug, PartialEq, Eq)]
pub enum Removal {
    Removed,
    AlreadyGone,
}

/// A directory the helper acts inside must be one a normal user cannot reshape:
/// a real directory (not a link), owned by `owner_uid`, writable by nobody else,
/// reached without crossing a symlink, and sitting under parents nobody else can
/// rename it out of. That is what lets the later checks stay valid until the removal.
fn check_trusted_dir(dir: &Path, owner_uid: u32) -> Result<fs::Metadata, String> {
    let meta = fs::symlink_metadata(dir).map_err(|e| format!("cannot stat {}: {e}", dir.display()))?;
    if !meta.is_dir() {
        return Err(format!("{} is not a directory", dir.display()));
    }
    if meta.uid() != owner_uid {
        return Err(format!("{} has the wrong owner", dir.display()));
    }
    if meta.mode() & 0o022 != 0 {
        return Err(format!("{} is writable by group or others", dir.display()));
    }
    let canon = dir
        .canonicalize()
        .map_err(|e| format!("cannot resolve {}: {e}", dir.display()))?;
    if canon != dir {
        return Err(format!(
            "{} goes through a symlink to {}",
            dir.display(),
            canon.display()
        ));
    }
    for parent in dir.ancestors().skip(1) {
        let pm = fs::symlink_metadata(parent)
            .map_err(|e| format!("cannot stat {}: {e}", parent.display()))?;
        let owned = pm.uid() == 0 || pm.uid() == owner_uid;
        // A sticky directory (like /tmp) lets others add entries but not rename ours.
        let closed = pm.mode() & 0o022 == 0 || pm.mode() & 0o1000 != 0;
        if !owned || !closed {
            return Err(format!("{} can be changed by other users", parent.display()));
        }
    }
    Ok(meta)
}

/// Remove one direct child of `root`. A symlink is unlinked and never followed.
pub fn remove_entry(root: &RemoveRoot, name: &str, owner_uid: u32) -> Result<Removal, String> {
    if name.is_empty() || name == "." || name == ".." || name.contains('/') || name.contains('\0') {
        return Err(format!("not an entry name: {name:?}"));
    }
    let root_meta = match fs::symlink_metadata(&root.dir) {
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Removal::AlreadyGone),
        _ => check_trusted_dir(&root.dir, owner_uid)?,
    };
    let path = root.dir.join(name);
    let meta = match fs::symlink_metadata(&path) {
        Ok(m) => m,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Removal::AlreadyGone),
        Err(e) => return Err(format!("cannot stat {}: {e}", path.display())),
    };
    if meta.dev() != root_meta.dev() {
        return Err(format!("{} is on another filesystem", path.display()));
    }
    if meta.is_dir() {
        if !root.allow_dirs {
            return Err(format!("{} is a directory", path.display()));
        }
        // std's remove_dir_all never follows symlinks inside the tree.
        fs::remove_dir_all(&path).map_err(|e| format!("cannot remove {}: {e}", path.display()))?;
    } else {
        fs::remove_file(&path).map_err(|e| format!("cannot remove {}: {e}", path.display()))?;
    }
    Ok(Removal::Removed)
}

/// Remove everything inside `dir` and keep `dir`. Returns how many entries went.
/// A missing directory counts as empty. Entries on another filesystem are an error.
pub fn clear_dir(dir: &Path, owner_uid: u32) -> Result<usize, String> {
    let dir_meta = match fs::symlink_metadata(dir) {
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(0),
        _ => check_trusted_dir(dir, owner_uid)?,
    };
    let entries = fs::read_dir(dir).map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
    let mut removed = 0;
    let mut errors = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let result = fs::symlink_metadata(&path).and_then(|meta| {
            if meta.dev() != dir_meta.dev() {
                Err(std::io::Error::other("on another filesystem"))
            } else if meta.is_dir() {
                fs::remove_dir_all(&path)
            } else {
                fs::remove_file(&path)
            }
        });
        match result {
            Ok(()) => removed += 1,
            Err(e) => errors.push(format!("{}: {e}", path.display())),
        }
    }
    if errors.is_empty() {
        Ok(removed)
    } else {
        Err(errors.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};
    use std::sync::atomic::{AtomicU32, Ordering};

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn parse(list: &[&str]) -> Result<Op, String> {
        Op::parse(&args(list))
    }

    /// A fresh private directory under the system temp dir, removed on drop.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new() -> Self {
            static N: AtomicU32 = AtomicU32::new(0);
            let base = std::env::temp_dir().canonicalize().unwrap();
            let dir = base.join(format!(
                "donutcleaner-ops-test-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&dir).unwrap();
            fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();
            Scratch(dir)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn me() -> u32 {
        fs::metadata("/proc/self").unwrap().uid()
    }

    // --- argument parsing ---

    #[test]
    fn fixed_operations_parse_and_roundtrip() {
        for op in [
            Op::PaccacheAll,
            Op::PaccacheUninstalled,
            Op::JournalVacuum,
            Op::RemoveOrphans,
            Op::ClearDir(CleanDir::DebtapCache),
            Op::ClearDir(CleanDir::Coredumps),
            Op::RemovePackages(args(&["firefox", "lib32-glibc", "gtk4"])),
            Op::RemovePaths(args(&["/opt/Foo.AppImage", "/usr/local/bin/tool"])),
        ] {
            assert_eq!(Op::parse(&op.to_args()).unwrap(), op);
        }
    }

    #[test]
    fn rejects_unknown_and_empty_operations() {
        assert!(Op::parse(&[]).is_err());
        assert!(parse(&["rm"]).is_err());
        assert!(parse(&["sh", "-c", "id"]).is_err());
        assert!(parse(&["PACCACHE-ALL"]).is_err());
        assert!(parse(&[""]).is_err());
    }

    #[test]
    fn fixed_operations_take_no_extra_arguments() {
        assert!(parse(&["paccache-all", "-k5"]).is_err());
        assert!(parse(&["journal-vacuum", "--vacuum-time=0"]).is_err());
        assert!(parse(&["remove-orphans", "firefox"]).is_err());
        assert!(parse(&["paccache-uninstalled", "x"]).is_err());
    }

    #[test]
    fn clear_dir_takes_only_known_keys() {
        assert_eq!(
            parse(&["clear-dir", "coredumps"]).unwrap(),
            Op::ClearDir(CleanDir::Coredumps)
        );
        assert!(parse(&["clear-dir"]).is_err());
        assert!(parse(&["clear-dir", "/etc"]).is_err());
        assert!(parse(&["clear-dir", "../etc"]).is_err());
        assert!(parse(&["clear-dir", "coredumps", "debtap-cache"]).is_err());
    }

    #[test]
    fn package_names_are_strict() {
        for good in ["a", "gtk4", "lib32-glibc", "python-3.12_x", "c++utilities", "@scope", "_x"] {
            assert!(check_package_name(good).is_ok(), "{good}");
        }
        for bad in [
            "", "-Rdd", "--noconfirm", ".hidden", "core/pacman", "a b", "a;b", "a$(id)", "a`id`",
            "a\nb", "a\0b", "../x", "x/y", "gt*", "gt?", "ä", "a|b", "a&b", "a>b", "$HOME",
        ] {
            assert!(check_package_name(bad).is_err(), "{bad:?}");
        }
        assert!(check_package_name(&"a".repeat(256)).is_err());
        assert!(check_package_name(&"a".repeat(255)).is_ok());
    }

    #[test]
    fn remove_packages_rejects_bad_lists() {
        assert!(parse(&["remove-packages"]).is_err());
        assert!(parse(&["remove-packages", "ok", "-Rdd"]).is_err());
        assert!(parse(&["remove-packages", "ok", "ok"]).is_err());
        let many: Vec<String> = (0..=MAX_PACKAGES).map(|i| format!("p{i}")).collect();
        let mut v = vec!["remove-packages".to_string()];
        v.extend(many);
        assert!(Op::parse(&v).is_err());
    }

    #[test]
    fn protected_packages_are_flagged() {
        assert!(is_protected_package("pacman"));
        assert!(is_protected_package("linux"));
        assert!(is_protected_package("donutcleaner"));
        assert!(!is_protected_package("firefox"));
    }

    #[test]
    fn removal_paths_must_be_direct_children_of_a_root() {
        let roots = system_remove_roots();
        assert_eq!(split_removal_path("/opt/Foo", &roots).unwrap(), (0, "Foo"));
        assert_eq!(
            split_removal_path("/usr/local/bin/tool", &roots).unwrap(),
            (1, "tool")
        );
        assert_eq!(
            split_removal_path("/usr/local/sbin/tool", &roots).unwrap(),
            (2, "tool")
        );
        for bad in [
            "",
            "opt/Foo",
            "Foo",
            "./Foo",
            "/",
            "/opt",
            "/opt/",
            "/opt/Foo/",
            "/opt//Foo",
            "/opt/./Foo",
            "/opt/../etc/passwd",
            "/opt/Foo/../../etc/passwd",
            "/opt/Foo/bar",
            "/opt/..",
            "/opt/.",
            "/etc/passwd",
            "/usr/bin/ls",
            "/usr/local/bin",
            "/usr/local/share/x",
            "/usr/local/bin/sub/x",
            "/home/user/x",
            "/optional/x",
            "/opt/Foo\0",
            "//opt/Foo",
            "/OPT/Foo",
            "/var/cache/pacman/pkg/x",
            "/usr/local/bin/../../../bin/sh",
        ] {
            assert!(split_removal_path(bad, &roots).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn remove_paths_rejects_duplicates_and_empty_lists() {
        assert!(parse(&["remove-paths"]).is_err());
        assert!(parse(&["remove-paths", "/opt/a", "/opt/a"]).is_err());
        assert!(parse(&["remove-paths", "/opt/a", "/etc/passwd"]).is_err());
        assert!(parse(&["remove-paths", "/opt/a", "/opt/b"]).is_ok());
    }

    // --- filesystem checks, in a private temp directory ---

    fn root_in(scratch: &Scratch, allow_dirs: bool) -> RemoveRoot {
        RemoveRoot {
            dir: scratch.0.clone(),
            allow_dirs,
        }
    }

    #[test]
    fn removes_a_file_and_a_directory_tree() {
        let s = Scratch::new();
        fs::write(s.0.join("app.AppImage"), b"x").unwrap();
        fs::create_dir_all(s.0.join("tree/sub")).unwrap();
        fs::write(s.0.join("tree/sub/f"), b"x").unwrap();
        let root = root_in(&s, true);
        assert_eq!(remove_entry(&root, "app.AppImage", me()), Ok(Removal::Removed));
        assert_eq!(remove_entry(&root, "tree", me()), Ok(Removal::Removed));
        assert_eq!(remove_entry(&root, "tree", me()), Ok(Removal::AlreadyGone));
        assert!(fs::read_dir(&s.0).unwrap().next().is_none());
    }

    #[test]
    fn a_symlink_is_unlinked_not_followed() {
        let s = Scratch::new();
        let outside = Scratch::new();
        fs::write(outside.0.join("precious"), b"keep").unwrap();
        symlink(&outside.0, s.0.join("link")).unwrap();
        let root = root_in(&s, true);
        assert_eq!(remove_entry(&root, "link", me()), Ok(Removal::Removed));
        assert!(outside.0.join("precious").exists());
        assert!(fs::symlink_metadata(s.0.join("link")).is_err());
    }

    #[test]
    fn symlinks_inside_a_removed_tree_are_not_followed() {
        let s = Scratch::new();
        let outside = Scratch::new();
        fs::write(outside.0.join("precious"), b"keep").unwrap();
        fs::create_dir(s.0.join("tree")).unwrap();
        symlink(&outside.0, s.0.join("tree/link")).unwrap();
        let root = root_in(&s, true);
        assert_eq!(remove_entry(&root, "tree", me()), Ok(Removal::Removed));
        assert!(outside.0.join("precious").exists());
    }

    #[test]
    fn directories_are_refused_where_only_files_belong() {
        let s = Scratch::new();
        fs::create_dir(s.0.join("d")).unwrap();
        fs::write(s.0.join("f"), b"x").unwrap();
        let root = root_in(&s, false);
        assert!(remove_entry(&root, "d", me()).is_err());
        assert!(s.0.join("d").exists());
        assert_eq!(remove_entry(&root, "f", me()), Ok(Removal::Removed));
    }

    #[test]
    fn entry_names_cannot_climb_out() {
        let s = Scratch::new();
        let root = root_in(&s, true);
        for bad in ["", ".", "..", "a/b", "../x", "/etc/passwd", "a\0"] {
            assert!(remove_entry(&root, bad, me()).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn a_root_others_can_write_is_refused() {
        let s = Scratch::new();
        fs::write(s.0.join("f"), b"x").unwrap();
        fs::set_permissions(&s.0, fs::Permissions::from_mode(0o777)).unwrap();
        let root = root_in(&s, true);
        assert!(remove_entry(&root, "f", me()).is_err());
        assert!(s.0.join("f").exists());
        fs::set_permissions(&s.0, fs::Permissions::from_mode(0o775)).unwrap();
        assert!(remove_entry(&root, "f", me()).is_err());
    }

    #[test]
    fn a_root_under_a_writable_parent_is_refused() {
        let s = Scratch::new();
        let open = s.0.join("open");
        fs::create_dir(&open).unwrap();
        fs::create_dir(open.join("root")).unwrap();
        fs::write(open.join("root/f"), b"x").unwrap();
        fs::set_permissions(&open, fs::Permissions::from_mode(0o777)).unwrap();
        let root = RemoveRoot {
            dir: open.join("root"),
            allow_dirs: true,
        };
        assert!(remove_entry(&root, "f", me()).is_err());
        assert!(open.join("root/f").exists());
        // Sticky closes the hole: others cannot rename our directory away.
        fs::set_permissions(&open, fs::Permissions::from_mode(0o1777)).unwrap();
        assert_eq!(remove_entry(&root, "f", me()), Ok(Removal::Removed));
    }

    #[test]
    fn a_missing_root_has_nothing_to_remove() {
        let s = Scratch::new();
        let root = RemoveRoot {
            dir: s.0.join("absent"),
            allow_dirs: true,
        };
        assert_eq!(remove_entry(&root, "f", me()), Ok(Removal::AlreadyGone));
    }

    #[test]
    fn a_root_with_the_wrong_owner_is_refused() {
        let s = Scratch::new();
        fs::write(s.0.join("f"), b"x").unwrap();
        let root = root_in(&s, true);
        assert!(remove_entry(&root, "f", me() + 1).is_err());
        assert!(s.0.join("f").exists());
    }

    #[test]
    fn a_root_reached_through_a_symlink_is_refused() {
        let s = Scratch::new();
        fs::create_dir(s.0.join("real")).unwrap();
        fs::write(s.0.join("real/f"), b"x").unwrap();
        symlink(s.0.join("real"), s.0.join("alias")).unwrap();
        let via_link = RemoveRoot {
            dir: s.0.join("alias"),
            allow_dirs: true,
        };
        assert!(remove_entry(&via_link, "f", me()).is_err());
        assert!(s.0.join("real/f").exists());
    }

    #[test]
    fn clear_dir_empties_but_keeps_the_directory() {
        let s = Scratch::new();
        let outside = Scratch::new();
        fs::write(outside.0.join("precious"), b"keep").unwrap();
        fs::write(s.0.join("a"), b"x").unwrap();
        fs::create_dir_all(s.0.join("d/e")).unwrap();
        fs::write(s.0.join("d/e/f"), b"x").unwrap();
        symlink(&outside.0, s.0.join("link")).unwrap();
        assert_eq!(clear_dir(&s.0, me()), Ok(3));
        assert!(s.0.exists());
        assert!(fs::read_dir(&s.0).unwrap().next().is_none());
        assert!(outside.0.join("precious").exists());
    }

    #[test]
    fn clear_dir_treats_a_missing_directory_as_empty() {
        let s = Scratch::new();
        assert_eq!(clear_dir(&s.0.join("nope"), me()), Ok(0));
    }

    #[test]
    fn clear_dir_refuses_a_symlinked_or_foreign_directory() {
        let s = Scratch::new();
        let target = Scratch::new();
        fs::write(target.0.join("f"), b"x").unwrap();
        symlink(&target.0, s.0.join("alias")).unwrap();
        assert!(clear_dir(&s.0.join("alias"), me()).is_err());
        assert!(clear_dir(&target.0, me() + 1).is_err());
        assert!(target.0.join("f").exists());
    }
}
