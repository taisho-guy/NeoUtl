#[cxx::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("app_main.h");
        fn run_qt_application(qml_url: &str);
    }
}

pub fn run(qml_url: &str) {
    ffi::run_qt_application(qml_url);
}
