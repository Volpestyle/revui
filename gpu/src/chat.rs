use eframe::egui;
use std::io::{BufRead, BufReader, Read};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

pub enum Said {
    Me(String),
    Agent(String),
    Tool(String),
    Problem(String),
}

enum Event {
    Said(Said),
    Done,
}

/// The review's agent as a conversation: each message is one run of the `claude` command the config names, on the
/// session the bundle's session.json holds, so the terminal viewer picks the same conversation up.
pub struct Chat {
    pub log: Vec<Said>,
    pub busy: bool,
    pub draft: String,
    events: (Sender<Event>, Receiver<Event>),
    child: Arc<Mutex<Option<Child>>>,
}

impl Default for Chat {
    fn default() -> Self {
        Self { log: Vec::new(), busy: false, draft: String::new(), events: channel(), child: Default::default() }
    }
}

fn uuid() -> String {
    let mut bytes = [0u8; 16];
    let _ = std::fs::File::open("/dev/urandom").and_then(|mut random| random.read_exact(&mut bytes));
    bytes[6] = bytes[6] & 0x0f | 0x40;
    bytes[8] = bytes[8] & 0x3f | 0x80;
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &hex[..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..])
}

/// The session to carry on, as (id, folder), when this bundle's agent has one that can be resumed.
pub fn session(dir: &str) -> Option<(String, String)> {
    let saved: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(std::path::Path::new(dir).join("session.json")).ok()?).ok()?;
    let (id, cwd) = (saved["id"].as_str()?, saved["cwd"].as_str()?);
    (saved["agent"] == "agent" && !id.is_empty() && std::path::Path::new(cwd).is_dir()).then(|| (id.to_string(), cwd.to_string()))
}

impl Chat {
    pub fn take(&mut self) {
        while let Ok(event) = self.events.1.try_recv() {
            match event {
                Event::Said(said) => self.log.push(said),
                Event::Done => self.busy = false,
            }
        }
    }

    pub fn stop(&mut self) {
        if let Some(child) = self.child.lock().unwrap().as_mut() {
            let _ = child.kill();
        }
    }

    /// Sends one message: `prompt` is what the agent is told, `shown` what the conversation shows for it.
    pub fn send(&mut self, ctx: &egui::Context, dir: &str, claude: &str, prompt: String, shown: String) {
        if self.busy {
            return;
        }
        let resumed = session(dir);
        let (id, cwd) = resumed.clone().unwrap_or_else(|| (uuid(), std::env::current_dir().unwrap_or_default().to_string_lossy().into_owned()));
        if resumed.is_none() {
            let saved = serde_json::json!({ "agent": "agent", "handle": "", "id": id, "cwd": cwd });
            let _ = std::fs::write(std::path::Path::new(dir).join("session.json"), saved.to_string());
        }
        let how = if resumed.is_some() { "--resume" } else { "--session-id" };
        let spawned = Command::new("/bin/sh")
            .args(["-c", &format!("exec {claude} \"$@\""), "sh", "-p", &prompt, "--output-format", "stream-json", "--verbose", how, &id])
            .current_dir(&cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();
        self.log.push(Said::Me(shown));
        let mut child = match spawned {
            Ok(child) => child,
            Err(problem) => return self.log.push(Said::Problem(format!("Could not start the agent: {problem}"))),
        };
        let (out, err) = (child.stdout.take(), child.stderr.take());
        *self.child.lock().unwrap() = Some(child);
        self.busy = true;
        let (tell, ctx, held) = (self.events.0.clone(), ctx.clone(), self.child.clone());
        std::thread::spawn(move || {
            let say = |said: Said| {
                let _ = tell.send(Event::Said(said));
                ctx.request_repaint();
            };
            let mut answered = false;
            for line in out.into_iter().flat_map(|out| BufReader::new(out).lines().map_while(Result::ok)) {
                let Ok(message) = serde_json::from_str::<serde_json::Value>(&line) else { continue };
                match message["type"].as_str() {
                    Some("assistant") => {
                        for block in message["message"]["content"].as_array().into_iter().flatten() {
                            match block["type"].as_str() {
                                Some("text") => (answered = true, say(Said::Agent(block["text"].as_str().unwrap_or("").to_string()))).1,
                                Some("tool_use") => {
                                    let input = &block["input"];
                                    let what = ["command", "file_path", "pattern", "description", "url"].iter().find_map(|key| input[*key].as_str()).unwrap_or("");
                                    say(Said::Tool(format!("{} {}", block["name"].as_str().unwrap_or("tool"), what.lines().next().unwrap_or(""))));
                                }
                                _ => {}
                            }
                        }
                    }
                    Some("result") if message["is_error"] == true || !answered => {
                        let text = message["result"].as_str().unwrap_or("").to_string();
                        if !text.is_empty() {
                            say(if message["is_error"] == true { Said::Problem(text) } else { Said::Agent(text) });
                        }
                    }
                    _ => {}
                }
            }
            let mut said = String::new();
            let _ = err.map(|mut err| err.read_to_string(&mut said));
            let ended = held.lock().unwrap().take().and_then(|mut child| child.wait().ok());
            if let (false, Some(line)) = (ended.is_some_and(|status| status.success()), said.lines().rev().find(|line| !line.trim().is_empty())) {
                say(Said::Problem(line.trim().to_string()));
            }
            let _ = tell.send(Event::Done);
            ctx.request_repaint();
        });
    }
}
