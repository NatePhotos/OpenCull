//! OpenCull.
//!
//! The Rust half: finds the previews inside RAW files, turns them into
//! pictures a web view can show, reads and writes ratings, and decides what
//! order to do it all in. The interface is in `../src`.

pub mod ai;
pub mod burst;
pub mod cache;
pub mod commands;
pub mod editors;
pub mod embed;
pub mod faces;
pub mod framing;
pub mod graphics;
pub mod ingest;
pub mod multicard;
pub mod pool;
pub mod preview;
pub mod raw;
pub mod review;
pub mod scan;
pub mod serve;
pub mod sidecar;
pub mod settings;
pub mod times;
pub mod transfer;
pub mod update;
pub mod xmp;
#[cfg(target_os = "linux")]
pub mod video_stream;

use std::sync::Arc;

use tauri::{Emitter, Manager, RunEvent};

use cache::Cache;
use commands::{App, Launch};
use serve::Previews;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let graphics = graphics::initialize();

    if ai::acceleration::requested() {
        eprintln!("[ai] CUDA requested; one shared model worker, CPU fallback on failure");
    }
    let launch = Launch::from_args();
    let mut builder = tauri::Builder::default().plugin(tauri_plugin_opener::init()).plugin(tauri_plugin_dialog::init()).plugin(tauri_plugin_updater::Builder::new().build());
    if !launch.self_test {
        // The window reopens where it was left, at the size it was left.
        // Whether it is showing is not part of that: it starts hidden and is
        // shown below once there is something behind it. A self-test's
        // one-point window is neither restored nor remembered.
        use tauri_plugin_window_state::StateFlags;
        builder = builder.plugin(tauri_plugin_window_state::Builder::default().with_state_flags(StateFlags::all() & !StateFlags::VISIBLE).build());
    }
    builder
        .manage(graphics)
        .setup(move |app| {
            // The dev page is HTTP, while previews use a custom scheme.
            // WebKitGTK otherwise rejects those cross-origin image requests.
            #[cfg(all(target_os = "linux", dev))]
            app.get_webview_window("main").expect("main window").with_webview(|webview| {
                use webkit2gtk::WebViewExt;
                webview.inner().set_cors_allowlist(&["opencull://localhost/*"]);
            })?;
            #[cfg(target_os = "linux")]
            if graphics.effective != graphics::Profile::Auto {
                app.get_webview_window("main").expect("main window").with_webview(move |webview| {
                    use webkit2gtk::{SettingsExt, WebViewExt};
                    use webkit2gtk::glib::prelude::ObjectExt;
                    if let Some(settings) = webview.inner().settings() {
                        let policy = if graphics.effective == graphics::Profile::Software { webkit2gtk::HardwareAccelerationPolicy::Never } else { webkit2gtk::HardwareAccelerationPolicy::Always };
                        settings.set_hardware_acceleration_policy(policy);
                        if graphics.effective == graphics::Profile::Nvidia {
                            settings.set_enable_webgl(true);
                            if settings.find_property("enable-2d-canvas-acceleration").is_some() { settings.set_property("enable-2d-canvas-acceleration", true); }
                        }
                    }
                })?;
            }
            // A self-test keeps its previews apart, so a run measures making
            // them rather than finding the last run's.
            let (dir, times_dir, faces_dir) = if launch.self_test {
                let root = std::env::temp_dir().join(format!("opencull-selftest-{}", std::process::id()));
                (root.join("previews"), root.join("times"), root.join("faces"))
            } else {
                let root = app.path().app_cache_dir()?;
                (root.join("previews"), root.join("times"), root.join("faces"))
            };
            let self_test_root = dir.parent().map(|p| p.to_path_buf());
            let cache = Cache::new(dir);

            if launch.self_test {
                // No dock icon and no stolen focus: this is a check running in
                // the background of someone's working day.
                #[cfg(target_os = "macos")]
                app.set_activation_policy(tauri::ActivationPolicy::Accessory);

                // But not no window at all. A web view in a hidden window has
                // its scripts suspended a few seconds after it loads —
                // MEASURED: every run froze at the same point, with the
                // backend idle and its work finished, waiting on an interface
                // that had stopped listening. So the window is shown where it
                // counts as on screen and cannot be noticed or clicked: one
                // point square, in a corner, beneath every other window, and
                // unable to take the keyboard.
                if let Some(window) = app.get_webview_window("main") {
                    window.set_decorations(false).ok();
                    window.set_min_size(None::<tauri::LogicalSize<f64>>).ok();
                    window.set_size(tauri::LogicalSize::new(1.0, 1.0)).ok();
                    window.set_position(tauri::LogicalPosition::new(0.0, 0.0)).ok();
                    window.set_focusable(false).ok();
                    window.set_always_on_bottom(true).ok();
                    window.set_skip_taskbar(true).ok();
                    window.show().ok();
                }
                // And never left running if something inside it hangs — nor
                // its previews left behind if it does.
                let abandoned = self_test_root.clone().unwrap_or_else(|| cache.dir().to_path_buf());
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_secs(90));
                    println!("[selftest] FAILED: gave up after 90 seconds");
                    std::fs::remove_dir_all(&abandoned).ok();
                    std::process::exit(2);
                });
            } else if let Some(window) = app.get_webview_window("main") {
                // The window starts hidden and is shown here, once there is
                // something behind it, rather than appearing white and then
                // painting.
                window.show().ok();
            }
            app.manage(settings::Settings(std::sync::Mutex::new(settings::location(app.handle())?)));
            app.manage(launch);

            // Bring the cache back under its limit, well away from startup.
            let trimming = cache.clone();
            std::thread::Builder::new()
                .name("opencull-cache-trim".into())
                .spawn(move || {
                    std::thread::sleep(std::time::Duration::from_secs(20));
                    trimming.trim(trimming.limit());
                })
                .ok();

            let state = Arc::new(App::new(Previews::new(cache), times::Times::new(times_dir), ai::store::Store::new(faces_dir)));
            // A rating that could not be written into a JPEG is still in its
            // sidecar, but other programs will not see it, and that is said.
            let handle = app.handle().clone();
            state.writer.on_failure(move |path, why, what| {
                let event = match what {
                    xmp::Wrote::Marks => "marks-not-saved",
                    xmp::Wrote::Jpeg => "jpeg-not-written",
                    xmp::Wrote::Framing => "framing-not-saved",
                    xmp::Wrote::JpegCrop => "crop-not-in-jpeg",
                };
                handle.emit(event, (path.to_string_lossy().into_owned(), why.to_string())).ok();
            });
            app.manage(state);
            Ok(())
        })
        .register_asynchronous_uri_scheme_protocol("opencull", |ctx, request, responder| {
            match ctx.app_handle().try_state::<Arc<App>>() {
                Some(app) => serve::handle(&app.previews, request, responder),
                // Asked before setup finished. The view retries.
                None => responder.respond(
                    tauri::http::Response::builder()
                        .status(tauri::http::StatusCode::SERVICE_UNAVAILABLE)
                        .body(Vec::new())
                        .expect("fixed response"),
                ),
            }
        })
        .invoke_handler(tauri::generate_handler![
            review::review_save,
            review::review_list,
            review::review_remove,
            review::review_load,
            commands::review_copy,
            settings::config_load,
            settings::config_set,
            settings::config_set_many,
            settings::config_export_file,
            commands::video_source,
            graphics::graphics_info,

            ai::acceleration::ai_acceleration_info,
            commands::app_info,
            commands::report,
            commands::finish_self_test,
            commands::pause,
            commands::open_folder,
            commands::close_folder,
            commands::load_metadata,
            commands::prefetch,
            commands::warm,
            commands::warm_remaining,
            commands::set_marks,
            commands::flush_marks,
            commands::set_framing,
            commands::move_to_trash,
            commands::histogram,
            commands::folder_cover,
            commands::folders_exist,
            commands::cache_usage,
            commands::trim_cache,
            commands::set_cache_limit,
            commands::transfer_photos,
            commands::ship_conflicts,
            commands::ship,
            commands::editing_apps,
            commands::lightroom_catalogs,
            commands::open_in_editor,
            commands::paths_exist,
            commands::ingest_volumes,
            commands::ingest_browse,
            commands::ingest_scan,
            commands::ingest_sources,
            commands::ingest_details,
            commands::ingest_conflicts,
            commands::ingest_copy,
            commands::ingest_cancel,
            commands::ai_scan,
            commands::ai_look_at,
            commands::ai_compare,
            commands::ai_stop,
            commands::ai_set_yield,
            commands::ai_cache_usage,
            commands::ai_clear_cache,
            update::update_check,
            update::update_install,
            update::open_release_page,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            // Quitting is the normal way to stop for the night, and a rating
            // given a moment before must not be lost to it.
            let copying = || app.try_state::<Arc<App>>().is_some_and(|s| s.ingesting.load(std::sync::atomic::Ordering::SeqCst));
            match event {
                // Photos are coming off a card. Quitting now would leave the
                // photographer with part of a shoot and no word of which
                // part, so it is refused and they are told why.
                RunEvent::ExitRequested { api, .. } if copying() => {
                    api.prevent_exit();
                    app.emit("quit-refused", ()).ok();
                }
                RunEvent::WindowEvent { event: tauri::WindowEvent::CloseRequested { api, .. }, .. } if copying() => {
                    api.prevent_close();
                    app.emit("quit-refused", ()).ok();
                }
                RunEvent::Exit => {
                    if let Some(state) = app.try_state::<Arc<App>>() {
                        state.writer.flush();
                    }
                }
                _ => {}
            }
        });
}
