//! One bounded GPU worker owns CUDA model sessions. CPU decoding stays parallel.
use super::{
    acceleration,
    file_backend::{self, Input, Reply},
    onnx,
};
use ort::session::Session;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{
    mpsc::{self, SyncSender},
    OnceLock,
};
use std::{collections::HashMap, path::PathBuf};

enum ModelSource {
    Embedded(&'static [u8]),
    File(PathBuf),
}
impl ModelSource {
    fn key(&self) -> String {
        match self {
            Self::Embedded(bytes) => format!("embedded:{:x}", bytes.as_ptr() as usize),
            Self::File(path) => format!("file:{}", path.display()),
        }
    }
}
struct Job {
    model: ModelSource,
    inputs: Vec<Input>,
    outputs: Vec<String>,
    done: SyncSender<Reply>,
}
static WORKER: OnceLock<Result<SyncSender<Job>, String>> = OnceLock::new();

fn start() -> Result<SyncSender<Job>, String> {
    // Bound queued work so GPU memory use does not grow with parallel callers.
    let (send, receive) = mpsc::sync_channel::<Job>(1);
    std::thread::Builder::new()
        .name("opencull-cuda".into())
        .spawn(move || {
            let mut sessions = HashMap::<String, Session>::new();
            for job in receive {
                let result = if acceleration::enabled() {
                    catch_unwind(AssertUnwindSafe(|| {
                        let key = job.model.key();
                        if let std::collections::hash_map::Entry::Vacant(entry) =
                            sessions.entry(key.clone())
                        {
                            let builder = onnx::builder()?
                                .with_execution_providers([opencull_cuda::provider()])
                                .map_err(|e| e.to_string())?;
                            // Bundled face models must run wholly on CUDA. File-backed
                            // models may contain small ops unsupported by the CUDA EP.
                            let mut builder = match &job.model {
                                ModelSource::Embedded(_) => builder
                                    .with_config_entry("session.disable_cpu_ep_fallback", "1")
                                    .map_err(|e| e.to_string())?,
                                ModelSource::File(_) => builder,
                            };
                            entry.insert(
                                match &job.model {
                                    ModelSource::Embedded(bytes) => {
                                        builder.commit_from_memory(bytes)
                                    }
                                    ModelSource::File(path) => builder.commit_from_file(path),
                                }
                                .map_err(|e| e.to_string())?,
                            );
                            let kind = match &job.model {
                                ModelSource::Embedded(_) => "bundled",
                                ModelSource::File(_) => "file",
                            };
                            eprintln!(
                                "[ai] CUDA model ready: {kind} model ({} shared sessions)",
                                sessions.len()
                            );
                        }
                        onnx::evaluate(
                            sessions.get_mut(&key).expect("model opened"),
                            &job.inputs,
                            &job.outputs,
                            |out| {
                                out.iter()
                                    .map(|(shape, data)| (shape.clone(), data.to_vec()))
                                    .collect()
                            },
                        )
                    }))
                    .unwrap_or_else(|_| Err("CUDA worker panicked".into()))
                } else {
                    Err("CUDA disabled after an earlier failure".into())
                };
                if let Err(reason) = &result {
                    sessions.clear();
                    acceleration::fallback(reason.clone());
                }
                let _ = job.done.send(Reply {
                    inputs: job.inputs,
                    result,
                });
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(send)
}

pub(super) fn run(
    bytes: &'static [u8],
    inputs: Vec<Input>,
    outputs: Vec<String>,
) -> Result<Reply, String> {
    submit(ModelSource::Embedded(bytes), inputs, outputs)
}

/// Registered as the optional file-model backend for separately installed models.
pub(super) fn run_file(
    path: PathBuf,
    inputs: Vec<Input>,
    outputs: Vec<String>,
) -> Result<Reply, String> {
    submit(ModelSource::File(path), inputs, outputs)
}

fn submit(model: ModelSource, inputs: Vec<Input>, outputs: Vec<String>) -> Result<Reply, String> {
    let worker = match WORKER.get_or_init(start) {
        Ok(worker) => worker,
        Err(error) => {
            acceleration::fallback(error.clone());
            return Ok(Reply {
                inputs,
                result: Err(error.clone()),
            });
        }
    };
    let (done, receive) = mpsc::sync_channel(1);
    if let Err(error) = worker.send(Job {
        model,
        inputs,
        outputs,
        done,
    }) {
        let reason = "CUDA worker stopped".to_string();
        acceleration::fallback(reason.clone());
        return Ok(Reply {
            inputs: error.0.inputs,
            result: Err(reason),
        });
    }
    receive
        .recv()
        .map_err(|_| "CUDA worker stopped before returning its input".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_cache_keys_separate_embedded_and_file_models() {
        let embedded = ModelSource::Embedded(b"model");
        let file = ModelSource::File(PathBuf::from("model"));
        assert_ne!(embedded.key(), file.key());
        assert_eq!(file.key(), ModelSource::File(PathBuf::from("model")).key());
    }

    #[test]
    #[ignore = "requires OPENCULL_AI=cuda, NVIDIA GPU, CUDA 13 and cuDNN 9"]
    fn registered_file_model_runs_on_cuda_and_matches_cpu() {
        assert!(acceleration::enabled(), "set OPENCULL_AI=cuda");
        let model = include_bytes!("../../models/finder.onnx");
        let path = std::env::temp_dir().join(format!(
            "opencull-cuda-file-model-{}.onnx",
            std::process::id()
        ));
        std::fs::write(&path, model).unwrap();
        let shape = vec![1, 3, 64, 96];
        let data = (0..shape.iter().product::<usize>())
            .map(|i| ((i * 19 % 251) as f32) / 250.0)
            .collect::<Vec<_>>();
        let inputs = vec![("input".to_owned(), shape, data)];
        let outputs = [
            "cls_8", "cls_16", "cls_32", "obj_8", "obj_16", "obj_32", "bbox_8", "bbox_16",
            "bbox_32", "kps_8", "kps_16", "kps_32",
        ];
        let output_names = outputs
            .iter()
            .map(|name| (*name).to_owned())
            .collect::<Vec<_>>();
        let mut cpu_session = onnx::builder().unwrap().commit_from_memory(model).unwrap();
        let cpu = onnx::evaluate(&mut cpu_session, &inputs, &output_names, |values| {
            values
                .iter()
                .map(|(shape, data)| (shape.clone(), data.to_vec()))
                .collect::<Vec<_>>()
        })
        .unwrap();

        crate::ai::register_cuda_file_backend().unwrap();
        assert!(file_backend::accelerated());
        let reply = file_backend::run(path.clone(), inputs.clone(), output_names).unwrap();
        let gpu = reply.result.unwrap();
        assert_eq!(cpu.len(), gpu.len());
        for ((cpu_shape, cpu_values), (gpu_shape, gpu_values)) in cpu.iter().zip(&gpu) {
            assert_eq!(cpu_shape, gpu_shape);
            assert_eq!(cpu_values.len(), gpu_values.len());
            for (cpu_value, gpu_value) in cpu_values.iter().zip(gpu_values) {
                assert!(
                    (cpu_value - gpu_value).abs() <= 0.001 + 0.001 * cpu_value.abs(),
                    "CPU {cpu_value}, CUDA {gpu_value}"
                );
            }
        }
        assert!(acceleration::failure().is_none(), "CUDA silently fell back");
        std::fs::remove_file(path).unwrap();
    }
}
