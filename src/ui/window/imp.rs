use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use adw::prelude::*;
use gtk::glib;
use gtk::subclass::prelude::*;

use crate::model::dep_tree::DependencyTree;
use crate::model::package_source::PackageSource;
use crate::ui::graph_view::GraphView;
use crate::ui::maintenance_view::MaintenanceView;

pub struct Window {
    pub dep_tree: Rc<RefCell<Option<DependencyTree>>>,
    pub selected_ids: Rc<RefCell<HashSet<String>>>,
    pub enabled_sources: Rc<RefCell<HashSet<PackageSource>>>,

    // UI widgets
    pub main_stack: RefCell<Option<gtk::Stack>>,
    pub view_stack: RefCell<Option<gtk::Stack>>,
    pub graph_view: RefCell<Option<GraphView>>,
    pub maintenance_view: RefCell<Option<MaintenanceView>>,
    pub status_label: RefCell<Option<gtk::Label>>,
    pub remove_button: RefCell<Option<gtk::Button>>,
    pub search_entry: RefCell<Option<gtk::SearchEntry>>,
    pub progress_bar: RefCell<Option<gtk::ProgressBar>>,
    pub progress_label: RefCell<Option<gtk::Label>>,
    pub loading_box: RefCell<Option<gtk::Box>>,
}

impl Default for Window {
    fn default() -> Self {
        let mut all_sources = HashSet::new();
        all_sources.insert(PackageSource::Pacman);
        all_sources.insert(PackageSource::Aur);
        all_sources.insert(PackageSource::Flatpak);
        all_sources.insert(PackageSource::Snap);
        all_sources.insert(PackageSource::AppImage);
        all_sources.insert(PackageSource::Wine);
        all_sources.insert(PackageSource::Pip);
        all_sources.insert(PackageSource::Npm);
        all_sources.insert(PackageSource::Cargo);
        all_sources.insert(PackageSource::Desktop);

        Self {
            dep_tree: Rc::new(RefCell::new(None)),
            selected_ids: Rc::new(RefCell::new(HashSet::new())),
            enabled_sources: Rc::new(RefCell::new(all_sources)),
            main_stack: RefCell::new(None),
            view_stack: RefCell::new(None),
            graph_view: RefCell::new(None),
            maintenance_view: RefCell::new(None),
            status_label: RefCell::new(None),
            remove_button: RefCell::new(None),
            search_entry: RefCell::new(None),
            progress_bar: RefCell::new(None),
            progress_label: RefCell::new(None),
            loading_box: RefCell::new(None),
        }
    }
}

#[glib::object_subclass]
impl ObjectSubclass for Window {
    const NAME: &'static str = "SmartCleanerWindow";
    type Type = super::Window;
    type ParentType = adw::ApplicationWindow;
}

impl ObjectImpl for Window {
    fn constructed(&self) {
        self.parent_constructed();
        let window = self.obj();

        window.set_default_size(1200, 800);
        window.set_title(Some(crate::config::APP_NAME));

        // Header bar
        let header = adw::HeaderBar::new();

        // View switcher buttons (Packages / Maintenance)
        let view_toggle = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        view_toggle.add_css_class("linked");

        let graph_btn = gtk::ToggleButton::with_label("Packages");
        graph_btn.set_active(true);
        let maint_btn = gtk::ToggleButton::with_label("Maintenance");
        maint_btn.set_group(Some(&graph_btn));
        view_toggle.append(&graph_btn);
        view_toggle.append(&maint_btn);
        header.pack_start(&view_toggle);

        // Search entry
        let search_entry = gtk::SearchEntry::new();
        search_entry.set_placeholder_text(Some("Search packages..."));
        search_entry.set_hexpand(true);
        header.set_title_widget(Some(&search_entry));

        // Remove button
        let remove_button = gtk::Button::with_label("Remove (0)");
        remove_button.add_css_class("destructive-action");
        remove_button.set_sensitive(false);
        header.pack_end(&remove_button);

        // Content: view stack (graph vs maintenance)
        let view_stack = gtk::Stack::new();
        view_stack.set_transition_type(gtk::StackTransitionType::SlideLeftRight);

        // Graph view
        let dep_tree = self.dep_tree.clone();
        let selected_ids = self.selected_ids.clone();
        let window_for_sel = window.clone();
        let graph_view = GraphView::new(dep_tree, selected_ids, move || {
            window_for_sel.update_status_bar();
        });

        // Maintenance view
        let maintenance_view = MaintenanceView::new();

        view_stack.add_named(&graph_view.widget, Some("graph"));
        view_stack.add_named(&maintenance_view.widget, Some("maintenance"));
        view_stack.set_visible_child_name("graph");

        // Loading overlay — progress bar + label at top of content
        let loading_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
        loading_box.set_margin_start(12);
        loading_box.set_margin_end(12);
        loading_box.set_margin_top(4);
        loading_box.set_margin_bottom(4);

        let progress_label = gtk::Label::new(Some("Starting scan..."));
        progress_label.set_halign(gtk::Align::Start);
        progress_label.add_css_class("dim-label");
        loading_box.append(&progress_label);

        let progress_bar = gtk::ProgressBar::new();
        progress_bar.set_show_text(false);
        loading_box.append(&progress_bar);

        // Bottom bar
        let bottom_bar = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        bottom_bar.set_margin_start(12);
        bottom_bar.set_margin_end(12);
        bottom_bar.set_margin_top(6);
        bottom_bar.set_margin_bottom(6);
        let status_label = gtk::Label::new(Some("Scanning..."));
        status_label.set_halign(gtk::Align::Start);
        status_label.set_hexpand(true);
        bottom_bar.append(&status_label);

        // Assemble: header, progress bar, content, bottom bar
        let content_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        content_box.set_vexpand(true);
        content_box.append(&loading_box);
        content_box.append(&view_stack);
        content_box.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        content_box.append(&bottom_bar);

        // Main stack (we keep it in case we want loading-only mode later, but show content right away)
        let main_stack = gtk::Stack::new();
        main_stack.set_transition_type(gtk::StackTransitionType::Crossfade);
        main_stack.add_named(&content_box, Some("content"));
        main_stack.set_visible_child_name("content");

        // Wire view toggle
        {
            let vs = view_stack.clone();
            graph_btn.connect_toggled(move |btn| {
                if btn.is_active() {
                    vs.set_visible_child_name("graph");
                }
            });
        }
        {
            let vs = view_stack.clone();
            maint_btn.connect_toggled(move |btn| {
                if btn.is_active() {
                    vs.set_visible_child_name("maintenance");
                }
            });
        }

        // Assemble window
        let outer_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        outer_box.append(&header);
        outer_box.append(&main_stack);
        window.set_content(Some(&outer_box));

        // Store references
        *self.main_stack.borrow_mut() = Some(main_stack);
        *self.view_stack.borrow_mut() = Some(view_stack);
        *self.graph_view.borrow_mut() = Some(graph_view);
        *self.maintenance_view.borrow_mut() = Some(maintenance_view);
        *self.status_label.borrow_mut() = Some(status_label);
        *self.remove_button.borrow_mut() = Some(remove_button);
        *self.search_entry.borrow_mut() = Some(search_entry);
        *self.progress_bar.borrow_mut() = Some(progress_bar);
        *self.progress_label.borrow_mut() = Some(progress_label);
        *self.loading_box.borrow_mut() = Some(loading_box);
    }
}

impl WidgetImpl for Window {}
impl WindowImpl for Window {}
impl ApplicationWindowImpl for Window {}
impl adw::subclass::application_window::AdwApplicationWindowImpl for Window {}
