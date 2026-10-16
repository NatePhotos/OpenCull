//! Saved card sessions live on this computer, never beside source photos.
use serde_json::Value;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
};
use tauri::Manager;
static WRITING: Mutex<()> = Mutex::new(());
fn path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("sessions/latest.json"))
}
fn save(path: &Path, value: &Value) -> Result<(), String> {
    let parent = path.parent().ok_or("No session directory")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let tmp = parent.join(".latest.tmp");
    let result = (|| {
        let mut options = fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&tmp).map_err(|e| e.to_string())?;
        file.write_all(&serde_json::to_vec(value).map_err(|e| e.to_string())?)
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;
        fs::rename(&tmp, path).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(tmp);
    }
    result
}
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
fn read(path: &Path) -> Result<Option<Value>, String> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|e| format!("Saved session could not be read: {e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}
fn save_history(latest: &Path, value: &Value) -> Result<(), String> {
    let id = value
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| valid_id(id) && *id != "latest")
        .ok_or("Invalid session ID")?;
    if value.get("version").and_then(Value::as_u64) != Some(1)
        || !value.get("items").is_some_and(Value::is_array)
    {
        return Err("Invalid session".into());
    }
    save(&latest.with_file_name(format!("{id}.json")), value)?;
    save(latest, value)
}
fn history(latest: &Path) -> Result<Vec<Value>, String> {
    let parent = latest.parent().ok_or("No session directory")?;
    let entries = match fs::read_dir(parent) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.to_string()),
    };
    let mut files: Vec<_> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                return None;
            }
            let modified = entry
                .metadata()
                .ok()?
                .modified()
                .ok()?
                .duration_since(std::time::UNIX_EPOCH)
                .ok()?
                .as_millis() as u64;
            Some((path, modified))
        })
        .collect();
    files.sort_by(|a, b| b.1.cmp(&a.1));
    let mut summaries = Vec::new();
    for (file, modified) in files {
        let Ok(Some(value)) = read(&file) else {
            continue;
        };
        if value["version"] != 1 || !value["items"].is_array() || !value["sources"].is_array() {
            continue;
        }
        let id = value["id"]
            .as_str()
            .filter(|id| valid_id(id))
            .unwrap_or("latest");
        if summaries.iter().any(|v: &Value| v["id"] == id) {
            continue;
        }
        summaries.push(serde_json::json!({
            "id": id, "name": value["name"].as_str().unwrap_or("Card session"),
            "sources": value["sources"], "count": value["items"].as_array().unwrap().len(),
            "updatedAt": value["updatedAt"].as_u64().unwrap_or(modified),
            "stage": value["stage"].as_str().unwrap_or("review"), "cover": value["items"][0],
        }));
        if summaries.len() == 24 {
            break;
        }
    }
    summaries.sort_by(|a, b| b["updatedAt"].as_u64().cmp(&a["updatedAt"].as_u64()));
    Ok(summaries)
}
fn remove_history(latest: &Path, ids: &[String]) -> Result<(), String> {
    if ids.iter().any(|id| !valid_id(id)) {
        return Err("Invalid session ID".into());
    }
    let current = read(latest)?;
    let remove_latest = current.as_ref().is_some_and(|v| {
        ids.iter()
            .any(|id| id == v["id"].as_str().unwrap_or("latest"))
    });
    for id in ids {
        let file = latest.with_file_name(format!("{id}.json"));
        match fs::remove_file(file) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    if remove_latest {
        match fs::remove_file(latest) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.to_string()),
        }
        if let Some(next) = history(latest)?.first() {
            let id = next["id"].as_str().ok_or("Invalid saved session")?;
            if let Some(value) = read(&latest.with_file_name(format!("{id}.json")))? {
                save(latest, &value)?;
            }
        }
    }
    Ok(())
}
#[tauri::command]
pub fn review_remove(app: tauri::AppHandle, ids: Vec<String>) -> Result<(), String> {
    let _lock = WRITING.lock().map_err(|e| e.to_string())?;
    remove_history(&path(&app)?, &ids)
}
#[tauri::command]
pub fn review_save(app: tauri::AppHandle, value: Value) -> Result<(), String> {
    let _lock = WRITING.lock().map_err(|e| e.to_string())?;
    save_history(&path(&app)?, &value)
}
#[tauri::command]
pub fn review_load(app: tauri::AppHandle, id: Option<String>) -> Result<Option<Value>, String> {
    let _lock = WRITING.lock().map_err(|e| e.to_string())?;
    let latest = path(&app)?;
    if let Some(id) = id {
        if !valid_id(&id) {
            return Err("Invalid session ID".into());
        }
        let file = latest.with_file_name(format!("{id}.json"));
        if let Some(value) = read(&file)? {
            return Ok(Some(value));
        }
        let legacy = read(&latest)?;
        return Ok(legacy.filter(|v| v["id"] == id));
    }
    read(&latest)
}
#[tauri::command]
pub async fn review_list(app: tauri::AppHandle) -> Result<Vec<Value>, String> {
    let latest = path(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let _lock = WRITING.lock().map_err(|e| e.to_string())?;
        history(&latest)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn removing_sessions_updates_latest_without_touching_source_files() {
        let root = std::env::temp_dir().join(format!("opencull-remove-{}", std::process::id()));
        let latest = root.join("sessions/latest.json");
        fs::create_dir_all(&root).unwrap();
        let source = root.join("original.raw");
        fs::write(&source, b"original photo").unwrap();
        for (id, updated) in [("a", 1), ("b", 2), ("c", 3)] {
            save_history(&latest, &serde_json::json!({"version":1,"id":id,"sources":[source],"items":[{"path":source}],"updatedAt":updated})).unwrap();
        }
        assert!(remove_history(&latest, &["../original".into(), "b".into()]).is_err());
        assert!(root.join("sessions/b.json").exists());
        remove_history(&latest, &["a".into()]).unwrap();
        assert_eq!(read(&latest).unwrap().unwrap()["id"], "c");
        remove_history(&latest, &["c".into()]).unwrap();
        assert_eq!(read(&latest).unwrap().unwrap()["id"], "b");
        assert_eq!(history(&latest).unwrap().len(), 1);
        remove_history(&latest, &["b".into()]).unwrap();
        assert!(read(&latest).unwrap().is_none());
        assert!(history(&latest).unwrap().is_empty());
        assert_eq!(fs::read(&source).unwrap(), b"original photo");
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn removing_legacy_session_clears_latest_alias() {
        let root =
            std::env::temp_dir().join(format!("opencull-remove-legacy-{}", std::process::id()));
        let latest = root.join("latest.json");
        save(
            &latest,
            &serde_json::json!({"version":1,"sources":[],"items":[]}),
        )
        .unwrap();
        remove_history(&latest, &["latest".into()]).unwrap();
        assert!(history(&latest).unwrap().is_empty());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn history_retains_independent_sessions_and_updates_them() {
        let root = std::env::temp_dir().join(format!("opencull-history-{}", std::process::id()));
        let latest = root.join("latest.json");
        for (id, updated) in [("a", 1), ("b", 2), ("a", 3)] {
            save_history(&latest, &serde_json::json!({"version":1,"id":id,"name":id,"sources":["/card"],"items":[],"updatedAt":updated})).unwrap();
        }
        let list = history(&latest).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0]["id"], "a");
        assert_eq!(list[1]["id"], "b");
        assert_eq!(read(&root.join("b.json")).unwrap().unwrap()["id"], "b");
        assert!(save_history(
            &latest,
            &serde_json::json!({"version":1,"id":"../outside","items":[]})
        )
        .is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn legacy_latest_session_is_listed_without_rewriting_it() {
        let root =
            std::env::temp_dir().join(format!("opencull-history-legacy-{}", std::process::id()));
        let latest = root.join("latest.json");
        save(
            &latest,
            &serde_json::json!({"version":1,"id":"old","sources":["/card"],"items":[]}),
        )
        .unwrap();
        let before = fs::read(&latest).unwrap();
        assert_eq!(history(&latest).unwrap()[0]["id"], "old");
        assert_eq!(fs::read(&latest).unwrap(), before);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn session_is_stored_separately_and_replaced_whole() {
        let root = std::env::temp_dir().join(format!("opencull-review-{}", std::process::id()));
        let path = root.join("sessions/latest.json");
        save(&path, &serde_json::json!({"version":1,"items":[]})).unwrap();
        save(&path, &serde_json::json!({"version":1,"items":["chosen"]})).unwrap();
        let read: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(read["items"][0], "chosen");
        assert!(!root.join("sessions/.latest.tmp").exists());
        fs::remove_dir_all(root).unwrap();
    }
}
