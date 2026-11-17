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

pub fn page(dir: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let read = |name: &str| std::fs::read_to_string(dir.join(name)).map_err(|e| format!("{}: {e}", dir.join(name).display()));
    let review: serde_json::Value = serde_json::from_str(&read("review.json")?)?;
    let patch = read(review["sources"][0]["patch"].as_str().unwrap_or("mr.patch"))?;
    let shown = review["items"].as_array().into_iter().flatten().flat_map(|item| item["evidence"].as_array().into_iter().flatten()).chain([&review["map"]]).filter_map(|evidence| evidence["path"].as_str());
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
    let bundle = serde_json::json!({ "review": review, "patch": patch, "files": files, "metric": metric }).to_string().replace('<', "\\u003c");
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
    }
}
