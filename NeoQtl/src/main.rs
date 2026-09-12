mod ffi;
mod renderer;

use cxx_qt_lib::{QByteArray, QGuiApplication, QQmlApplicationEngine, QUrl};

fn main() {
    ffi::register();

    let mut application = QGuiApplication::new();
    let mut engine = QQmlApplicationEngine::new();
    let url = QUrl::from_encoded(&QByteArray::from("qrc:/NeoQtl/qml/Main.qml"));
    engine.pin_mut().load(&url);
    application.pin_mut().exec();
}
