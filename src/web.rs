//! `revui web`: serves the interface built for the browser (gpu/dist), and does on its behalf what a page cannot:
//! run revui and read the bundle's files. Only this machine, and only the page it served, is answered.

use crate::mrs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tiny_http::{Header, Method, Request, Response, Server};

const RUNS: [&str; 9] = ["json", "list", "state", "publish", "prompt", "fetch", "threads", "agent", "tree"];

fn kind(path: &Path) -> &'static str {
    match path.extension().and_then(|kind| kind.to_str()).unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript",
        "wasm" => "application/wasm",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "mp4" => "video/mp4",
        "json" => "application/json",
        _ => "text/plain; charset=utf-8",
    }
}

fn reply(request: Request, status: u16, kind: &str, body: Vec<u8>) {
    let header = Header::from_bytes("content-type", kind).expect("a content type");
    let sniff = Header::from_bytes("x-content-type-options", "nosniff").expect("a header");
    let _ = request.respond(Response::from_data(body).with_status_code(status).with_header(header).with_header(sniff));
}

fn param(url: &str, name: &str) -> Option<String> {
    let value = url.split_once('?')?.1.split('&').find_map(|pair| pair.strip_prefix(&format!("{name}=")))?;
    Some(decode(value))
}

fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let (mut out, mut at) = (Vec::new(), 0);
    while at < bytes.len() {
        match (bytes[at], bytes.get(at + 1..at + 3).and_then(|hex| u8::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok())) {
            (b'%', Some(byte)) => (out.push(byte), at += 3).1,
            (byte, _) => (out.push(byte), at += 1).1,
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// A file under `root`, and nothing a `..` or a link could reach outside it.
fn inside(root: &Path, path: &str) -> Option<PathBuf> {
    let (root, full) = (root.canonicalize().ok()?, root.join(path).canonicalize().ok()?);
    (full.starts_with(&root) && full.is_file()).then_some(full)
}

/// A bundle the page may read: the one being served, or any other this machine keeps.
fn bundle(served: &Path, asked: Option<String>) -> Option<PathBuf> {
    let asked = PathBuf::from(asked?).canonicalize().ok()?;
    let kept = mrs::data_dir().canonicalize().ok();
    (asked == served || kept.is_some_and(|kept| asked.starts_with(kept))).then_some(asked)
}

fn run(args: &[String], input: Option<&str>) -> serde_json::Value {
    let ran = (|| {
        let mut child = Command::new(std::env::current_exe().ok()?).args(args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().ok()?;
        if let (Some(mut stdin), Some(input)) = (child.stdin.take(), input) {
            let _ = stdin.write_all(input.as_bytes());
        }
        child.wait_with_output().ok()
    })();
    match ran {
        Some(out) => {
            let said = String::from_utf8_lossy(&out.stderr);
            let problem = said.lines().rev().find(|line| !line.trim().is_empty()).unwrap_or("").trim().to_string();
            serde_json::json!({ "ok": out.status.success(), "out": String::from_utf8_lossy(&out.stdout), "problem": problem })
        }
        None => serde_json::json!({ "ok": false, "out": "", "problem": "revui could not run" }),
    }
}

pub fn serve(named: &str) -> Result<(), Box<dyn std::error::Error>> {
    let dist = Path::new(env!("CARGO_MANIFEST_DIR")).join("gpu/dist");
    if !dist.join("index.html").exists() {
        return Err(format!("revui web serves the interface built at {}, and it is not there. From a checkout of revui: cd gpu && trunk build --release", dist.display()).into());
    }
    let dir = if named.starts_with("http://") || named.starts_with("https://") { mrs::fetch(named)? } else { PathBuf::from(named) };
    let dir = dir.canonicalize().map_err(|e| format!("{}: {e}", dir.display()))?;
    let first: u16 = std::env::var("PORT").ok().and_then(|port| port.parse().ok()).unwrap_or(4319);
    let (server, port) = (first..first.saturating_add(20)).find_map(|port| Some((Server::http(("127.0.0.1", port)).ok()?, port))).ok_or("no free port to serve on")?;
    let hosts = [format!("127.0.0.1:{port}"), format!("localhost:{port}")];
    let url = format!("http://127.0.0.1:{port}");
    println!("revui: {url}  ({})", dir.display());
    if std::env::var("REVUI_NO_OPEN").is_err() {
        let opener = std::env::var("BROWSER").ok().filter(|opener| !opener.is_empty()).unwrap_or_else(|| if cfg!(target_os = "macos") { "open".into() } else { "xdg-open".into() });
        let _ = Command::new("sh").args(["-c", &format!("{opener} \"$1\""), "sh", &url]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
    }
    for mut request in server.incoming_requests() {
        let header = |name: &str| request.headers().iter().find(|header| header.field.as_str().as_str().eq_ignore_ascii_case(name)).map(|header| header.value.as_str().to_string());
        let host = header("host").unwrap_or_default();
        let get = *request.method() == Method::Get;
        let trusted = hosts.contains(&host) && (get || (header("origin").as_deref() == Some(&format!("http://{host}")) && header("content-type").is_some_and(|kind| kind.starts_with("application/json"))));
        let whole = request.url().to_string();
        let path = decode(whole.split('?').next().unwrap_or("/"));
        if !trusted {
            reply(request, 403, "text/plain", b"forbidden".to_vec());
            continue;
        }
        match (get, path.as_str()) {
            (true, "/api/dir") => reply(request, 200, "text/plain", dir.to_string_lossy().into_owned().into_bytes()),
            (true, "/api/stamp") => {
                let newest = bundle(&dir, param(&whole, "dir")).and_then(|bundle| ["review.json", "state.json", "threads.json"].iter().filter_map(|name| std::fs::metadata(bundle.join(name)).and_then(|file| file.modified()).ok()).max());
                reply(request, 200, "text/plain", format!("{newest:?}").into_bytes());
            }
            (false, "/api/run") => {
                let mut body = String::new();
                let _ = request.as_reader().read_to_string(&mut body);
                let asked: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
                let args: Vec<String> = asked["args"].as_array().into_iter().flatten().filter_map(|arg| arg.as_str().map(String::from)).collect();
                let answer = match args.first().is_some_and(|first| RUNS.contains(&first.as_str())) {
                    true => run(&args, asked["input"].as_str()),
                    false => serde_json::json!({ "ok": false, "out": "", "problem": "not something the page may run" }),
                };
                reply(request, 200, "application/json", answer.to_string().into_bytes());
            }
            (true, file) if file.starts_with("/bundle/") => match bundle(&dir, param(&whole, "dir")).and_then(|bundle| inside(&bundle, &file["/bundle/".len()..])) {
                Some(found) => reply(request, 200, kind(&found), std::fs::read(&found).unwrap_or_default()),
                None => reply(request, 404, "text/plain", b"not found".to_vec()),
            },
            (true, file) => {
                let found = inside(&dist, file.trim_start_matches('/')).unwrap_or_else(|| dist.join("index.html"));
                reply(request, 200, kind(&found), std::fs::read(&found).unwrap_or_default());
            }
            _ => reply(request, 404, "text/plain", b"not found".to_vec()),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_outside_the_bundle_is_served() {
        let root = std::env::temp_dir().join(format!("revui-web-test-{}", std::process::id()));
        std::fs::create_dir_all(root.join("bundle/assets")).unwrap();
        std::fs::write(root.join("bundle/assets/map.png"), b"x").unwrap();
        std::fs::write(root.join("secret"), b"x").unwrap();
        let bundle = root.join("bundle");
        assert!(inside(&bundle, "assets/map.png").is_some());
        assert!(inside(&bundle, "../secret").is_none(), "a path that climbs out is refused");
        assert!(inside(&bundle, &decode("..%2fsecret")).is_none(), "and so is its encoded form");
        assert!(inside(&bundle, "assets").is_none(), "a folder is not a file");
        assert_eq!(param("/bundle/a.png?dir=%2Ftmp%2Fx&v=1", "dir").as_deref(), Some("/tmp/x"));
        let _ = std::fs::remove_dir_all(root);
    }
}
