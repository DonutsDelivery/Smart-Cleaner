use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk::gio;
use gtk::glib;

use crate::scanner::maintenance::{self, MaintenanceItem};

pub struct MaintenanceView {
    pub widget: gtk::Box,
    items: Rc<RefCell<Vec<MaintenanceItem>>>,
    list_box: gtk::Box,
    status_label: gtk::Label,
    clean_button: gtk::Button,
    spinner: gtk::Spinner,
}

impl MaintenanceView {
    pub fn new() -> Self {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 0);
        widget.set_vexpand(true);

        // Header
        let header_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        header_box.set_margin_start(16);
        header_box.set_margin_end(16);
        header_box.set_margin_top(12);
        header_box.set_margin_bottom(8);

        let title = gtk::Label::new(Some("System Maintenance"));
        title.add_css_class("title-2");
        title.set_halign(gtk::Align::Start);
        title.set_hexpand(true);
        header_box.append(&title);

        let scan_button = gtk::Button::with_label("Scan");
        scan_button.add_css_class("suggested-action");
        header_box.append(&scan_button);

        let clean_button = gtk::Button::with_label("Clean Selected");
        clean_button.add_css_class("destructive-action");
        clean_button.set_sensitive(false);
        header_box.append(&clean_button);

        widget.append(&header_box);
        widget.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

        // Spinner for scanning
        let spinner = gtk::Spinner::new();
        spinner.set_visible(false);
        spinner.set_halign(gtk::Align::Center);
        spinner.set_valign(gtk::Align::Center);
        spinner.set_width_request(32);
        spinner.set_height_request(32);

        // Scrollable content
        let scroll = gtk::ScrolledWindow::new();
        scroll.set_vexpand(true);

        let list_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        list_box.set_margin_start(16);
        list_box.set_margin_end(16);
        list_box.set_margin_top(8);
        list_box.set_margin_bottom(8);

        // Initial placeholder
        let placeholder = gtk::Label::new(Some("Click \"Scan\" to discover reclaimable disk space."));
        placeholder.add_css_class("dim-label");
        placeholder.set_valign(gtk::Align::Center);
        placeholder.set_vexpand(true);
        list_box.append(&placeholder);

        scroll.set_child(Some(&list_box));

        let stack = gtk::Stack::new();
        stack.add_named(&scroll, Some("content"));
        stack.add_named(&spinner, Some("scanning"));
        stack.set_visible_child_name("content");
        widget.append(&stack);

        // Bottom bar
        let bottom = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        bottom.set_margin_start(16);
        bottom.set_margin_end(16);
        bottom.set_margin_top(6);
        bottom.set_margin_bottom(6);
        let status_label = gtk::Label::new(Some("Ready"));
        status_label.set_halign(gtk::Align::Start);
        status_label.set_hexpand(true);
        bottom.append(&status_label);

        widget.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        widget.append(&bottom);

        let items: Rc<RefCell<Vec<MaintenanceItem>>> = Rc::new(RefCell::new(Vec::new()));

        // Wire scan button
        {
            let items = items.clone();
            let list_box = list_box.clone();
            let status_label = status_label.clone();
            let clean_button = clean_button.clone();
            let stack = stack.clone();
            let spinner = spinner.clone();

            scan_button.connect_clicked(move |btn| {
                btn.set_sensitive(false);
                spinner.set_spinning(true);
                stack.set_visible_child_name("scanning");
                status_label.set_label("Scanning...");

                let items = items.clone();
                let list_box = list_box.clone();
                let status_label = status_label.clone();
                let clean_button = clean_button.clone();
                let stack = stack.clone();
                let spinner = spinner.clone();
                let btn = btn.clone();

                glib::spawn_future_local(async move {
                    let result = gio::spawn_blocking(maintenance::scan_maintenance_items)
                        .await
                        .expect("Maintenance scan panicked");

                    spinner.set_spinning(false);
                    stack.set_visible_child_name("content");
                    btn.set_sensitive(true);

                    populate_list(&list_box, &result, &items, &clean_button, &status_label);

                    let total: u64 = result.iter().map(|i| i.size).sum();
                    status_label.set_label(&format!(
                        "{} items found | {} reclaimable",
                        result.len(),
                        bytesize::ByteSize(total)
                    ));

                    *items.borrow_mut() = result;
                });
            });
        }

