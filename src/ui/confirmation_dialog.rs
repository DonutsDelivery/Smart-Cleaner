use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;

use crate::model::removal_plan::RemovalPlan;

pub fn show_confirmation(
    parent: &impl IsA<gtk::Widget>,
    plan: &RemovalPlan,
    on_confirm: impl FnOnce() + 'static,
) {
    let dialog = adw::AlertDialog::new(
        Some(&format!(
            "Remove {} package(s)?",
            plan.total_packages()
        )),
        Some(&format!(
            "This will free approximately {}.",
            bytesize::ByteSize(plan.total_size)
        )),
    );

    // Build details text
    let details = plan.summary_lines().join("\n");

    // Add a scrolled text view with the details
    let text_view = gtk::TextView::new();
    text_view.set_editable(false);
    text_view.set_monospace(true);
    text_view.set_wrap_mode(gtk::WrapMode::WordChar);
    text_view.buffer().set_text(&details);
    text_view.add_css_class("card");

    let scroll = gtk::ScrolledWindow::new();
    scroll.set_child(Some(&text_view));
    scroll.set_min_content_height(200);
    scroll.set_min_content_width(400);

    dialog.set_extra_child(Some(&scroll));

    dialog.add_response("cancel", "Cancel");
    dialog.add_response("remove", "Remove");
    dialog.set_response_appearance("remove", adw::ResponseAppearance::Destructive);
    dialog.set_default_response(Some("cancel"));
    dialog.set_close_response("cancel");

    // Wrap FnOnce in Rc<RefCell<Option<>>> so it can be used in Fn closure
    let on_confirm = Rc::new(RefCell::new(Some(on_confirm)));
    dialog.connect_response(None, move |_, response| {
        if response == "remove" {
            if let Some(cb) = on_confirm.borrow_mut().take() {
                cb();
            }
        }
    });

    dialog.present(Some(parent));
}
