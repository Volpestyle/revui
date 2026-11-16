//! What the interface asks of the machine it runs on. As an app it runs revui and reads the bundle's files itself;
//! in a browser it asks the server `revui web` started, which does the same on its behalf.

#[cfg(not(target_arch = "wasm32"))]
mod at {
    use std::io::Write;
    use std::process::{Command, Stdio};

    pub fn run(args: &[&str], input: Option<&str>) -> Result<String, String> {
        let bin = std::env::var("REVUI_BIN").unwrap_or_else(|_| "revui".into());
        let mut child = Command::new(bin).args(args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e| format!("revui: {e}"))?;
        if let (Some(mut stdin), Some(input)) = (child.stdin.take(), input) {
            let _ = stdin.write_all(input.as_bytes());
        }
        let out = child.wait_with_output().map_err(|e| e.to_string())?;
        match out.status.success() {
            true => Ok(String::from_utf8_lossy(&out.stdout).into_owned()),
            false => Err(String::from_utf8_lossy(&out.stderr).lines().rev().find(|line| !line.trim().is_empty()).unwrap_or("revui failed").trim().to_string()),
        }
    }

    pub fn named() -> Option<String> {
        std::env::args().nth(1)
    }

    pub fn text(dir: &str, path: &str) -> Option<String> {
        std::fs::read_to_string(std::path::Path::new(dir).join(path)).ok()
    }

    pub fn uri(dir: &str, path: &str) -> String {
        format!("file://{}", std::path::Path::new(dir).join(path).display())
    }

    /// Hands a file of the bundle to whatever this machine opens it with: Preview, QuickTime, a browser.
    pub fn open(dir: &str, path: &str) {
        let (dir, file) = (std::path::Path::new(dir).canonicalize(), std::path::Path::new(dir).join(path).canonicalize());
        if let (Ok(dir), Ok(file)) = (dir, file) {
            if file.starts_with(dir) {
                let _ = Command::new(if cfg!(target_os = "macos") { "open" } else { "xdg-open" }).arg(file).spawn();
            }
        }
    }

    pub fn stamp(dir: &str) -> String {
        let newest = ["review.json", "state.json", "threads.json"].iter().filter_map(|name| std::fs::metadata(std::path::Path::new(dir).join(name)).and_then(|file| file.modified()).ok()).max();
        format!("{newest:?}")
    }
}

#[cfg(target_arch = "wasm32")]
mod at {
    use web_sys::XmlHttpRequest;

    // ponytail: these wait on the server with a synchronous request, which keeps the interface's code the same as the
    // app's. The server is on this machine and answers in milliseconds; make them async if a slow fetch ever freezes a frame.
    fn ask(method: &str, url: &str, body: Option<&str>) -> Result<String, String> {
        let problem = |_| "The revui server did not answer.".to_string();
        let request = XmlHttpRequest::new().map_err(problem)?;
        request.open_with_async(method, url, false).map_err(problem)?;
        if body.is_some() {
            request.set_request_header("content-type", "application/json").map_err(problem)?;
        }
        request.send_with_opt_str(body).map_err(problem)?;
        let text = request.response_text().map_err(problem)?.unwrap_or_default();
        if request.status().map_err(problem)? == 200 { Ok(text) } else { Err(text) }
    }

    fn quoted(text: &str) -> String {
        js_sys::encode_uri_component(text).into()
    }

    pub fn run(args: &[&str], input: Option<&str>) -> Result<String, String> {
        let answer = ask("POST", "/api/run", Some(&serde_json::json!({ "args": args, "input": input }).to_string()))?;
        let answer: serde_json::Value = serde_json::from_str(&answer).map_err(|e| e.to_string())?;
        match answer["ok"] == true {
            true => Ok(answer["out"].as_str().unwrap_or("").to_string()),
            false => Err(answer["problem"].as_str().unwrap_or("revui failed").to_string()),
        }
    }

    pub fn named() -> Option<String> {
        ask("GET", "/api/dir", None).ok().filter(|dir| !dir.is_empty())
    }

    pub fn text(dir: &str, path: &str) -> Option<String> {
        ask("GET", &format!("/bundle/{path}?dir={}", quoted(dir)), None).ok()
    }

    pub fn uri(dir: &str, path: &str) -> String {
        let origin = web_sys::window().and_then(|window| window.location().origin().ok()).unwrap_or_default();
        format!("{origin}/bundle/{path}?dir={}", quoted(dir))
    }

    pub fn open(dir: &str, path: &str) {
        let _ = web_sys::window().map(|window| window.open_with_url_and_target(&uri(dir, path), "_blank"));
    }

    pub fn stamp(dir: &str) -> String {
        ask("GET", &format!("/api/stamp?dir={}", quoted(dir)), None).unwrap_or_default()
    }
}

pub use at::*;
