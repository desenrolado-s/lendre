#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use legenda::ui::run_app;

fn main() {
    let smoke = std::env::args().any(|arg| arg == "--smoke-test");
    run_app(smoke);
}
