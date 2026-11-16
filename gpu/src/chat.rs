#[cfg(not(target_arch = "wasm32"))]
pub use native::*;
#[cfg(target_arch = "wasm32")]
pub use web::*;

/// One thing the agent did with a tool: what it ran, and what came back once it has.
pub struct Step {
    pub id: String,
    pub name: String,
    pub what: String,
    pub input: String,
    pub result: Option<(String, bool)>,
}

pub enum Said {
    Me(String),
    Agent(String),
    Tool(Step),
    Problem(String),
}

#[cfg(target_arch = "wasm32")]
mod web {
    use super::Said;
    use eframe::egui;

    /// In a browser there is no process to start: the message goes to an agent in a pane of the app the page sits in,
    /// through `revui agent`, and the conversation carries on there.
    #[derive(Default)]
    pub struct Chat {
        pub log: Vec<Said>,
        pub busy: bool,
        pub draft: String,
    }

    pub fn session(_dir: &str) -> Option<(String, String)> {
        None
    }

    impl Chat {
        pub fn take(&mut self) {}

        pub fn sync(&mut self, _dir: &str) {}

        pub fn stop(&mut self) {}

        pub fn send(&mut self, _ctx: &egui::Context, dir: &str, _claude: &str, _folder: &str, about: (&str, Option<&str>), _prompt: String, shown: String) {
            let args: Vec<&str> = ["agent", dir, about.0].into_iter().chain(about.1).collect();
            self.log.push(Said::Me(shown));
            self.log.push(match crate::host::run(&args, None) {
                Ok(note) => Said::Agent(note.trim().to_string()),
                Err(problem) => Said::Problem(problem),
            });
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::{Said, Step};
    use eframe::egui;
    use std::io::{BufRead, BufReader, Read};
    use std::process::{Child, Command, Stdio};
    use std::sync::mpsc::{Receiver, Sender, channel};
    use std::sync::{Arc, Mutex};

    enum Event {
        Line(serde_json::Value),
        Problem(String),
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
        read: Option<std::time::SystemTime>,
    }

    impl Default for Chat {
        fn default() -> Self {
            Self { log: Vec::new(), busy: false, draft: String::new(), events: channel(), child: Default::default(), read: None }
        }
    }

    /// What one line of Claude Code's stream, or of the transcript it keeps of a session, adds to the conversation.
    /// The transcript also holds what the person asked; the stream does not, so `asked` says whether to take it.
    fn add(log: &mut Vec<Said>, line: &serde_json::Value, asked: bool) {
        let content = &line["message"]["content"];
        let short = |text: &str| text.char_indices().nth(6000).map_or(text.to_string(), |(at, _)| format!("{}\n…", &text[..at]));
        match (line["type"].as_str(), content.as_str()) {
            (Some("user"), Some(text)) if asked && !text.trim_start().starts_with('<') => log.push(Said::Me(text.trim().to_string())),
            (Some("user"), None) => {
                for block in content.as_array().into_iter().flatten() {
                    match block["type"].as_str() {
                        Some("text") if asked && !block["text"].as_str().unwrap_or("<").trim_start().starts_with('<') => log.push(Said::Me(block["text"].as_str().unwrap_or("").trim().to_string())),
                        Some("tool_result") => {
                            let said = block["content"].as_str().map(String::from).unwrap_or_else(|| block["content"].as_array().into_iter().flatten().filter_map(|part| part["text"].as_str()).collect::<Vec<_>>().join("\n"));
                            let step = log.iter_mut().rev().find_map(|said| match said {
                                Said::Tool(step) if block["tool_use_id"] == step.id.as_str() => Some(step),
                                _ => None,
                            });
                            if let Some(step) = step {
                                step.result = Some((short(said.trim()), block["is_error"] == true));
                            }
                        }
                        _ => {}
                    }
                }
            }
            (Some("assistant"), _) => {
                for block in content.as_array().into_iter().flatten() {
                    match block["type"].as_str() {
                        Some("text") if !block["text"].as_str().unwrap_or("").trim().is_empty() => log.push(Said::Agent(block["text"].as_str().unwrap_or("").trim().to_string())),
                        Some("tool_use") => {
                            let input = &block["input"];
                            let what = ["command", "file_path", "pattern", "description", "url", "prompt", "skill"].iter().find_map(|key| input[*key].as_str()).unwrap_or("");
                            let whole = input["command"].as_str().map(String::from).unwrap_or_else(|| serde_json::to_string_pretty(input).unwrap_or_default());
                            log.push(Said::Tool(Step { id: block["id"].as_str().unwrap_or("").to_string(), name: block["name"].as_str().unwrap_or("tool").to_string(), what: what.lines().next().unwrap_or("").to_string(), input: short(&whole), result: None }));
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    /// Where Claude Code keeps the transcript of this bundle's session.
    // ponytail: looks under $CLAUDE_CONFIG_DIR, then ~/.claude; a `claude` command that keeps its sessions elsewhere shows only what this window streamed.
    fn transcript(dir: &str) -> Option<std::path::PathBuf> {
        let (id, cwd) = session(dir)?;
        let slug: String = cwd.chars().map(|letter| if letter.is_ascii_alphanumeric() { letter } else { '-' }).collect();
        let homes = [std::env::var("CLAUDE_CONFIG_DIR").ok().map(std::path::PathBuf::from), std::env::var("HOME").ok().map(|home| std::path::Path::new(&home).join(".claude"))];
        homes.into_iter().flatten().map(|home| home.join("projects").join(&slug).join(format!("{id}.jsonl"))).find(|path| path.is_file())
    }

    fn uuid() -> String {
        let mut bytes = [0u8; 16];
        let _ = std::fs::File::open("/dev/urandom").and_then(|mut random| random.read_exact(&mut bytes));
        bytes[6] = bytes[6] & 0x0f | 0x40;
        bytes[8] = bytes[8] & 0x3f | 0x80;
        let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        format!("{}-{}-{}-{}-{}", &hex[..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..])
    }

    /// The session to carry on, as (id, folder), when this bundle's agent has one that can be resumed: the one this
    /// window started, or the one a pane of the person's terminal is carrying.
    pub fn session(dir: &str) -> Option<(String, String)> {
        let saved: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(std::path::Path::new(dir).join("session.json")).ok()?).ok()?;
        let (id, cwd) = (saved["id"].as_str()?, saved["cwd"].as_str()?);
        (!id.is_empty() && std::path::Path::new(cwd).is_dir()).then(|| (id.to_string(), cwd.to_string()))
    }

    impl Chat {
        pub fn take(&mut self) {
            while let Ok(event) = self.events.1.try_recv() {
                match event {
                    Event::Line(line) if line["type"] == "result" => {
                        let (text, failed) = (line["result"].as_str().unwrap_or("").to_string(), line["is_error"] == true);
                        let answered = self.log.iter().rev().take_while(|said| !matches!(said, Said::Me(_))).any(|said| matches!(said, Said::Agent(_)));
                        if !text.is_empty() && (failed || !answered) {
                            self.log.push(if failed { Said::Problem(text) } else { Said::Agent(text) });
                        }
                    }
                    Event::Line(line) => add(&mut self.log, &line, false),
                    Event::Problem(problem) => self.log.push(Said::Problem(problem)),
                    Event::Done => self.busy = false,
                }
            }
        }

        /// Shows the session as Claude Code has it on disk, whenever that changes: what was said before this window
        /// opened, and what a terminal carrying on the same session has done since.
        pub fn sync(&mut self, dir: &str) {
            let Some(path) = transcript(dir).filter(|_| !self.busy) else { return };
            let stamp = std::fs::metadata(&path).and_then(|file| file.modified()).ok();
            if stamp.is_none() || stamp == self.read {
                return;
            }
            self.read = stamp;
            let mut log = Vec::new();
            for line in std::fs::read_to_string(&path).unwrap_or_default().lines().filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok()) {
                if line["isSidechain"] != true && line["isMeta"] != true {
                    add(&mut log, &line, true);
                }
            }
            let failed = self.log.drain(..).rev().take_while(|said| matches!(said, Said::Problem(_))).collect::<Vec<_>>();
            self.log = log;
            self.log.extend(failed.into_iter().rev());
        }

        pub fn stop(&mut self) {
            if let Some(child) = self.child.lock().unwrap().as_mut() {
                let _ = child.kill();
            }
        }

        /// Sends one message: `prompt` is what the agent is told, `shown` what the conversation shows for it.
        pub fn send(&mut self, ctx: &egui::Context, dir: &str, claude: &str, folder: &str, _about: (&str, Option<&str>), prompt: String, shown: String) {
            if self.busy {
                return;
            }
            let resumed = session(dir);
            let (id, cwd) = resumed.clone().unwrap_or_else(|| (uuid(), folder.to_string()));
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
                let say = |event: Event| {
                    let _ = tell.send(event);
                    ctx.request_repaint();
                };
                for line in out.into_iter().flat_map(|out| BufReader::new(out).lines().map_while(Result::ok)) {
                    if let Ok(line) = serde_json::from_str::<serde_json::Value>(&line) {
                        say(Event::Line(line));
                    }
                }
                let mut said = String::new();
                let _ = err.map(|mut err| err.read_to_string(&mut said));
                let ended = held.lock().unwrap().take().and_then(|mut child| child.wait().ok());
                if let (false, Some(line)) = (ended.is_some_and(|status| status.success()), said.lines().rev().find(|line| !line.trim().is_empty())) {
                    say(Event::Problem(line.trim().to_string()));
                }
                say(Event::Done);
            });
        }
    }

}
