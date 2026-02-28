mod app;
mod config;
mod model;
mod remover;
mod scanner;
mod ui;

use gtk::prelude::*;

fn main() {
    let app = app::Application::new();
    app.run();
}
