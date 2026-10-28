use cxx_qt_build::{CxxQtBuilder, QmlFile, QmlModule};

fn main() {
    let qml_files = std::fs::read_dir("qml")
        .expect("qml directory")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "qml"))
        .collect::<Vec<_>>();
    for f in &qml_files {
        println!("cargo:rerun-if-changed={}", f.display());
    }
    let qml_files = qml_files.into_iter().map(|p| {
        let singleton = p.file_stem().is_some_and(|s| s == "Theme");
        QmlFile::from(p).singleton(singleton)
    });
    println!("cargo:rerun-if-changed=qml");
    println!("cargo:rerun-if-changed=resources/icons");

    let icons = std::fs::read_dir("resources/icons")
        .expect("icons directory")
        .filter_map(|e| e.ok().map(|e| e.path().to_string_lossy().into_owned()))
        .collect::<Vec<_>>();

    CxxQtBuilder::new_qml_module(QmlModule::new("app.pheme").qml_files(qml_files))
        .qt_module("Gui")
        .qt_module("Quick")
        .qt_module("QuickControls2")
        .qt_module("WebEngineQuick")
        .files([
            "src/bridge/app_controller.rs",
            "src/bridge/sidebar_model.rs",
            "src/bridge/timeline_model.rs",
        ])
        .cpp_file("cpp/platform.cpp")
        .qrc_resources(icons)
        .build();
}
