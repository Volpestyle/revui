//! `revui html`: a guided review as one page, guide.html, that any browser opens with no revui and no server: a
//! pipeline keeps it as an artifact, or it is sent as a file. The page (guide.html beside this file) builds itself
//! from what is put into it here: review.json, the patch, each picture as a data address and each diagram's source.

use base64::Engine;
use std::path::{Path, PathBuf};

fn kind(path: &str) -> Option<&'static str> {
    Some(match Path::new(path).extension()?.to_str()?.to_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        _ => return None,
    })
}

/// What a run of the change in a sandbox did, when a pipeline left its record in the bundle (verify.json): its
/// steps and its checks, each step with the end of its log (<step>.log beside it) as proof.
pub fn verify(dir: &Path) -> Option<serde_json::Value> {
    let mut run: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("verify.json")).ok()?).ok()?;
    for step in run["steps"].as_array_mut().into_iter().flatten() {
        let name = step["step"].as_str().unwrap_or_default();
        let log = Some(name).filter(|name| !name.is_empty() && !name.contains(['/', '\\'])).and_then(|name| std::fs::read_to_string(dir.join(format!("{name}.log"))).ok()).unwrap_or_default();
        let lines: Vec<&str> = log.trim_end().lines().collect();
        let earlier = lines.len().saturating_sub(60);
        step["log"] = format!("{}{}", if earlier > 0 { format!("… {earlier} earlier lines\n") } else { String::new() }, lines[earlier..].join("\n")).into();
    }
    Some(run)
}

pub fn page(dir: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let read = |name: &str| std::fs::read_to_string(dir.join(name)).map_err(|e| format!("{}: {e}", dir.join(name).display()));
    let review: serde_json::Value = serde_json::from_str(&read("review.json")?)?;
    let patch = read(review["sources"][0]["patch"].as_str().unwrap_or("mr.patch"))?;
    let verify = verify(dir);
    let ran = verify.iter().flat_map(|run| run["media"].as_array().into_iter().flatten());
    let shown = review["items"].as_array().into_iter().flatten().flat_map(|item| item["evidence"].as_array().into_iter().flatten()).chain([&review["map"]]).chain(ran).filter_map(|evidence| evidence["path"].as_str());
    let mut files = serde_json::Map::new();
    // Only what the review names, and only from inside the bundle: a path that climbs out of it is left out.
    let inside = |path: &str| Some(dir.join(path).canonicalize().ok()?).filter(|full| dir.canonicalize().is_ok_and(|root| full.starts_with(root)));
    for path in shown {
        let Some(full) = inside(path) else { continue };
        let held = match kind(path) {
            Some(kind) => format!("data:{kind};base64,{}", base64::engine::general_purpose::STANDARD.encode(std::fs::read(&full)?)),
            None => String::from("beside the page"),
        };
        files.insert(path.into(), held.into());
        let source = Path::new(path).with_extension("mmd").to_string_lossy().into_owned();
        if let Some(drawn) = inside(&source).and_then(|full| std::fs::read_to_string(full).ok()) {
            files.insert(source, drawn.into());
        }
    }
    // What the run that wrote the guide cost, when the job that ran it left that beside the review.
    let metric = std::fs::read_to_string(dir.join("guide-metric.json")).unwrap_or_default();
    let bundle = serde_json::json!({ "review": review, "patch": patch, "files": files, "metric": metric, "verify": verify }).to_string().replace('<', "\\u003c");
    Ok(include_str!("guide.html").replacen("/*BUNDLE*/", &bundle, 1))
}

pub fn write(dir: &Path, out: Option<&String>) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let out = out.map_or_else(|| dir.join("guide.html"), PathBuf::from);
    std::fs::write(&out, page(dir)?)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_page_holds_the_review_and_nothing_that_ends_its_script() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/demo");
        let page = super::page(&dir).unwrap();
        let held = page.split_once("type=\"application/json\">").unwrap().1.split_once("</script>").unwrap().0;
        let bundle: serde_json::Value = serde_json::from_str(held).unwrap();
        assert!(bundle["review"]["title"].is_string() && bundle["patch"].as_str().unwrap().contains("diff --git"));
        assert!(!held.contains('<'), "nothing in the review can close the block it sits in");
        assert!(bundle["verify"].is_null(), "no sandbox run, no verification card");
    }

    #[test]
    fn a_sandbox_run_carries_the_end_of_each_step_log() {
        let dir = std::env::temp_dir().join(format!("revui-verify-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("verify.json"), r#"{"steps":[{"step":"seed","ok":true,"seconds":3},{"step":"../out","ok":false,"seconds":1}]}"#).unwrap();
        std::fs::write(dir.join("seed.log"), (1..=70).map(|n| format!("line {n}\n")).collect::<String>()).unwrap();
        let run = super::verify(&dir).unwrap();
        let log = run["steps"][0]["log"].as_str().unwrap();
        assert!(log.starts_with("… 10 earlier lines\nline 11\n") && log.ends_with("line 70"));
        assert_eq!(run["steps"][1]["log"], "", "a step named to climb out of the bundle reads nothing");
        let _ = std::fs::remove_dir_all(dir);
    }
}
