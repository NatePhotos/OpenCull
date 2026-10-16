//! Optional acceleration for models installed separately from the application.
//!
//! The CPU feature works without a registered provider. Acceleration adapters
//! register at application startup; callers retain their own CPU fallback.
use std::{path::PathBuf, sync::OnceLock};

pub type Input = (String, Vec<usize>, Vec<f32>);
pub type Output = (Vec<usize>, Vec<f32>);
pub struct Reply {
    pub result: Result<Vec<Output>, String>,
    /// Original tensors returned after failed inference for a CPU retry.
    pub inputs: Vec<Input>,
}
pub type Run = fn(PathBuf, Vec<Input>, Vec<String>) -> Result<Reply, String>;

#[derive(Clone, Copy)]
struct Backend {
    active: fn() -> bool,
    run: Run,
    failure: fn() -> Option<String>,
}
static BACKEND: OnceLock<Backend> = OnceLock::new();

fn register_into(storage: &OnceLock<Backend>, backend: Backend) -> Result<(), &'static str> {
    storage
        .set(backend)
        .map_err(|_| "file model backend already registered")
}

impl Backend {
    fn accelerated(&self) -> bool {
        (self.active)()
    }
    fn failure(&self) -> Option<String> {
        (self.failure)()
    }
    fn run(
        &self,
        path: PathBuf,
        inputs: Vec<Input>,
        outputs: Vec<String>,
    ) -> Result<Reply, String> {
        (self.run)(path, inputs, outputs)
    }
}

/// Registration is process-wide, occurs before analysis starts, and cannot
/// replace an existing provider.
pub fn register(
    active: fn() -> bool,
    run: Run,
    failure: fn() -> Option<String>,
) -> Result<(), &'static str> {
    register_into(
        &BACKEND,
        Backend {
            active,
            run,
            failure,
        },
    )
}
pub fn accelerated() -> bool {
    BACKEND.get().is_some_and(Backend::accelerated)
}
pub fn failure() -> Option<String> {
    BACKEND.get().and_then(Backend::failure)
}
pub fn run(path: PathBuf, inputs: Vec<Input>, outputs: Vec<String>) -> Result<Reply, String> {
    let backend = BACKEND
        .get()
        .ok_or("no accelerated file model backend is registered")?;
    backend.run(path, inputs, outputs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn active() -> bool {
        true
    }
    fn last_failure() -> Option<String> {
        Some("test failure".into())
    }
    fn failed_model(
        path: PathBuf,
        inputs: Vec<Input>,
        outputs: Vec<String>,
    ) -> Result<Reply, String> {
        assert_eq!(path, PathBuf::from("model.onnx"));
        assert_eq!(outputs, ["score"]);
        Ok(Reply {
            result: Err("CUDA allocation failed".into()),
            inputs,
        })
    }

    #[test]
    fn local_backend_preserves_inputs_after_failure_and_rejects_replacement() {
        let local = OnceLock::new();
        let backend = Backend {
            active,
            run: failed_model,
            failure: last_failure,
        };
        register_into(&local, backend).expect("register a local test provider");
        assert!(local.get().is_some_and(Backend::accelerated));
        assert_eq!(
            local.get().and_then(Backend::failure).as_deref(),
            Some("test failure")
        );
        let inputs = vec![("image".to_owned(), vec![1, 3], vec![0.0, 1.0, 2.0])];
        let reply = local
            .get()
            .unwrap()
            .run(
                PathBuf::from("model.onnx"),
                inputs.clone(),
                vec!["score".to_owned()],
            )
            .unwrap();
        assert_eq!(reply.result.unwrap_err(), "CUDA allocation failed");
        assert_eq!(reply.inputs, inputs);
        assert_eq!(
            register_into(&local, backend).unwrap_err(),
            "file model backend already registered"
        );
    }
}
