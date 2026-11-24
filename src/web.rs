//! `revui web`: serves the interface built for the browser (gpu/dist), and does on its behalf what a page cannot:
//! run revui and read the bundle's files. Only this machine, and only the page it served, is answered.

use crate::mrs;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tiny_http::{Header, Method, Request, Response, Server};

const RUNS: [&str; 10] = ["json", "list", "state", "publish", "prompt", "fetch", "threads", "agent", "tree", "open"];
const DEFAULT_PORT: u16 = 4319;
const PORT_SPAN: u16 = 20;

#[derive(Debug, PartialEq, Eq)]
pub struct WebArgs {
    pub target: Option<String>,
    pub open: bool,
    pub daemon: bool,
}

pub fn parse_args(args: &[String]) -> Result<WebArgs, String> {
    let mut target = None;
    let mut open = false;
    let mut daemon = false;
    for arg in args {
        match arg.as_str() {
            "--open" => open = true,
            "--daemon" => daemon = true,
            flag if flag.starts_with("--") => return Err(format!("unknown flag: {flag}\nusage: revui web [--open] [<dir | mr url>]")),
            _ if target.is_some() => return Err("usage: revui web [--open] [<dir | mr url>]".into()),
            _ => target = Some(arg.clone()),
        }
    }
    if open && daemon {
        return Err("cannot combine --open and --daemon".into());
    }
    Ok(WebArgs { target, open, daemon })
}

struct Context {
    dist: PathBuf,
    data_dir: PathBuf,
    served: Option<PathBuf>,
    hosts: [String; 2],
}

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

/// A bundle under `data_dir` identified by its single-segment directory name.
fn resolve_slug_in(data_dir: &Path, slug: &str) -> Option<PathBuf> {
    if slug.is_empty() || slug == "." || slug == ".." || slug.contains(['/', '\\', '\0']) {
        return None;
    }
    let kept = data_dir.canonicalize().ok()?;
    let found = kept.join(slug).canonicalize().ok()?;
    (found.starts_with(&kept) && found != kept && found.is_dir()).then_some(found)
}

/// A bundle the page may read: the one explicitly served, or any bundle kept under `data_dir`.
fn resolve_bundle_in(data_dir: &Path, served: Option<&Path>, asked: Option<String>) -> Option<PathBuf> {
    let asked = PathBuf::from(asked?).canonicalize().ok()?;
    if !asked.is_dir() {
        return None;
    }
    let kept = data_dir.canonicalize().ok();
    let allowed = served.is_some_and(|dir| asked == dir) || kept.is_some_and(|kept| asked.starts_with(&kept) && asked != kept);
    allowed.then_some(asked)
}

