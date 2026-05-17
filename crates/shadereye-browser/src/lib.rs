//! Runs the harness page in a headless system Chromium and captures the
//! console/error transcript plus a screenshot.

pub mod harness;

use serde::Serialize;

#[derive(Debug, Clone)]
pub struct BrowserRunParams {
    pub source: String,
    pub width: u32,
    pub height: u32,
    pub frames: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct BrowserTranscript {
    pub console: Vec<String>, // every log/info/warn/error line, prefixed with level
    pub exceptions: Vec<String>, // uncaught JS exceptions
    pub gl_errors: Vec<String>, // lines tagged SHADEREYE_GL_ERROR / SHADER_LOG / PROGRAM_LOG
    pub compiled_ok: bool,
    #[serde(skip)]
    pub screenshot_png: Vec<u8>,
}

#[derive(Debug, thiserror::Error)]
pub enum BrowserError {
    #[error("no Chromium/Chrome found. Install Chrome or set SHADEREYE_BROWSER to its path")]
    NoBrowser,
    #[error("browser driver error: {0}")]
    Driver(String),
}

/// Locate a system Chrome/Chromium binary.
pub fn find_browser() -> Result<std::path::PathBuf, BrowserError> {
    if let Ok(p) = std::env::var("SHADEREYE_BROWSER") {
        let pb = std::path::PathBuf::from(p);
        if pb.exists() {
            return Ok(pb);
        }
    }
    for name in [
        "chrome",
        "chromium",
        "chromium-browser",
        "google-chrome",
        "msedge",
    ] {
        if let Ok(p) = which::which(name) {
            return Ok(p);
        }
    }
    #[cfg(windows)]
    for p in [
        r"C:\Program Files\Google\Chrome\Application\chrome.exe",
        r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
        r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
    ] {
        let pb = std::path::PathBuf::from(p);
        if pb.exists() {
            return Ok(pb);
        }
    }
    Err(BrowserError::NoBrowser)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_browser_reports_missing_clearly() {
        // Either it finds one (Ok) or returns the actionable NoBrowser error.
        match find_browser() {
            Ok(p) => assert!(p.exists()),
            Err(e) => assert!(e.to_string().contains("SHADEREYE_BROWSER")),
        }
    }
}
