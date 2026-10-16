//! Native preferences. File writes are serialized and replace atomically.
use std::{collections::BTreeMap, fs, io::Write, path::{Path, PathBuf}, sync::{Mutex, atomic::{AtomicU64, Ordering}}};
use serde_json::Value;
use tauri::Manager;
type Values = BTreeMap<String, Value>;
static NEXT: AtomicU64 = AtomicU64::new(0);
pub struct Settings(pub Mutex<PathBuf>);

#[cfg(target_os = "linux")]
pub fn linux_path(xdg: Option<&Path>, home: &Path) -> PathBuf {
    xdg.filter(|p| p.is_absolute()).map(Path::to_path_buf).unwrap_or_else(|| home.join(".config")).join("opencull/config.json")
}

pub fn location(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    #[cfg(target_os = "linux")]
    {
        let home = app.path().home_dir().map_err(|e| e.to_string())?;
        Ok(linux_path(std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).as_deref(), &home))
    }
    #[cfg(not(target_os = "linux"))]
    { Ok(app.path().app_config_dir().map_err(|e| e.to_string())?.join("config.json")) }
}

fn read(path: &Path) -> Result<Values, String> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}; file left untouched", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Values::new()),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

fn write(path: &Path, values: &Values) -> Result<(), String> {
    let parent = path.parent().ok_or("Settings path has no parent")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let temp = parent.join(format!(".config-{}-{}.tmp", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
    let result = (|| {
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)] { use std::os::unix::fs::OpenOptionsExt; options.mode(0o600); }
        let mut file = options.open(&temp).map_err(|e| e.to_string())?;
        let bytes = serde_json::to_vec_pretty(values).map_err(|e| e.to_string())?;
        file.write_all(&bytes).and_then(|_| file.sync_all()).map_err(|e| e.to_string())?;
        fs::rename(&temp, path).map_err(|e| e.to_string())
    })();
    if result.is_err() { let _ = fs::remove_file(&temp); }
    result
}

pub fn migrate(path: &Path, legacy: Values) -> Result<Values, String> {
    let mut values = read(path)?;
    for (key, value) in legacy { values.entry(key).or_insert(value); }
    write(path, &values)?;
    Ok(values)
}

#[tauri::command]
pub fn config_load(settings: tauri::State<Settings>, legacy: Values) -> Result<Values, String> {
    let path = settings.0.lock().map_err(|e| e.to_string())?;
    migrate(&path, legacy)
}

#[tauri::command]
pub fn config_set(settings: tauri::State<Settings>, key: String, value: Value) -> Result<(), String> {
    if key.is_empty() || key.len() > 256 { return Err("Invalid preference key".into()); }
    let path = settings.0.lock().map_err(|e| e.to_string())?;
    let mut values = read(&path)?;
    values.insert(key, value);
    write(&path, &values)
}

fn merge_values(path: &Path, incoming: Values) -> Result<(), String> {
    if incoming.len() > 128 || incoming.keys().any(|key| key.is_empty() || key.len() > 256) {
        return Err("Invalid settings import".into());
    }
    if serde_json::to_vec(&incoming).map_err(|e| e.to_string())?.len() > 1024 * 1024 {
        return Err("Settings import exceeds 1 MB".into());
    }
    let mut values = read(path)?;
    values.extend(incoming);
    write(path, &values)
}

#[tauri::command]
pub fn config_set_many(settings: tauri::State<Settings>, values: Values) -> Result<(), String> {
    let path = settings.0.lock().map_err(|e| e.to_string())?;
    merge_values(&path, values)
}

#[tauri::command]
pub async fn config_export_file(app: tauri::AppHandle, contents: String) -> Result<bool, String> {
    use tauri_plugin_dialog::DialogExt;
    if contents.len() > 1024 * 1024 { return Err("Settings export exceeds 1 MB".into()); }
    let document: Values = serde_json::from_str(&contents).map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        let Some(file) = app.dialog().file().set_title("Export OpenCull settings")
            .add_filter("OpenCull settings", &["json"]).set_file_name("opencull-settings.json").blocking_save_file() else { return Ok(false); };
        let path = file.into_path().map_err(|e| e.to_string())?;
        if path.extension().and_then(|s| s.to_str()).map(|s| s.eq_ignore_ascii_case("json")) != Some(true) {
            return Err("Choose a .json filename".into());
        }
        let active = location(&app)?;
        let same_file = path.canonicalize().ok().zip(active.canonicalize().ok()).is_some_and(|(a, b)| a == b);
        if path == active || same_file { return Err("Export to a separate file, not the active config.json".into()); }
        write(&path, &document)?;
        Ok(true)
    }).await.map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    fn root() -> PathBuf { std::env::temp_dir().join(format!("opencull-config-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed))) }
    #[cfg(target_os = "linux")]
    #[test] fn xdg_paths() {
        assert_eq!(linux_path(Some(Path::new("/custom")), Path::new("/home/a")), Path::new("/custom/opencull/config.json"));
        assert_eq!(linux_path(None, Path::new("/home/a")), Path::new("/home/a/.config/opencull/config.json"));
        assert_eq!(linux_path(Some(Path::new("relative")), Path::new("/home/a")), linux_path(None, Path::new("/home/a")));
    }
    #[test] fn migration_preserves_existing_and_unknown_values() {
        let dir = root(); let path = dir.join("nested/config.json");
        let first = Values::from([("theme".into(), Value::from("grey")), ("future".into(), Value::from(42))]);
        migrate(&path, first).unwrap();
        let result = migrate(&path, Values::from([("theme".into(), Value::from("dark")), ("font".into(), Value::from("system"))])).unwrap();
        assert_eq!(result["theme"], "grey"); assert_eq!(result["future"], 42); assert_eq!(result["font"], "system");
        assert_eq!(read(&path).unwrap(), result);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test] fn malformed_file_is_never_overwritten() {
        let dir = root(); fs::create_dir_all(&dir).unwrap(); let path = dir.join("config.json");
        fs::write(&path, b"{broken").unwrap(); assert!(migrate(&path, Values::new()).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"{broken"); fs::remove_dir_all(dir).unwrap();
    }
    #[test] fn failed_write_keeps_previous_config() {
        let dir = root(); fs::create_dir_all(&dir).unwrap(); let path = dir.join("config.json");
        write(&path, &Values::new()).unwrap();
        assert!(write(&path.join("bad"), &Values::new()).is_err()); assert_eq!(read(&path).unwrap(), Values::new());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test] fn bulk_import_merges_preferences_and_preserves_history() {
        let dir = root(); let path = dir.join("config.json");
        write(&path, &Values::from([("theme".into(), Value::from("dark")), ("recents".into(), serde_json::json!(["/photos"]))])).unwrap();
        merge_values(&path, Values::from([("theme".into(), Value::from("light")), ("showExif".into(), Value::from(true))])).unwrap();
        let values = read(&path).unwrap();
        assert_eq!(values["theme"], "light"); assert_eq!(values["showExif"], true);
        assert_eq!(values["recents"], serde_json::json!(["/photos"]));
        fs::remove_dir_all(dir).unwrap();
    }
    #[test] fn invalid_bulk_import_preserves_the_whole_config() {
        let dir = root(); let path = dir.join("config.json");
        write(&path, &Values::from([("theme".into(), Value::from("dark"))])).unwrap();
        let before = fs::read(&path).unwrap();
        assert!(merge_values(&path, Values::from([("".into(), Value::from(true)), ("theme".into(), Value::from("light"))])).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
        fs::remove_dir_all(dir).unwrap();
    }
}
