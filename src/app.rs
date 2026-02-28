use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

use crate::config;
use crate::ui::window::Window;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct Application;

    #[glib::object_subclass]
    impl ObjectSubclass for Application {
        const NAME: &'static str = "SmartCleanerApplication";
        type Type = super::Application;
        type ParentType = adw::Application;
    }

    impl ObjectImpl for Application {}

    impl ApplicationImpl for Application {
        fn activate(&self) {
            let app = self.obj();
            let window = Window::new(app.upcast_ref());
            window.present();
        }
    }

    impl GtkApplicationImpl for Application {}
    impl adw::subclass::application::AdwApplicationImpl for Application {}
}

glib::wrapper! {
    pub struct Application(ObjectSubclass<imp::Application>)
        @extends adw::Application, gtk::Application, gtk::gio::Application,
        @implements gtk::gio::ActionMap, gtk::gio::ActionGroup;
}

impl Application {
    pub fn new() -> Self {
        glib::Object::builder()
            .property("application-id", config::APP_ID)
            .build()
    }
}
