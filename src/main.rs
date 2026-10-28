//! Pheme: a Feedly-backed RSS reader for Linux, in the spirit of NetNewsWire.

mod bridge;
mod credentials;
mod feedly;
mod local;
mod paths;
mod render;
mod store;
mod sync;
mod text;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QString, QUrl};

unsafe extern "C" {
    fn pheme_platform_init();
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("pheme=info")).init();

    // SAFETY: called once, before any Qt object exists, as QtWebEngine requires.
    unsafe { pheme_platform_init() };

    let mut app = QGuiApplication::new();
    if let Some(mut app) = app.as_mut() {
        app.as_mut().set_application_name(&QString::from("Pheme"));
        app.as_mut().set_organization_name(&QString::from("pheme"));
    }
    QGuiApplication::set_desktop_file_name(&QString::from("pheme"));

    let mut engine = QQmlApplicationEngine::new();
    if let Some(mut engine) = engine.as_mut() {
        engine
            .as_mut()
            .on_object_creation_failed(|_, url| {
                log::error!("Could not load {url}; see the Qt messages above (or `journalctl --user -t pheme`).");
                std::process::exit(1);
            })
            .release();
        engine.load(&QUrl::from("qrc:/qt/qml/app/pheme/qml/Main.qml"));
    }
    if let Some(app) = app.as_mut() {
        std::process::exit(app.exec());
    }
}
