/// System notifications via notify-rust.
///
/// Gracefully no-ops on platforms where notifications aren't available.

pub fn send(title: &str, body: &str) {
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    {
        let _ = notify_rust::Notification::new()
            .summary(title)
            .body(body)
            .icon("audio-x-generic")  // falls back gracefully on Windows
            .timeout(notify_rust::Timeout::Milliseconds(6000))
            .show();
    }
}