        // Wire clean button
        {
            let items = items.clone();
            let status_label = status_label.clone();

            clean_button.connect_clicked(move |btn| {
                let checked: Vec<MaintenanceItem> = items
                    .borrow()
                    .iter()
                    .filter(|i| i.checked)
                    .cloned()
                    .collect();

                if checked.is_empty() {
                    return;
                }

                btn.set_sensitive(false);
                let status = status_label.clone();
                let btn = btn.clone();

                status.set_label(&format!("Cleaning {} items...", checked.len()));

                glib::spawn_future_local(async move {
                    let total = checked.len();
                    let results = gio::spawn_blocking(move || {
                        // Split into root (pkexec) and user commands
                        let mut root_cmds: Vec<(String, String)> = Vec::new(); // (label, cmd without pkexec)
                        let mut user_cmds: Vec<(String, String)> = Vec::new(); // (label, cmd)
                        for item in &checked {
                            if item.command.is_empty() {
                                continue;
                            }
                            if item.command.starts_with("pkexec ") {
                                root_cmds.push((
                                    item.label.clone(),
                                    item.command["pkexec ".len()..].to_string(),
                                ));
                            } else {
                                user_cmds.push((item.label.clone(), item.command.clone()));
                            }
                        }

                        let mut outputs: Vec<(String, bool, String)> = Vec::new();

                        // Run each user command individually for proper error tracking
                        for (label, cmd) in &user_cmds {
                            eprintln!("CLEAN [user]: {label} => {cmd}");
                            let result = std::process::Command::new("sh")
                                .args(["-c", cmd])
                                .output();
                            match result {
                                Ok(output) => {
                                    let success = output.status.success();
                                    let stdout = String::from_utf8_lossy(&output.stdout);
                                    let stderr = String::from_utf8_lossy(&output.stderr);
                                    eprintln!("  exit={} stdout={} stderr={}", output.status, stdout.chars().take(200).collect::<String>(), stderr.chars().take(200).collect::<String>());
                                    outputs.push((label.clone(), success, stderr.to_string()));
                                }
                                Err(e) => {
                                    eprintln!("  spawn error: {e}");
                                    outputs.push((label.clone(), false, e.to_string()));
                                }
                            }
                        }

                        // Run all root commands under a single pkexec (one auth prompt)
                        // Each command tracked with || markers so we know which failed
                        if !root_cmds.is_empty() {
                            let script = root_cmds
                                .iter()
                                .enumerate()
                                .map(|(i, (_, cmd))| format!("{cmd} && echo __OK_{i}__", i = i, cmd = cmd))
                                .collect::<Vec<_>>()
                                .join("\n");
                            eprintln!("CLEAN [root] script:\n{script}");
                            let result = std::process::Command::new("pkexec")
                                .args(["sh", "-c", &script])
                                .output();
                            match result {
                                Ok(output) => {
                                    let code = output.status.code().unwrap_or(-1);
                                    let stdout = String::from_utf8_lossy(&output.stdout);
                                    let stderr = String::from_utf8_lossy(&output.stderr);
                                    eprintln!("CLEAN [root] exit={code} stdout={stdout} stderr={stderr}");
                                    if code == 126 || code == 127 {
                                        for (label, _) in &root_cmds {
                                            outputs.push((label.clone(), false, "Auth cancelled".into()));
                                        }
                                    } else {
                                        let stdout = String::from_utf8_lossy(&output.stdout);
                                        let stderr = String::from_utf8_lossy(&output.stderr);
                                        for (i, (label, _)) in root_cmds.iter().enumerate() {
                                            let marker = format!("__OK_{i}__");
                                            let success = stdout.contains(&marker);
                                            let err = if success { String::new() } else { stderr.to_string() };
                                            outputs.push((label.clone(), success, err));
                                        }
                                    }
                                }
                                Err(e) => {
                                    for (label, _) in &root_cmds {
                                        outputs.push((label.clone(), false, e.to_string()));
                                    }
                                }
                            }
                        }

                        outputs
                    })
                    .await
                    .expect("Clean thread panicked");

                    let success_count = results.iter().filter(|(_, s, _)| *s).count();
                    let fail_count = results.len() - success_count;

                    for (label, success, err) in &results {
                        if !success {
                            eprintln!("Clean failed: {label}: {err}");
                        }
                    }

                    let msg = if fail_count > 0 {
                        format!("Done: {success_count}/{total} succeeded, {fail_count} failed")
                    } else {
                        format!("Done: {total} items cleaned")
                    };
                    status.set_label(&msg);
                    btn.set_sensitive(true);
                });
            });
        }

        Self {
            widget,
            items,
            list_box,
            status_label,
            clean_button,
            spinner,
        }
    }
}

fn populate_list(
    list_box: &gtk::Box,
    items: &[MaintenanceItem],
    items_state: &Rc<RefCell<Vec<MaintenanceItem>>>,
    clean_button: &gtk::Button,
    _status_label: &gtk::Label,
) {
    // Clear existing children
    while let Some(child) = list_box.first_child() {
        list_box.remove(&child);
    }

    if items.is_empty() {
        let empty = gtk::Label::new(Some("No reclaimable items found."));
        empty.add_css_class("dim-label");
        empty.set_valign(gtk::Align::Center);
        empty.set_vexpand(true);
        list_box.append(&empty);
        return;
    }

    let mut current_category = "";

    for (idx, item) in items.iter().enumerate() {
        // Category header
        if item.category != current_category {
            current_category = item.category;
            if idx > 0 {
                list_box.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
            }
            let cat_label = gtk::Label::new(Some(current_category));
            cat_label.add_css_class("heading");
            cat_label.set_halign(gtk::Align::Start);
            cat_label.set_margin_top(12);
            cat_label.set_margin_bottom(4);
            list_box.append(&cat_label);
        }

        // Item row
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        row.set_margin_top(2);
        row.set_margin_bottom(2);

        let check = gtk::CheckButton::new();
        row.append(&check);

        let label = gtk::Label::new(Some(&item.label));
        label.set_halign(gtk::Align::Start);
        label.set_hexpand(true);
        row.append(&label);

        let size_label = gtk::Label::new(Some(&format!("{}", bytesize::ByteSize(item.size))));
        size_label.add_css_class("numeric");
        size_label.set_halign(gtk::Align::End);
        row.append(&size_label);

        // Wire checkbox
        let items_state = items_state.clone();
        let clean_button = clean_button.clone();
        check.connect_toggled(move |cb| {
            let mut items = items_state.borrow_mut();
            if let Some(item) = items.get_mut(idx) {
                item.checked = cb.is_active();
            }
            let checked_count = items.iter().filter(|i| i.checked).count();
            let checked_size: u64 = items.iter().filter(|i| i.checked).map(|i| i.size).sum();
            clean_button.set_sensitive(checked_count > 0);
            if checked_count > 0 {
                clean_button.set_label(&format!(
                    "Clean Selected ({})",
                    bytesize::ByteSize(checked_size)
                ));
            } else {
                clean_button.set_label("Clean Selected");
            }
        });

        list_box.append(&row);
    }
}
