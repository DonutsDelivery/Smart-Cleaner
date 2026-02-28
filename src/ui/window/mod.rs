mod imp;

use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

use crate::model::removal_plan::RemovalPlan;
use crate::scanner::registry::{self, ScanProgress, ScanResult};
use crate::ui::confirmation_dialog;

glib::wrapper! {
    pub struct Window(ObjectSubclass<imp::Window>)
        @extends adw::ApplicationWindow, gtk::ApplicationWindow, gtk::Window, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget,
                    gtk::Native, gtk::Root, gtk::ShortcutManager,
                    gtk::gio::ActionMap, gtk::gio::ActionGroup;
}

impl Window {
    pub fn new(app: &adw::Application) -> Self {
        let window: Self = glib::Object::builder()
            .property("application", app)
            .build();

        window.setup_search();
        window.setup_remove_button();
        window.start_scan();
        window
    }

    fn start_scan(&self) {
        // Show progress bar
        if let Some(ref lb) = *self.imp().loading_box.borrow() {
            lb.set_visible(true);
        }

        let window = self.clone();
        registry::scan_all_progressive(move |progress| {
            window.on_scan_progress(progress);
        });
    }

    fn on_scan_progress(&self, progress: ScanProgress) {
        let imp = self.imp();

        match progress {
            ScanProgress::ScannerStarting(label, step, total) => {
                let fraction = step as f64 / total as f64;
                if let Some(ref bar) = *imp.progress_bar.borrow() {
                    bar.set_fraction(fraction);
                }
                if let Some(ref lbl) = *imp.progress_label.borrow() {
                    lbl.set_label(&format!("Scanning: {}...", label));
                }
                self.set_status(&format!("Scanning: {}...", label));
            }

            ScanProgress::ScannerDone { label, total_so_far, .. } => {
                if let Some(ref lbl) = *imp.progress_label.borrow() {
                    lbl.set_label(&format!("{} — {} packages found", label, total_so_far));
                }
                self.set_status(&format!(
                    "{} done — {} packages found so far",
                    label, total_so_far
                ));
            }

            ScanProgress::Enriching(msg) => {
                if let Some(ref bar) = *imp.progress_bar.borrow() {
                    bar.pulse();
                }
                if let Some(ref lbl) = *imp.progress_label.borrow() {
                    lbl.set_label(&msg);
                }
                self.set_status(&msg);
            }

            ScanProgress::BuildingLayout => {
                if let Some(ref bar) = *imp.progress_bar.borrow() {
                    bar.set_fraction(0.9);
                }
                if let Some(ref lbl) = *imp.progress_label.borrow() {
                    lbl.set_label("Building graph layout...");
                }
                self.set_status("Building graph layout...");
            }

            ScanProgress::Complete(result) => {
                self.on_scan_complete(result);
            }

            ScanProgress::Error(msg) => {
                eprintln!("Scanner error: {msg}");
            }
        }
    }

    fn on_scan_complete(&self, result: ScanResult) {
        let imp = self.imp();

        // Log any errors
        for err in &result.errors {
            eprintln!("Scanner error: {err}");
        }

        // Store final dep tree
        *imp.dep_tree.borrow_mut() = Some(result.dep_tree);
        imp.selected_ids.borrow_mut().clear();

        // Apply final graph layout
        if let Some(ref gv) = *imp.graph_view.borrow() {
            gv.set_layout(result.graph_layout);
        }

        // Hide progress bar
        if let Some(ref lb) = *imp.loading_box.borrow() {
            lb.set_visible(false);
        }

        // Update status
        self.set_status(&format!(
            "{} packages scanned",
            result.packages.len()
        ));
    }

    fn setup_remove_button(&self) {
        let window = self.clone();
        if let Some(ref button) = *self.imp().remove_button.borrow() {
            button.connect_clicked(move |_| {
                window.on_remove_clicked();
            });
        }
    }

    fn setup_search(&self) {
        let window = self.clone();

        if let Some(ref entry) = *self.imp().search_entry.borrow() {
            entry.connect_search_changed(move |entry| {
                let query = entry.text().to_string();
                let wimp = window.imp();
                let tree_ref = wimp.dep_tree.borrow();
                if let Some(ref tree) = *tree_ref {
                    if let Some(ref gv) = *wimp.graph_view.borrow() {
                        gv.set_search(&query, tree);
                    }
                }
            });
        }
    }

    fn on_remove_clicked(&self) {
        let imp = self.imp();
        let selected = imp.selected_ids.borrow().clone();
        if selected.is_empty() {
            return;
        }

        // Build removal plan
        let tree_ref = imp.dep_tree.borrow();
        let packages: Vec<_> = if let Some(ref tree) = *tree_ref {
            selected
                .iter()
                .filter_map(|id| tree.get(id).cloned())
                .collect()
        } else {
            return;
        };
        drop(tree_ref);

        let plan = RemovalPlan::from_packages(packages);
        let commands = plan.commands();

        let window = self.clone();
        confirmation_dialog::show_confirmation(self, &plan, move || {
            window.execute_removal(commands);
        });
    }

    fn execute_removal(&self, commands: Vec<crate::model::removal_plan::RemovalCommand>) {
        let window = self.clone();
        crate::remover::executor::execute_removal(commands, move |success, output| {
            for line in &output {
                eprintln!("{line}");
            }
            if success {
                window.set_status("Removal complete. Re-scanning...");
                window.start_scan();
            } else {
                window.set_status("Removal finished with errors");
            }
        });
    }

    fn update_status_bar(&self) {
        let imp = self.imp();
        let selected = imp.selected_ids.borrow();
        let count = selected.len();

        if let Some(ref button) = *imp.remove_button.borrow() {
            button.set_label(&format!("Remove ({count})"));
            button.set_sensitive(count > 0);
        }

        if count > 0 {
            let tree_ref = imp.dep_tree.borrow();
            let total_size = if let Some(ref tree) = *tree_ref {
                tree.total_size(&selected)
            } else {
                0
            };
            drop(tree_ref);

            self.set_status(&format!(
                "Selected: {} packages | Total: {} freed",
                count,
                bytesize::ByteSize(total_size)
            ));
        } else {
            self.set_status("Ready");
        }

        // Redraw graph
        if let Some(ref gv) = *imp.graph_view.borrow() {
            gv.queue_draw();
        }
    }

    fn set_status(&self, text: &str) {
        if let Some(ref label) = *self.imp().status_label.borrow() {
            label.set_label(text);
        }
    }
}
