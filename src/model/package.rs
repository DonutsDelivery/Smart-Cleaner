use std::cell::{Cell, RefCell};
use std::path::PathBuf;

use gtk::glib;
use gtk::subclass::prelude::*;

use super::package_source::PackageSource;

#[derive(Debug, Clone)]
pub struct PackageInfo {
    pub id: String,
    pub name: String,
    pub display_name: String,
    pub version: String,
    pub description: String,
    pub categories: Vec<String>,
    pub icon_name: Option<String>,
    pub source: PackageSource,
    pub installed_size: u64,
    pub depends: Vec<String>,
    pub required_by: Vec<String>,
    pub provides: Vec<String>,
    pub is_explicit: bool,
    pub is_protected: bool,
    pub install_path: Option<PathBuf>,
    pub install_date: Option<i64>,
}

impl PackageInfo {
    pub fn qualified_id(&self) -> String {
        format!("{}:{}", self.source.prefix(), self.name)
    }
}

// GObject wrapper for PackageInfo to use in GTK models
mod imp {
    use super::*;

    #[derive(Default)]
    pub struct PackageObject {
        pub info: RefCell<Option<PackageInfo>>,
        pub selected: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PackageObject {
        const NAME: &'static str = "SysCleanPackageObject";
        type Type = super::PackageObject;
    }

    impl ObjectImpl for PackageObject {}
}

glib::wrapper! {
    pub struct PackageObject(ObjectSubclass<imp::PackageObject>);
}

impl PackageObject {
    pub fn new(info: PackageInfo) -> Self {
        let obj: Self = glib::Object::builder().build();
        obj.imp().info.replace(Some(info));
        obj
    }

    pub fn info(&self) -> std::cell::Ref<'_, PackageInfo> {
        std::cell::Ref::map(self.imp().info.borrow(), |opt| opt.as_ref().unwrap())
    }

    pub fn selected(&self) -> bool {
        self.imp().selected.get()
    }

    pub fn set_selected(&self, selected: bool) {
        self.imp().selected.set(selected);
    }

    pub fn id(&self) -> String {
        self.info().qualified_id()
    }

    pub fn name(&self) -> String {
        self.info().name.clone()
    }

    pub fn display_name(&self) -> String {
        let info = self.info();
        if info.display_name.is_empty() {
            info.name.clone()
        } else {
            info.display_name.clone()
        }
    }

    pub fn version(&self) -> String {
        self.info().version.clone()
    }

    pub fn description(&self) -> String {
        self.info().description.clone()
    }

    pub fn source(&self) -> PackageSource {
        self.info().source
    }

    pub fn source_label(&self) -> String {
        self.info().source.label().to_string()
    }

    pub fn installed_size(&self) -> u64 {
        self.info().installed_size
    }

    pub fn icon_name(&self) -> Option<String> {
        self.info().icon_name.clone()
    }

    pub fn is_explicit(&self) -> bool {
        self.info().is_explicit
    }

    pub fn depends(&self) -> Vec<String> {
        self.info().depends.clone()
    }

    pub fn required_by(&self) -> Vec<String> {
        self.info().required_by.clone()
    }

    pub fn categories(&self) -> Vec<String> {
        self.info().categories.clone()
    }
}