fn route_for_in(data_dir: &Path, dir: &Path) -> String {
    let dir = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
    if let Ok(kept) = data_dir.canonicalize()
        && dir.parent() == Some(kept.as_path())
        && let Some(slug) = dir.file_name().and_then(|name| name.to_str())
    {
        return format!("/r/{}", crate::url_encode(slug));
    }
    format!("/?dir={}", crate::url_encode(&dir.to_string_lossy()))
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

fn handle(mut request: Request, ctx: &Context) {
    let header = |name: &str| request.headers().iter().find(|header| header.field.as_str().as_str().eq_ignore_ascii_case(name)).map(|header| header.value.as_str().to_string());
    let host = header("host").unwrap_or_default();
    let get = *request.method() == Method::Get;
    let trusted = ctx.hosts.contains(&host) && (get || (header("origin").as_deref() == Some(&format!("http://{host}")) && header("content-type").is_some_and(|kind| kind.starts_with("application/json"))));
    let whole = request.url().to_string();
    let path = decode(whole.split('?').next().unwrap_or("/"));
    if !trusted {
        reply(request, 403, "text/plain", b"forbidden".to_vec());
        return;
    }
    match (get, path.as_str()) {
        (true, "/api/ping") => reply(request, 200, "text/plain; charset=utf-8", b"revui".to_vec()),
        (true, "/api/dir") => {
            if let Some(slug) = param(&whole, "slug") {
                match resolve_slug_in(&ctx.data_dir, &slug) {
                    Some(found) => reply(request, 200, "text/plain; charset=utf-8", found.to_string_lossy().into_owned().into_bytes()),
                    None => reply(request, 404, "text/plain", b"not found".to_vec()),
                }
            } else if let Some(asked) = param(&whole, "dir") {
                match resolve_bundle_in(&ctx.data_dir, ctx.served.as_deref(), Some(asked)) {
                    Some(found) => reply(request, 200, "text/plain; charset=utf-8", found.to_string_lossy().into_owned().into_bytes()),
                    None => reply(request, 404, "text/plain", b"not found".to_vec()),
                }
            } else {
                let default = ctx.served.as_ref().map(|dir| dir.to_string_lossy().into_owned()).unwrap_or_default();
                reply(request, 200, "text/plain; charset=utf-8", default.into_bytes());
            }
        }
        (true, "/api/route") => match resolve_bundle_in(&ctx.data_dir, ctx.served.as_deref(), param(&whole, "dir")) {
            Some(found) => reply(request, 200, "text/plain; charset=utf-8", route_for_in(&ctx.data_dir, &found).into_bytes()),
            None => reply(request, 404, "text/plain", b"not found".to_vec()),
        },
        (true, "/api/stamp") => {
            let newest = resolve_bundle_in(&ctx.data_dir, ctx.served.as_deref(), param(&whole, "dir"))
                .and_then(|bundle| ["review.json", "state.json", "threads.json"].iter().filter_map(|name| std::fs::metadata(bundle.join(name)).and_then(|file| file.modified()).ok()).max());
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
        (true, file) if file.starts_with("/bundle/") => match resolve_bundle_in(&ctx.data_dir, ctx.served.as_deref(), param(&whole, "dir")).and_then(|bundle| inside(&bundle, &file["/bundle/".len()..])) {
            Some(found) => reply(request, 200, kind(&found), std::fs::read(&found).unwrap_or_default()),
            None => reply(request, 404, "text/plain", b"not found".to_vec()),
        },
        (true, file) => {
            let found = inside(&ctx.dist, file.trim_start_matches('/')).unwrap_or_else(|| ctx.dist.join("index.html"));
            reply(request, 200, kind(&found), std::fs::read(&found).unwrap_or_default());
        }
        _ => reply(request, 404, "text/plain", b"not found".to_vec()),
    }
}

fn dist_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let dist = Path::new(env!("CARGO_MANIFEST_DIR")).join("gpu/dist");
    if !dist.join("index.html").exists() {
        return Err(format!("revui web serves the interface built at {}, and it is not there. From a checkout of revui: cd gpu && trunk build --release", dist.display()).into());
    }
    Ok(dist)
}

fn configured_port() -> u16 {
    std::env::var("PORT").ok().and_then(|port| port.parse().ok()).unwrap_or(DEFAULT_PORT)
}

fn resolve_target(named: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let dir = if named.starts_with("http://") || named.starts_with("https://") { mrs::fetch(named)? } else { PathBuf::from(named) };
    let dir = dir.canonicalize().map_err(|e| format!("{}: {e}", dir.display()))?;
    if !dir.is_dir() {
        return Err(format!("{}: not a directory", dir.display()).into());
    }
    Ok(dir)
}

fn ping(port: u16) -> bool {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let Ok(mut stream) = TcpStream::connect_timeout(&addr, Duration::from_millis(200)) else { return false };
    let _ = stream.set_read_timeout(Some(Duration::from_millis(300)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(200)));
    let req = format!("GET /api/ping HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    if stream.write_all(req.as_bytes()).is_err() {
        return false;
    }
    let mut response = String::new();
    let _ = stream.read_to_string(&mut response);
    let Some((head, body)) = response.split_once("\r\n\r\n") else { return false };
    let ok = head.lines().next().is_some_and(|line| line.starts_with("HTTP/1.") && line.contains(" 200"));
    ok && body.trim() == "revui"
}

fn ensure_daemon(extra_dir: Option<&Path>) -> Result<u16, Box<dyn std::error::Error>> {
    let first = configured_port();
    for port in first..first.saturating_add(PORT_SPAN) {
        if ping(port) {
            return Ok(port);
        }
    }
    let port = (first..first.saturating_add(PORT_SPAN)).find(|&port| TcpListener::bind(("127.0.0.1", port)).is_ok()).ok_or("no free port to serve on")?;
    let exe = std::env::current_exe()?;
    let mut cmd = Command::new(exe);
    cmd.args(["web", "--daemon"]);
    if let Some(dir) = extra_dir {
        cmd.arg(dir);
    }
    cmd.env("PORT", port.to_string()).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    cmd.spawn()?;
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if ping(port) {
            return Ok(port);
        }
        std::thread::sleep(Duration::from_millis(40));
    }
    Err(format!("revui web daemon did not respond on 127.0.0.1:{port}").into())
}

fn herdr_pane(json: &str, terminal_id: &str) -> Option<String> {
    let data: serde_json::Value = serde_json::from_str(json).ok()?;
    for space in data["spaces"].as_array()? {
        for tab in space["tabs"].as_array().into_iter().flatten() {
            for term in tab["terminals"].as_array().into_iter().flatten() {
                if term["terminal"].as_str() == Some(terminal_id)
                    && let Some(pane) = term["pane"].as_str().filter(|pane| !pane.is_empty())
                {
                    return Some(pane.to_string());
                }
            }
        }
    }
    None
}

