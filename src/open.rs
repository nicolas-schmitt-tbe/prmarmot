//! Opening links in the browser. GPUI's `open_url` hands any scheme to
//! NSWorkspace or xdg-open, so a URL that came from GitHub or the config is
//! opened only when core's `is_web_url` says it is a web address.

use gpui::App;
use prmarmot_core::board::is_web_url;

/// Open `url` in the default browser when it is an `https://` or `http://`
/// address. Any other scheme is ignored, and the return value says so.
pub fn open_web_url(cx: &App, url: &str) -> bool {
    if !is_web_url(url) {
        return false;
    }
    cx.open_url(url);
    true
}

/// Show a folder of the app's own (the themes folder) in the file manager.
/// The path is the app's, never one that arrived from GitHub or a config
/// value, which is why this takes a [`Path`] and not a string to gate.
pub fn open_folder(cx: &App, folder: &std::path::Path) {
    cx.open_url(&format!("file://{}", folder.display()));
}

#[cfg(test)]
mod tests {
    use prmarmot_core::board::is_web_url;

    /// The gate `open_web_url` applies before anything reaches the system
    /// opener: web pages pass, every other scheme and bare text do not.
    #[test]
    fn only_web_addresses_reach_the_opener() {
        for url in [
            "https://github.com/oliver-kriska/prmarmot/pull/1",
            "http://github.example.com/a/b/pull/2",
            "HTTPS://GITHUB.COM/a/b",
        ] {
            assert!(is_web_url(url), "{url}");
        }
        for url in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "",
            "github.com/oliver-kriska/prmarmot",
            "https://",
            "x-apple.systempreferences:com.apple.Notifications-Settings.extension",
            // A multi-byte character at the prefix boundary must not panic.
            "httpséé://x",
        ] {
            assert!(!is_web_url(url), "{url}");
        }
    }
}
