use gtk::prelude::*;

pub struct ProgressView {
    pub widget: gtk::Box,
    progress_bar: gtk::ProgressBar,
    status_label: gtk::Label,
    log_buffer: gtk::TextBuffer,
    log_view: gtk::TextView,
}

impl ProgressView {
    pub fn new() -> Self {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 8);
        widget.set_margin_top(12);
        widget.set_margin_bottom(12);
        widget.set_margin_start(12);
        widget.set_margin_end(12);

        let status_label = gtk::Label::new(Some("Removing packages..."));
        status_label.add_css_class("title-4");
        status_label.set_halign(gtk::Align::Start);

        let progress_bar = gtk::ProgressBar::new();
        progress_bar.set_show_text(true);

        let log_buffer = gtk::TextBuffer::new(None::<&gtk::TextTagTable>);
        let log_view = gtk::TextView::with_buffer(&log_buffer);
        log_view.set_editable(false);
        log_view.set_monospace(true);
        log_view.set_wrap_mode(gtk::WrapMode::WordChar);

        let scroll = gtk::ScrolledWindow::new();
        scroll.set_child(Some(&log_view));
        scroll.set_vexpand(true);
        scroll.set_min_content_height(200);

        widget.append(&status_label);
        widget.append(&progress_bar);
        widget.append(&scroll);

        ProgressView {
            widget,
            progress_bar,
            status_label,
            log_buffer,
            log_view: log_view,
        }
    }

    pub fn set_progress(&self, fraction: f64, text: &str) {
        self.progress_bar.set_fraction(fraction);
        self.progress_bar.set_text(Some(text));
    }

    pub fn append_log(&self, text: &str) {
        let mut end = self.log_buffer.end_iter();
        self.log_buffer.insert(&mut end, text);
        self.log_buffer.insert(&mut end, "\n");

        // Auto-scroll to bottom
        let mark = self.log_buffer.create_mark(None, &end, false);
        self.log_view.scroll_to_mark(&mark, 0.0, false, 0.0, 0.0);
    }

    pub fn set_complete(&self, success: bool) {
        if success {
            self.status_label.set_label("Removal complete");
            self.progress_bar.set_fraction(1.0);
            self.progress_bar.set_text(Some("Done"));
        } else {
            self.status_label.set_label("Removal finished with errors");
            self.progress_bar.add_css_class("error");
        }
    }
}
