use std::collections::HashMap;
use std::sync::mpsc;

use gtk::gio;
use gtk::glib;

use crate::model::dep_tree::DependencyTree;
use crate::model::package::PackageInfo;
use crate::model::package_source::PackageSource;

use super::appimage::AppImageScanner;
use super::desktop_entries;
use super::desktop_scanner::DesktopEntryScanner;
use super::flatpak::FlatpakScanner;
use super::pacman::PacmanScanner;
use super::PackageScanner;

pub struct ScanResult {
    pub packages: Vec<PackageInfo>,
    pub dep_tree: DependencyTree,
    pub source_counts: HashMap<PackageSource, usize>,
    pub errors: Vec<String>,
}

/// Progress update sent from the scanner thread to the main thread.
pub enum ScanProgress {
    /// A scanner is starting. (scanner_label, step_index, total_steps)
    ScannerStarting(String, usize, usize),
    /// A scanner finished and produced packages.
    ScannerDone {
        label: String,
        total_so_far: usize,
    },
    /// Enrichment / desktop scanner phase.
    Enriching(String),
    /// All done — final result.
    Complete(ScanResult),
    /// A scanner produced an error.
    Error(String),
}

/// Run all available scanners, sending incremental progress to the main thread.
pub fn scan_all_progressive(on_progress: impl Fn(ScanProgress) + 'static) {
    let (tx, rx) = mpsc::channel::<ScanProgress>();

    // Poll the channel from the main thread at ~60fps
    glib::timeout_add_local(std::time::Duration::from_millis(16), move || {
        while let Ok(msg) = rx.try_recv() {
            let is_complete = matches!(msg, ScanProgress::Complete(_));
            on_progress(msg);
            if is_complete {
                return glib::ControlFlow::Break;
            }
        }
        glib::ControlFlow::Continue
    });

    gio::spawn_blocking(move || {
        let scanners: Vec<Box<dyn PackageScanner>> = vec![
            Box::new(PacmanScanner),
            Box::new(FlatpakScanner),
            Box::new(AppImageScanner),
        ];

        let total_steps = scanners.len() + 2; // +enrichment +desktop_scanner
        let mut all_packages: Vec<PackageInfo> = Vec::new();
        let mut errors: Vec<String> = Vec::new();
        let mut step = 0;

        for scanner in &scanners {
            if !scanner.is_available() {
                step += 1;
                continue;
            }

            let _ = tx.send(ScanProgress::ScannerStarting(
                scanner.label().to_string(),
                step,
                total_steps,
            ));

            match scanner.scan_blocking() {
                Ok(mut pkgs) => {
                    all_packages.append(&mut pkgs);
                    let _ = tx.send(ScanProgress::ScannerDone {
                        label: scanner.label().to_string(),
                        total_so_far: all_packages.len(),
                    });
                }
                Err(e) => {
                    let msg = e.to_string();
                    errors.push(msg.clone());
                    let _ = tx.send(ScanProgress::Error(msg));
                }
            }
            step += 1;
        }

        // Enrichment
        let _ = tx.send(ScanProgress::Enriching("Enriching with desktop metadata...".into()));
        desktop_entries::enrich_packages(&mut all_packages);
        step += 1;

        // Desktop entry scanner
        let _ = tx.send(ScanProgress::ScannerStarting(
            "Desktop entries".into(),
            step,
            total_steps,
        ));
        let desktop_scanner = DesktopEntryScanner::new(&all_packages);
        if desktop_scanner.is_available() {
            match desktop_scanner.scan_blocking() {
                Ok(mut pkgs) => {
                    all_packages.append(&mut pkgs);
                    let _ = tx.send(ScanProgress::ScannerDone {
                        label: "Desktop entries".into(),
                        total_so_far: all_packages.len(),
                    });
                }
                Err(e) => {
                    let msg = e.to_string();
                    errors.push(msg.clone());
                    let _ = tx.send(ScanProgress::Error(msg));
                }
            }
        }
        step += 1;

        let mut source_counts: HashMap<PackageSource, usize> = HashMap::new();
        for pkg in &all_packages {
            *source_counts.entry(pkg.source).or_default() += 1;
        }

        let mut dep_tree = DependencyTree::build(all_packages.clone());

        // Mark critical system packages as protected from removal
        let protected = crate::scanner::pacman::get_protected_package_names();
        dep_tree.mark_protected(&protected);

        let _ = tx.send(ScanProgress::Complete(ScanResult {
            packages: all_packages,
            dep_tree,
            source_counts,
            errors,
        }));
    });
}
