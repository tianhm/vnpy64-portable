//! Desktop runner. Linux uses GTK 4, Windows uses the Win32 nsh backend.

#[cfg(target_os = "linux")]
use nsh_platform_linux as platform;
#[cfg(target_os = "macos")]
use nsh_platform_macos as platform;
#[cfg(target_os = "windows")]
use nsh_platform_windows as platform;

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn main() -> impl std::process::Termination {
    ctp_md_gui::apply_cli();
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    platform::run_app(
        platform::AppSource::Rust(platform::Root::of::<ctp_md_gui::App>()),
        platform::AppOptions {
            title: "CTP 行情".to_owned(),
            app_id: "local.ctp.md.gui".to_owned(),
            size: (1280, 800),
            ..Default::default()
        },
    )
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn main() {
    compile_error!("ctp-md-gui runs on Windows, Linux, or macOS");
}
