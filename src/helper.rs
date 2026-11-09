//! `donutcleaner-helper`: the only program DonutCleaner runs as root.
//!
//! The app starts it as `pkexec /usr/lib/donutcleaner/donutcleaner-helper <op> [args]`.
//! The operation and its arguments are parsed and checked by `donutcleaner_ops`
//! before anything runs. The helper never starts a shell, never takes a program
//! name or path to execute from its arguments, and starts its tools by absolute path
//! with a fresh environment.

use std::io::Write;
use std::path::Path;
use std::process::{Command, ExitCode, Stdio};

use donutcleaner_ops::{
    Op, Removal, RemoveRoot, check_package_name, clear_dir, is_protected_package, remove_entry,
    split_removal_path, system_remove_roots,
};

const PACMAN: &str = "/usr/bin/pacman";
const PACCACHE: &str = "/usr/bin/paccache";
const JOURNALCTL: &str = "/usr/bin/journalctl";
/// Files and directories under the allowlisted roots must belong to root.
const ROOT_UID: u32 = 0;

/// Exit status for arguments the helper refused. Failures of an accepted operation exit 1.
const EXIT_REFUSED: u8 = 2;

fn main() -> ExitCode {
    let mut args = Vec::new();
    for arg in std::env::args_os().skip(1) {
        match arg.into_string() {
            Ok(a) => args.push(a),
            Err(_) => return refuse("arguments must be valid UTF-8"),
        }
    }
    let op = match Op::parse(&args) {
        Ok(op) => op,
        Err(e) => return refuse(&e),
    };
    match run(&op) {
        Ok(()) => ExitCode::SUCCESS,
        Err(Failure::Refused(e)) => refuse(&e),
        Err(Failure::Failed(e)) => {
            eprintln!("donutcleaner-helper: {e}");
            ExitCode::FAILURE
        }
    }
}

fn refuse(reason: &str) -> ExitCode {
    eprintln!("donutcleaner-helper: refused: {reason}");
    ExitCode::from(EXIT_REFUSED)
}

enum Failure {
    /// The request itself is not allowed.
    Refused(String),
    /// The request was allowed but did not work.
    Failed(String),
}

fn run(op: &Op) -> Result<(), Failure> {
    match op {
        Op::PaccacheAll => run_tool(PACCACHE, &["-rk0"]),
        Op::PaccacheUninstalled => run_tool(PACCACHE, &["-ruk0"]),
        Op::JournalVacuum => run_tool(JOURNALCTL, &["--vacuum-time=1week"]),
        Op::ClearDir(dir) => {
            let removed = clear_dir(Path::new(dir.path()), ROOT_UID).map_err(Failure::Failed)?;
            println!("cleared {} ({removed} entries)", dir.path());
            Ok(())
        }
        Op::RemoveOrphans => remove_orphans(),
        Op::RemovePackages(names) => remove_packages(names),
        Op::RemovePaths(paths) => remove_paths(paths),
    }
}

/// A command with an empty environment, so nothing inherited from the caller can steer it.
fn tool(program: &str) -> Command {
    let mut cmd = Command::new(program);
    cmd.env_clear()
        .env("PATH", "/usr/bin:/usr/local/bin")
        .env("LC_ALL", "C.UTF-8")
        .stdin(Stdio::null());
    cmd
}

fn run_tool(program: &str, args: &[&str]) -> Result<(), Failure> {
    let status = tool(program)
        .args(args)
        .status()
        .map_err(|e| Failure::Failed(format!("cannot run {program}: {e}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(Failure::Failed(format!("{program} exited with {status}")))
    }
}

fn pacman_remove(names: &[String]) -> Result<(), Failure> {
    let status = tool(PACMAN)
        .args(["-Rns", "--noconfirm", "--"])
        .args(names)
        .status()
        .map_err(|e| Failure::Failed(format!("cannot run {PACMAN}: {e}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(Failure::Failed(format!("pacman exited with {status}")))
    }
}

fn remove_packages(names: &[String]) -> Result<(), Failure> {
    if let Some(name) = names.iter().find(|n| is_protected_package(n)) {
        return Err(Failure::Refused(format!("{name} is a protected package")));
    }
    // Every name must be an installed package. Without this check, pacman would
    // read a group name such as `gnome` as "remove every member of the group".
    let installed = tool(PACMAN)
        .args(["-Qq", "--"])
        .args(names)
        .stdout(Stdio::null())
        .status()
        .map_err(|e| Failure::Failed(format!("cannot run {PACMAN}: {e}")))?;
    if !installed.success() {
        return Err(Failure::Refused(
            "not every name is an installed package".to_string(),
        ));
    }
    pacman_remove(names)
}

fn remove_orphans() -> Result<(), Failure> {
    let output = tool(PACMAN)
        .arg("-Qdtq")
        .output()
        .map_err(|e| Failure::Failed(format!("cannot run {PACMAN}: {e}")))?;
    let mut names = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let name = line.trim();
        if name.is_empty() {
            continue;
        }
        if check_package_name(name).is_err() || is_protected_package(name) {
            println!("skipping {name}");
        } else {
            names.push(name.to_string());
        }
    }
    if names.is_empty() {
        println!("no orphaned packages");
        return Ok(());
    }
    pacman_remove(&names)
}

/// True when pacman says some package owns this path. Without pacman nothing is owned.
/// A pacman failure that is not "no package owns it" stops the removal instead of
/// counting as "not owned".
fn owned_by_a_package(path: &str) -> Result<bool, String> {
    match tool(PACMAN).args(["-Qo", "--", path]).output() {
        Ok(out) if out.status.success() => Ok(true),
        Ok(out) if String::from_utf8_lossy(&out.stderr).contains("No package owns") => Ok(false),
        Ok(out) => Err(format!(
            "cannot tell whether a package owns {path}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(format!("cannot run {PACMAN}: {e}")),
    }
}

fn remove_paths(paths: &[String]) -> Result<(), Failure> {
    let roots: Vec<RemoveRoot> = system_remove_roots();
    // Check every path first, so a refusal leaves everything in place.
    let mut targets = Vec::new();
    for path in paths {
        let (root, name) = split_removal_path(path, &roots).map_err(Failure::Refused)?;
        if owned_by_a_package(path).map_err(Failure::Failed)? {
            return Err(Failure::Refused(format!(
                "{path} belongs to an installed package, remove the package instead"
            )));
        }
        targets.push((path, &roots[root], name));
    }
    let mut failed = false;
    for (path, root, name) in targets {
        match remove_entry(root, name, ROOT_UID) {
            Ok(Removal::Removed) => println!("removed {path}"),
            Ok(Removal::AlreadyGone) => println!("already gone {path}"),
            Err(e) => {
                eprintln!("donutcleaner-helper: {e}");
                failed = true;
            }
        }
    }
    let _ = std::io::stdout().flush();
    if failed {
        Err(Failure::Failed("some paths could not be removed".to_string()))
    } else {
        Ok(())
    }
}
