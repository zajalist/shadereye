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

use base64::Engine;
use chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotFormat;
use chromiumoxide::{Browser, BrowserConfig};
use futures::StreamExt;

/// Run the shader in a real headless browser; collect console/exception/GL logs
/// and a screenshot. Times out after ~15s of no completion sentinel.
pub fn run_in_browser(p: &BrowserRunParams) -> Result<BrowserTranscript, BrowserError> {
    let exe = find_browser()?;
    let html = harness::generate_html(&p.source, p.width, p.height, p.frames.max(1));
    let data_url = format!(
        "data:text/html;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(html.as_bytes())
    );

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| BrowserError::Driver(e.to_string()))?;

    rt.block_on(async move {
        let cfg = BrowserConfig::builder()
            .chrome_executable(exe)
            .arg("--headless=new")
            .arg("--use-gl=swiftshader")
            .arg("--enable-unsafe-swiftshader")
            .arg("--disable-gpu-sandbox")
            .build()
            .map_err(BrowserError::Driver)?;
        let (mut browser, mut handler) = Browser::launch(cfg)
            .await
            .map_err(|e| BrowserError::Driver(e.to_string()))?;
        let handle = tokio::spawn(async move { while handler.next().await.is_some() {} });

        let page = browser
            .new_page("about:blank")
            .await
            .map_err(|e| BrowserError::Driver(e.to_string()))?;

        // chromiumoxide 0.7 only auto-enables the Log/Performance domains on a
        // new page, not Runtime, so Runtime.consoleAPICalled /
        // Runtime.exceptionThrown never fire without this explicit enable.
        page.enable_runtime()
            .await
            .map_err(|e| BrowserError::Driver(e.to_string()))?;

        let mut console = Vec::new();
        let mut exceptions = Vec::new();
        let mut gl_errors = Vec::new();

        let mut logs = page
            .event_listener::<chromiumoxide::cdp::js_protocol::runtime::EventConsoleApiCalled>()
            .await
            .map_err(|e| BrowserError::Driver(e.to_string()))?;
        let mut exc = page
            .event_listener::<chromiumoxide::cdp::js_protocol::runtime::EventExceptionThrown>()
            .await
            .map_err(|e| BrowserError::Driver(e.to_string()))?;

        // Navigate only after Runtime is enabled and the console/exception
        // listeners are registered, so no early log lines (including the
        // SHADEREYE_DONE sentinel) are missed on fast-completing shaders.
        page.goto(data_url.as_str())
            .await
            .map_err(|e| BrowserError::Driver(e.to_string()))?;

        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(15);
        let mut done = false;
        while tokio::time::Instant::now() < deadline && !done {
            tokio::select! {
                Some(ev) = logs.next() => {
                    let level = ev.r#type.clone();
                    let text: String = ev.args.iter()
                        .filter_map(|a| a.value.as_ref().map(|v| v.to_string().trim_matches('"').to_string()))
                        .collect::<Vec<_>>().join(" ");
                    if text.contains("SHADEREYE_DONE") { done = true; }
                    if text.contains("SHADEREYE_GL_ERROR") || text.contains("SHADEREYE_SHADER_LOG")
                        || text.contains("SHADEREYE_PROGRAM_LOG") {
                        gl_errors.push(text.clone());
                    }
                    console.push(format!("[{level:?}] {text}"));
                }
                Some(ev) = exc.next() => {
                    exceptions.push(format!("{:?}", ev.exception_details));
                }
                _ = tokio::time::sleep(std::time::Duration::from_millis(200)) => {}
            }
        }

        let shot = page
            .screenshot(
                chromiumoxide::page::ScreenshotParams::builder()
                    .format(CaptureScreenshotFormat::Png)
                    .build(),
            )
            .await
            .unwrap_or_default();

        let _ = browser.close().await;
        handle.abort();

        let compiled_ok = !gl_errors.iter().any(|l| l.contains("compile failed") || l.contains("link failed"));
        Ok(BrowserTranscript {
            console,
            exceptions,
            gl_errors,
            compiled_ok,
            screenshot_png: shot,
        })
    })
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
