use gtk::prelude::*;

use crate::model::dep_tree::DependencyTree;
use crate::model::package::PackageInfo;

/// Create a popover displaying full package details, anchored to the drawing area.
pub fn create_popover(
    parent: &gtk::DrawingArea,
    pkg: &PackageInfo,
    tree: &DependencyTree,
    x: f64,
    y: f64,
) -> gtk::Popover {
    let popover = gtk::Popover::new();
    popover.set_parent(parent);
    popover.set_autohide(true);
    popover.set_pointing_to(Some(&gtk::gdk::Rectangle::new(
        x as i32,
        y as i32,
        1,
        1,
    )));

    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 4);
    vbox.set_margin_start(8);
    vbox.set_margin_end(8);
    vbox.set_margin_top(6);
    vbox.set_margin_bottom(6);

    // Name + version
    let display = if pkg.display_name.is_empty() {
        &pkg.name
    } else {
        &pkg.display_name
    };
    let title = gtk::Label::new(Some(&format!("{display} {}", pkg.version)));
    title.add_css_class("heading");
    title.set_halign(gtk::Align::Start);
    title.set_wrap(true);
    vbox.append(&title);

    // Technical name if different from display
    if !pkg.display_name.is_empty() && pkg.display_name != pkg.name {
        let tech = gtk::Label::new(Some(&pkg.name));
        tech.add_css_class("dim-label");
        tech.set_halign(gtk::Align::Start);
        vbox.append(&tech);
    }

    // Description
    if !pkg.description.is_empty() {
        let desc = gtk::Label::new(Some(&pkg.description));
        desc.set_halign(gtk::Align::Start);
        desc.set_wrap(true);
        desc.set_max_width_chars(50);
        vbox.append(&desc);
    }

    // Separator
    vbox.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

    // Source + size
    let source_size = format!(
        "Source: {} | Size: {}",
        pkg.source.label(),
        bytesize::ByteSize(pkg.installed_size)
    );
    let ss_label = gtk::Label::new(Some(&source_size));
    ss_label.set_halign(gtk::Align::Start);
    ss_label.add_css_class("dim-label");
    vbox.append(&ss_label);

    // Categories
    if !pkg.categories.is_empty() {
        let cats = format!("Categories: {}", pkg.categories.join(", "));
        let cats_label = gtk::Label::new(Some(&cats));
        cats_label.set_halign(gtk::Align::Start);
        cats_label.set_wrap(true);
        cats_label.add_css_class("dim-label");
        vbox.append(&cats_label);
    }

    // Depends on
    let pkg_id = pkg.qualified_id();
    let deps = tree.children_of(&pkg_id);
    if !deps.is_empty() {
        let names: Vec<&str> = deps.iter().take(10).map(|d| d.name.as_str()).collect();
        let mut dep_text = format!("Depends on: {}", names.join(", "));
        if deps.len() > 10 {
            dep_text.push_str(&format!(" (+{} more)", deps.len() - 10));
        }
        let dep_label = gtk::Label::new(Some(&dep_text));
        dep_label.set_halign(gtk::Align::Start);
        dep_label.set_wrap(true);
        dep_label.set_max_width_chars(50);
        vbox.append(&dep_label);
    }

    // Required by
    let parents = tree.parents_of(&pkg_id);
    if !parents.is_empty() {
        let names: Vec<&str> = parents.iter().take(10).map(|p| p.name.as_str()).collect();
        let mut req_text = format!("Required by: {}", names.join(", "));
        if parents.len() > 10 {
            req_text.push_str(&format!(" (+{} more)", parents.len() - 10));
        }
        let req_label = gtk::Label::new(Some(&req_text));
        req_label.set_halign(gtk::Align::Start);
        req_label.set_wrap(true);
        req_label.set_max_width_chars(50);
        vbox.append(&req_label);
    }

    popover.set_child(Some(&vbox));
    popover
}