fn open_in_herdr(url: &str) -> bool {
    let in_herdr = std::env::var("HERDR_BROWSER").ok().filter(|v| !v.is_empty()).is_some();
    let term_id = std::env::var("HC_TERMINAL_ID").ok().filter(|v| !v.is_empty());
    if !in_herdr && term_id.is_none() {
        return false;
    }
    let on_path = Command::new("sh").args(["-c", "command -v herdr-browser >/dev/null"]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success());
    if !on_path {
        return false;
    }
    let pane = term_id.and_then(|id| {
        let out = Command::new("herdr-browser").arg("ls").stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
        out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned()).and_then(|json| herdr_pane(&json, &id))
    });
    let mut cmd = Command::new("herdr-browser");
    cmd.args(["do", "browser.open", "--profile", "agent", "--url", url, "--place", "right"]);
    if let Some(pane) = pane {
        cmd.args(["--pane", &pane]);
    }
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().is_ok()
}

fn open_url(url: &str) {
    if std::env::var("REVUI_NO_OPEN").ok().filter(|v| !v.is_empty()).is_some() {
        return;
    }
    if open_in_herdr(url) {
        return;
    }
    let opener = std::env::var("BROWSER").ok().filter(|opener| !opener.is_empty()).unwrap_or_else(|| if cfg!(target_os = "macos") { "open".into() } else { "xdg-open".into() });
    let _ = Command::new("sh").args(["-c", &format!("{opener} \"$1\""), "sh", url]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
}

pub fn cli(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let parsed = parse_args(args)?;
    let dist = dist_dir()?;
    let data_dir = mrs::data_dir();
    let dir = parsed.target.as_deref().map(resolve_target).transpose()?;
    if parsed.open {
        let external = dir.as_deref().filter(|dir| resolve_bundle_in(&data_dir, None, Some(dir.to_string_lossy().into_owned())).is_none());
        let port = ensure_daemon(external)?;
        let route = dir.as_deref().map_or_else(|| "/".to_string(), |dir| route_for_in(&data_dir, dir));
        let url = format!("http://127.0.0.1:{port}{route}");
        println!("{url}");
        open_url(&url);
        return Ok(());
    }
    serve_loop(dist, data_dir, dir, parsed.daemon)
}

fn serve_loop(dist: PathBuf, data_dir: PathBuf, served: Option<PathBuf>, daemon: bool) -> Result<(), Box<dyn std::error::Error>> {
    let first = configured_port();
    let (server, port) = if daemon {
        (Server::http(("127.0.0.1", first)).map_err(|e| format!("could not bind 127.0.0.1:{first}: {e}"))?, first)
    } else {
        (first..first.saturating_add(PORT_SPAN)).find_map(|port| Some((Server::http(("127.0.0.1", port)).ok()?, port))).ok_or("no free port to serve on")?
    };
    let hosts = [format!("127.0.0.1:{port}"), format!("localhost:{port}")];
    if !daemon {
        let route = served.as_deref().map_or_else(|| "/".to_string(), |dir| route_for_in(&data_dir, dir));
        let url = format!("http://127.0.0.1:{port}{route}");
        match &served {
            Some(dir) => println!("revui: {url}  ({})", dir.display()),
            None => println!("revui: {url}"),
        }
        open_url(&url);
    }
    let ctx = Arc::new(Context { dist, data_dir, served, hosts });
    for request in server.incoming_requests() {
        let ctx = Arc::clone(&ctx);
        std::thread::spawn(move || handle(request, &ctx));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("revui-web-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn nothing_outside_the_bundle_is_served() {
        let root = temp_root("inside");
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

    #[test]
    fn slug_and_bundle_resolution_enforce_boundaries() {
        let root = temp_root("slug");
        let data_dir = root.join("data");
        let kept = data_dir.join("repo-42-deadbeef");
        let external = root.join("external");
        let other = root.join("other");
        std::fs::create_dir_all(&kept).unwrap();
        std::fs::create_dir_all(&external).unwrap();
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(data_dir.join("panes.json"), b"{}").unwrap();

        let kept_canon = kept.canonicalize().unwrap();
        let external_canon = external.canonicalize().unwrap();

        assert_eq!(resolve_slug_in(&data_dir, "repo-42-deadbeef"), Some(kept_canon.clone()));
        assert!(resolve_slug_in(&data_dir, "").is_none());
        assert!(resolve_slug_in(&data_dir, ".").is_none());
        assert!(resolve_slug_in(&data_dir, "..").is_none());
        assert!(resolve_slug_in(&data_dir, "../external").is_none());
        assert!(resolve_slug_in(&data_dir, "a/b").is_none());
        assert!(resolve_slug_in(&data_dir, "panes.json").is_none());
        assert!(resolve_slug_in(&data_dir, "missing").is_none());

        assert_eq!(resolve_bundle_in(&data_dir, None, Some(kept.to_string_lossy().into_owned())), Some(kept_canon.clone()));
        assert!(resolve_bundle_in(&data_dir, None, Some(data_dir.to_string_lossy().into_owned())).is_none());
        assert!(resolve_bundle_in(&data_dir, None, Some(other.to_string_lossy().into_owned())).is_none());
        assert_eq!(resolve_bundle_in(&data_dir, Some(&external_canon), Some(external.to_string_lossy().into_owned())), Some(external_canon.clone()));
        assert!(resolve_bundle_in(&data_dir, Some(&external_canon), Some(other.to_string_lossy().into_owned())).is_none());

        assert_eq!(route_for_in(&data_dir, &kept), "/r/repo-42-deadbeef");
        assert!(route_for_in(&data_dir, &external).starts_with("/?dir="));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn parses_web_cli_args() {
        let s = |xs: &[&str]| xs.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert_eq!(parse_args(&s(&[])).unwrap(), WebArgs { target: None, open: false, daemon: false });
        assert_eq!(parse_args(&s(&["/tmp/b"])).unwrap(), WebArgs { target: Some("/tmp/b".into()), open: false, daemon: false });
        assert_eq!(parse_args(&s(&["--open", "/tmp/b"])).unwrap(), WebArgs { target: Some("/tmp/b".into()), open: true, daemon: false });
        assert_eq!(parse_args(&s(&["/tmp/b", "--open"])).unwrap(), WebArgs { target: Some("/tmp/b".into()), open: true, daemon: false });
        assert_eq!(parse_args(&s(&["--open"])).unwrap(), WebArgs { target: None, open: true, daemon: false });
        assert_eq!(parse_args(&s(&["--daemon"])).unwrap(), WebArgs { target: None, open: false, daemon: true });
        assert!(parse_args(&s(&["--open", "--daemon"])).is_err());
        assert!(parse_args(&s(&["--bogus"])).is_err());
        assert!(parse_args(&s(&["a", "b"])).is_err());
    }

    #[test]
    fn parses_herdr_pane_from_ls_json() {
        let sample = r#"{"spaces":[{"tabs":[{"terminals":[{"terminal":"t17n","pane":"p23824f"}]}]}]}"#;
        assert_eq!(herdr_pane(sample, "t17n").as_deref(), Some("p23824f"));
        assert_eq!(herdr_pane(sample, "missing"), None);
        assert_eq!(herdr_pane("not json", "t17n"), None);
    }

    #[test]
    fn ping_and_dir_endpoints_answer_over_http() {
        let root = temp_root("http");
        let dist = root.join("dist");
        let data_dir = root.join("data");
        let b1 = data_dir.join("slug-one");
        let b2 = data_dir.join("slug-two");
        std::fs::create_dir_all(&dist).unwrap();
        std::fs::create_dir_all(&b1).unwrap();
        std::fs::create_dir_all(&b2).unwrap();
        std::fs::write(dist.join("index.html"), b"<html>revui</html>").unwrap();

        let server = Server::http("127.0.0.1:0").unwrap();
        let port = server.server_addr().to_ip().unwrap().port();
        let ctx = Arc::new(Context { dist, data_dir: data_dir.clone(), served: None, hosts: [format!("127.0.0.1:{port}"), format!("localhost:{port}")] });
        let handle_ctx = Arc::clone(&ctx);
        let thread = std::thread::spawn(move || {
            for request in server.incoming_requests().take(5) {
                handle(request, &handle_ctx);
            }
        });

        let get = |path: &str| {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
            write!(stream, "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n").unwrap();
            let mut resp = String::new();
            stream.read_to_string(&mut resp).unwrap();
            let (head, body) = resp.split_once("\r\n\r\n").unwrap();
            let status: u16 = head.lines().next().unwrap().split_whitespace().nth(1).unwrap().parse().unwrap();
            (status, body.to_string())
        };

        assert!(ping(port));
        assert_eq!(get("/api/dir?slug=slug-one"), (200, b1.canonicalize().unwrap().to_string_lossy().into_owned()));
        assert_eq!(get("/api/dir?slug=slug-two"), (200, b2.canonicalize().unwrap().to_string_lossy().into_owned()));
        assert_eq!(get("/api/dir?slug=..%2Fdist").0, 404);
        assert_eq!(get("/r/slug-one"), (200, "<html>revui</html>".to_string()));
        thread.join().unwrap();
        let _ = std::fs::remove_dir_all(root);
    }
}
