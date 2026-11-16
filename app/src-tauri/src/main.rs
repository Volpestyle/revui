#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::io::{BufRead, BufReader, Read};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_dialog::DialogExt;

/// Inside a review: a link that would leave it, or open beside it, goes to the browser, and the logo or q goes back to the list.
const LINKS: &str = r#"
if (location.hostname === "127.0.0.1") {
  addEventListener("click", (event) => {
    const link = event.target instanceof Element ? event.target.closest("a[href]") : null
    if (!link || (link.target !== "_blank" && link.origin === location.origin)) return
    event.preventDefault()
    location.href = link.href
  }, true)
  window.open = (url) => void (location.href = url)
  const reviews = () => void (location.href = "revui:reviews")
  addEventListener("click", (event) => event.target instanceof Element && event.target.closest(".bar .brand") && reviews())
  addEventListener("keydown", (event) => {
    const typing = event.target instanceof Element && event.target.closest("input, textarea, [contenteditable], dialog, [role=dialog], [role=alertdialog]")
    if (event.key === "q" && !typing && !event.metaKey && !event.ctrlKey && !event.altKey) reviews()
  })
  addEventListener("DOMContentLoaded", () => document.head.append(Object.assign(document.createElement("style"), { textContent: ".bar .brand { cursor: pointer }" })))
}
"#;

/// The server behind the review each window shows, by the window's label.
#[derive(Default)]
struct Servers(Mutex<HashMap<String, Child>>);

fn stop(app: &tauri::AppHandle, label: &str) {
    if let Some(mut child) = app.state::<Servers>().0.lock().unwrap().remove(label) {
        let _ = child.kill();
        let _ = child.wait();
    }
}

/// A window on the list of reviews, or on one review. Every window can show either: the list opens a review in place, and the review goes back to it.
fn window(app: &tauri::AppHandle, review: Option<tauri::Url>) -> Result<tauri::WebviewWindow, String> {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let label = format!("w{}", NEXT.fetch_add(1, Ordering::Relaxed));
    let (handle, own) = (app.clone(), label.clone());
    let made = WebviewWindowBuilder::new(app, &label, review.map_or(WebviewUrl::App("index.html".into()), WebviewUrl::External))
        .title("revui")
        .inner_size(1360.0, 900.0)
        .initialization_script(LINKS)
        .on_navigation(move |to| {
            if to.scheme() == "revui" {
                let (handle, own) = (handle.clone(), own.clone());
                std::thread::spawn(move || {
                    if let (Some(window), Ok(list)) = (handle.get_webview_window(&own), "tauri://localhost/index.html".parse()) {
                        let _ = window.navigate(list);
                    }
                    stop(&handle, &own);
                });
                return false;
            }
            let list = matches!(to.scheme(), "tauri" | "about") || to.host_str() == Some("tauri.localhost");
            let inside = list || (to.host_str() == Some("127.0.0.1") && !to.path().starts_with("/bundle/"));
            if !inside {
                let _ = Command::new("open").arg(to.as_str()).spawn();
            }
            inside
        })
        .build()
        .map_err(|e| e.to_string())?;
    let (handle, own) = (app.clone(), label);
    made.on_window_event(move |event| {
        if let WindowEvent::Destroyed = event {
            stop(&handle, &own);
        }
    });
    Ok(made)
}

/// An app started from the Dock gets a bare PATH. The tools a review needs (the agent, glab, gh) are on the one a login shell builds.
fn path() -> &'static str {
    static PATH: OnceLock<String> = OnceLock::new();
    PATH.get_or_init(|| {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
        let asked = Command::new(shell).args(["-ilc", "printf '\\n%s' \"$PATH\""]).stdin(Stdio::null()).stderr(Stdio::null()).output();
        let found = asked.ok().and_then(|out| String::from_utf8_lossy(&out.stdout).lines().last().map(str::to_string)).filter(|path| path.contains('/'));
        found.unwrap_or_else(|| std::env::var("PATH").unwrap_or_default())
    })
}

fn beside(name: &str) -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    Ok(exe.parent().ok_or("the app has no folder")?.join(name))
}

