//! The CUDA provider, isolated from macOS and ordinary CPU builds.
pub fn provider() -> ort::ep::ExecutionProviderDispatch {
    ort::ep::CUDA::default()
        .with_arena_extend_strategy(ort::ep::ArenaExtendStrategy::SameAsRequested)
        .with_tf32(false)
        .build()
        .error_on_failure()
}

/// Probe the installed driver and runtime without allocating model sessions.
pub fn detect() -> Result<(), String> {
    // SAFETY: only the platform CUDA libraries are loaded. The two symbols use
    // the documented CUDA Driver API C ABI; handles live through every call.
    unsafe {
        let names = ["libcuda.so.1", "libcudart.so.13", "libcublas.so.13", "libcublasLt.so.13", "libcurand.so.10", "libcudnn.so.9"];
        let libraries = names.iter().map(|name| libloading::Library::new(name).map_err(|e| format!("{name}: {e}"))).collect::<Result<Vec<_>,_>>()?;
        let initialize: libloading::Symbol<unsafe extern "C" fn(u32) -> i32> = libraries[0].get(b"cuInit\0").map_err(|e| e.to_string())?;
        let count_devices: libloading::Symbol<unsafe extern "C" fn(*mut i32) -> i32> = libraries[0].get(b"cuDeviceGetCount\0").map_err(|e| e.to_string())?;
        let status = initialize(0);
        if status != 0 { return Err(format!("CUDA driver initialization failed ({status})")); }
        let mut count = 0;
        let status = count_devices(&mut count);
        if status != 0 || count == 0 { return Err(format!("No usable CUDA device ({status})")); }
        Ok(())
    }
}
