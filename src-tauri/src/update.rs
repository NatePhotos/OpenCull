//! Finding out that a newer OpenCull exists, and putting it in this one's place.
//!
//! The updater asks GitHub for a
//! small file saying what the latest release is and where its installers
//! are, and sends nothing of its own: not the photographer's name, not a
//! folder, not a photo. Whatever it downloads is refused unless it carries
//! the signature of the key the releases are signed with, so a file from
//! anywhere else is never installed.
//!
//! The check happens here and not in the view, so the view needs no leave to
//! reach the network at all.

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tauri::ipc::Channel;
use tauri_plugin_opener::OpenerExt;
use tauri_plugin_updater::UpdaterExt;

use crate::commands::App;

/// Where releases are read about and downloaded by hand.
const RELEASES: &str = "https://github.com/wispdevon/OpenCull/releases";

/// How long the question "is there a newer version?" may take. It is asked
/// as the app opens, of a server that may not be reachable.
const CHECK_TIMEOUT: Duration = Duration::from_secs(15);

/// How long the download may take. Tens of megabytes, on any connection.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// A newer version than the one running.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Found {
    version: String,
    current: String,
}

/// How far a download has got: bytes so far, and of how many if that is known.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    done: u64,
    total: Option<u64>,
}

/// Asks whether a newer version has been released. `None` when this is it.
#[tauri::command]
pub async fn update_check(handle: tauri::AppHandle) -> Result<Option<Found>, String> {
    let updater = handle.updater_builder().timeout(CHECK_TIMEOUT).build().map_err(|e| e.to_string())?;
    let found = updater.check().await.map_err(|e| e.to_string())?;
    Ok(found.map(|u| Found { version: u.version, current: u.current_version }))
}

/// Downloads the newest version, checks its signature, puts it in place of
/// this one and starts it. Returns only if that could not be done.
#[tauri::command]
pub async fn update_install(handle: tauri::AppHandle, app: tauri::State<'_, Arc<App>>, progress: Channel<Progress>) -> Result<(), String> {
    // The app is about to close. Not in the middle of a card.
    if app.ingesting.load(Ordering::SeqCst) {
        return Err("Photos are being copied off a card. Install the update once that has finished.".into());
    }
    let updater = handle.updater_builder().timeout(DOWNLOAD_TIMEOUT).build().map_err(|e| e.to_string())?;
    let Some(update) = updater.check().await.map_err(|e| e.to_string())? else {
        return Err("This is already the newest version.".into());
    };

    // A rating given a moment ago is written before anything is replaced.
    let writer = app.writer.clone();
    tauri::async_runtime::spawn_blocking(move || writer.flush()).await.map_err(|e| e.to_string())?;

    let mut done = 0u64;
    update
        .download_and_install(
            |chunk, total| {
                done += chunk as u64;
                progress.send(Progress { done, total }).ok();
            },
            || {},
        )
        .await
        .map_err(|e| e.to_string())?;

    // On Windows the installer has already closed the app by now. Elsewhere
    // the new version is in place and this one makes way for it.
    handle.restart()
}

/// Opens the page for one release in the browser, or the list of them all.
/// The address is made here, so the view can only ever open these pages.
#[tauri::command]
pub fn open_release_page(handle: tauri::AppHandle, version: Option<String>) -> Result<(), String> {
    handle.opener().open_url(release_page(version.as_deref()), None::<&str>).map_err(|e| e.to_string())
}

/// The address of one release's page, or of the list when no version is
/// given or what is given is not a version number.
fn release_page(version: Option<&str>) -> String {
    let numbered = |v: &&str| !v.is_empty() && v.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+'));
    match version.filter(numbered) {
        Some(version) => format!("{RELEASES}/tag/v{version}"),
        None => RELEASES.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_release_page_is_ever_opened() {
        assert_eq!(release_page(Some("0.2.5")), "https://github.com/wispdevon/OpenCull/releases/tag/v0.2.5");
        assert_eq!(release_page(None), RELEASES);
        // Whatever the view sends, the address stays on the releases page.
        for odd in ["", "../../evil", "1.0/../..", "x?y=z", "a b", "1.0#frag"] {
            assert_eq!(release_page(Some(odd)), RELEASES, "{odd:?}");
        }
    }
}