/// Runs the revui inside the app with the tools a login shell has, and returns what it printed.
fn revui(args: &[&str]) -> Result<String, String> {
    let out = Command::new(beside("revui")?).args(args).env("PATH", path()).stdin(Stdio::null()).output().map_err(|e| format!("revui: {e}"))?;
    if out.status.success() {
        return Ok(String::from_utf8_lossy(&out.stdout).trim().to_string());
    }
    let said = String::from_utf8_lossy(&out.stderr);
    Err(said.lines().rev().find(|line| !line.trim().is_empty()).unwrap_or("revui failed").trim().to_string())
}

#[tauri::command]
async fn pick_folder(app: tauri::AppHandle, title: String) -> Option<String> {
    app.dialog().file().set_title(title).blocking_pick_folder().map(|folder| folder.to_string())
}

#[tauri::command]
async fn reviews() -> Result<String, String> {
    revui(&["list"])
}

/// Opens a review in a window of its own. The agent starts in a checkout of the code: the folder given, or the one this
/// review's agent last ran in, or the one another review of the same repo used. With none of those, the repo's name comes
/// back and nothing opens, so the page can ask once.
#[tauri::command]
async fn open_review(app: tauri::AppHandle, from: tauri::WebviewWindow, named: String, folder: Option<String>, new_window: bool) -> Result<Option<String>, String> {
    let named = named.trim().to_string();
    if named.is_empty() {
        return Err("Paste a merge request or pull request link.".into());
    }
    let dir = if named.starts_with("http://") || named.starts_with("https://") { revui(&["fetch", &named])? } else { named };
    let all: Vec<serde_json::Value> = serde_json::from_str(&revui(&["list"])?).unwrap_or_default();
    let read = |name: &str| std::fs::read_to_string(std::path::Path::new(&dir).join(name)).ok().and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok());
    let repo = read("review.json").and_then(|review| review["sources"][0]["repo"].as_str().map(String::from)).unwrap_or_default();
    let here = read("session.json").and_then(|session| session["cwd"].as_str().map(String::from));
    let elsewhere = all.iter().filter(|row| !repo.is_empty() && row["repo"] == repo.as_str()).filter_map(|row| row["cwd"].as_str().map(String::from));
    let Some(folder) = folder.into_iter().chain(here).chain(elsewhere).find(|folder| !folder.is_empty() && std::path::Path::new(folder).is_dir()) else {
        return Ok(Some(repo));
    };
    let port = TcpListener::bind("127.0.0.1:0").and_then(|socket| socket.local_addr()).map_err(|e| e.to_string())?.port();
    let web = app.path().resource_dir().map_err(|e| e.to_string())?.join("web");
    let mut child = Command::new(beside("revui-server")?)
        .arg(&dir)
        .current_dir(&folder)
        .env("PATH", path())
        .env("PORT", port.to_string())
        .env("REVUI_BIN", beside("revui")?)
        .env("REVUI_WEB", web)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Could not start the review: {e}"))?;
    let mut ready = String::new();
    let _ = BufReader::new(child.stdout.take().ok_or("no output")?).read_line(&mut ready);
    if !ready.starts_with("revui: http://") {
        let mut said = String::new();
        let _ = child.stderr.take().map(|mut err| err.read_to_string(&mut said));
        let _ = child.kill();
        let _ = child.wait();
        return Err(said.lines().rev().find(|line| !line.trim().is_empty()).unwrap_or("The review did not start.").trim().to_string());
    }
    let url: tauri::Url = format!("http://127.0.0.1:{port}/").parse().map_err(|e: <tauri::Url as std::str::FromStr>::Err| e.to_string())?;
    let shown = if new_window { window(&app, Some(url))? } else { from.navigate(url).map(|_| from).map_err(|e| e.to_string())? };
    stop(&app, shown.label());
    app.state::<Servers>().0.lock().unwrap().insert(shown.label().to_string(), child);
    Ok(None)
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Servers::default())
        .invoke_handler(tauri::generate_handler![pick_folder, reviews, open_review])
        .setup(|app| Ok(window(app.handle(), None).map(|_| ())?))
        .build(tauri::generate_context!())
        .expect("revui could not start")
        .run(|app, event| match event {
            tauri::RunEvent::Exit => {
                for (_, mut child) in app.state::<Servers>().0.lock().unwrap().drain() {
                    let _ = child.kill();
                }
            }
            #[cfg(target_os = "macos")]
            tauri::RunEvent::Reopen { has_visible_windows: false, .. } => drop(window(app, None)),
            _ => {}
        });
}
