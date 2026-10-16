//! Automatic inference selection. Rendering is configured independently.
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
};

static FAILED: AtomicBool = AtomicBool::new(false);
static FAILURE: Mutex<Option<String>> = Mutex::new(None);

fn requested_on(
    platform: &str,
    arch: &str,
    compiled: bool,
    requested: Option<&str>,
    detected: bool,
) -> bool {
    platform == "linux"
        && arch == "x86_64"
        && compiled
        && match requested {
            Some("cuda") => true,
            None | Some("auto") => detected,
            _ => false,
        }
}

fn detected() -> bool {
    #[cfg(all(feature = "cuda", target_os = "linux", target_arch = "x86_64"))]
    {
        static DETECTED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *DETECTED.get_or_init(|| match opencull_cuda::detect() {
            Ok(()) => {
                eprintln!("[ai] Automatic CUDA: compatible NVIDIA device and runtime detected");
                true
            }
            Err(reason) => {
                eprintln!("[ai] Automatic CPU: {reason}");
                false
            }
        })
    }
    #[cfg(not(all(feature = "cuda", target_os = "linux", target_arch = "x86_64")))]
    {
        false
    }
}
pub fn requested() -> bool {
    let request = std::env::var("OPENCULL_AI").ok();
    let automatic = request.as_deref().is_none_or(|value| value == "auto");
    requested_on(
        std::env::consts::OS,
        std::env::consts::ARCH,
        cfg!(all(
            feature = "cuda",
            target_os = "linux",
            target_arch = "x86_64"
        )),
        request.as_deref(),
        automatic && detected(),
    )
}
pub fn enabled() -> bool {
    requested() && !FAILED.load(Ordering::Acquire)
}

pub fn fallback(reason: String) {
    if !FAILED.swap(true, Ordering::AcqRel) {
        eprintln!("[ai] CUDA unavailable; continuing on CPU: {reason}");
        *FAILURE.lock().unwrap_or_else(|e| e.into_inner()) = Some(reason);
    }
}
pub fn failure() -> Option<String> {
    FAILURE.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Diagnose an actual worker error; device-busy errors do not prove low VRAM.
pub fn failure_kind(reason: &str) -> &'static str {
    let text = reason.to_ascii_lowercase();
    if [
        "out of memory",
        "failed to allocate memory",
        "cudaerrormemoryallocation",
        "cudnn_status_alloc_failed",
    ]
    .iter()
    .any(|s| text.contains(s))
    {
        "memory"
    } else if text.contains("busy or unavailable") || text.contains("cudasetdevice") {
        "unavailable"
    } else {
        "runtime"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn memory_failures_are_distinguished_from_busy_devices_and_other_errors() {
        for error in [
            "CUDA out of memory",
            "Failed to allocate memory for requested buffer",
            "cudaErrorMemoryAllocation",
            "CUDNN_STATUS_ALLOC_FAILED",
        ] {
            assert_eq!(failure_kind(error), "memory");
        }
        assert_eq!(
            failure_kind("CUDA failure 46: CUDA-capable devices are busy or unavailable"),
            "unavailable"
        );
        assert_eq!(
            failure_kind("CUDA provider library could not be loaded"),
            "runtime"
        );
    }
    #[test]
    fn cuda_auto_detection_respects_supported_target_build_and_overrides() {
        if !cfg!(all(
            feature = "cuda",
            target_os = "linux",
            target_arch = "x86_64"
        )) {
            assert!(ai_acceleration_info().is_none());
        }
        assert!(requested_on("linux", "x86_64", true, Some("cuda"), false));
        assert!(requested_on("linux", "x86_64", true, None, true));
        assert!(requested_on("linux", "x86_64", true, Some("auto"), true));
        assert!(!requested_on("linux", "x86_64", true, Some("cpu"), true));
        assert!(!requested_on("linux", "x86_64", true, Some("auto"), false));
        for (os, arch, compiled, request) in [
            ("macos", "aarch64", true, Some("cuda")),
            ("macos", "x86_64", true, Some("cuda")),
            ("windows", "x86_64", true, Some("cuda")),
            ("linux", "aarch64", true, Some("cuda")),
            ("linux", "x86_64", false, Some("cuda")),
            ("linux", "x86_64", true, None),
            ("linux", "x86_64", true, Some("cpu")),
        ] {
            assert!(!requested_on(os, arch, compiled, request, false));
        }
    }
}

/// The UI receives no CUDA option on macOS, Windows, ARM, or CPU-only builds.
#[tauri::command]
pub fn ai_acceleration_info() -> Option<serde_json::Value> {
    if !cfg!(all(
        feature = "cuda",
        target_os = "linux",
        target_arch = "x86_64"
    )) {
        return None;
    }
    let reason = failure();
    Some(
        serde_json::json!({ "requested": requested(), "enabled": enabled(), "failureKind": reason.as_deref().map(failure_kind), "failure": reason }),
    )
}
