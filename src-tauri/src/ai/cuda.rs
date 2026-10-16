//! One bounded GPU worker owns all CUDA model sessions. CPU decoding stays parallel.
use std::{collections::HashMap, path::PathBuf};
use std::sync::{mpsc::{self, SyncSender}, OnceLock};
use std::panic::{catch_unwind, AssertUnwindSafe};
use ort::session::Session;
use super::{acceleration, onnx::{self, OwnedInput}};

type Output = Vec<(Vec<usize>, Vec<f32>)>;
pub(super) struct Reply {
    pub inputs: Vec<OwnedInput>,
    pub result: Result<Output, String>,
}
enum ModelSource { Embedded(&'static [u8]), File(PathBuf) }
impl ModelSource {
    fn key(&self) -> String {
        match self { Self::Embedded(bytes) => format!("embedded:{:x}",bytes.as_ptr() as usize), Self::File(path) => format!("file:{}",path.display()) }
    }
}
struct Job {
    model: ModelSource,
    inputs: Vec<OwnedInput>,
    outputs: Vec<String>,
    done: SyncSender<Reply>,
}
static WORKER: OnceLock<Result<SyncSender<Job>, String>> = OnceLock::new();

fn start() -> Result<SyncSender<Job>, String> {
    // At most one queued job; existing analysis callers apply backpressure.
    let (send, receive) = mpsc::sync_channel::<Job>(1);
    std::thread::Builder::new().name("opencull-cuda".into()).spawn(move || {
        let mut sessions = HashMap::<String, Session>::new();
        for job in receive {
            let result = if acceleration::enabled() {
                catch_unwind(AssertUnwindSafe(|| {
                    let key = job.model.key();
                    if let std::collections::hash_map::Entry::Vacant(entry) = sessions.entry(key.clone()) {
                        let builder = onnx::builder()?
                            .with_execution_providers([opencull_cuda::provider()]).map_err(|e| e.to_string())?;
                        // MobileCLIP has shape/control nodes that ORT deliberately
                        // places on CPU. Keep its compute graph on CUDA while
                        // allowing those tiny operations; bundled face models
                        // retain their stricter all-CUDA requirement.
                        let mut builder = match &job.model {
                            ModelSource::Embedded(_) => builder.with_config_entry("session.disable_cpu_ep_fallback", "1").map_err(|e| e.to_string())?,
                            ModelSource::File(_) => builder,
                        };
                        entry.insert(match &job.model {
                            ModelSource::Embedded(bytes) => builder.commit_from_memory(bytes),
                            ModelSource::File(path) => builder.commit_from_file(path),
                        }.map_err(|e| e.to_string())?);
                        eprintln!("[ai] CUDA model ready: {} ({} shared sessions)", match &job.model { ModelSource::Embedded(_) => "face analysis", ModelSource::File(_) => "MobileCLIP-S0" }, sessions.len());
                    }
                    onnx::evaluate(sessions.get_mut(&key).expect("model opened"), &job.inputs, &job.outputs,
                        |out| out.iter().map(|(shape, data)| (shape.clone(), data.to_vec())).collect())
                })).unwrap_or_else(|_| Err("CUDA worker panicked".into()))
            } else { Err("CUDA disabled after an earlier failure".into()) };
            if let Err(reason) = &result {
                sessions.clear();
                acceleration::fallback(reason.clone());
            }
            let _ = job.done.send(Reply { inputs: job.inputs, result });
        }
    }).map_err(|e| e.to_string())?;
    Ok(send)
}

pub(super) fn run(bytes: &'static [u8], inputs: Vec<OwnedInput>, outputs: Vec<String>) -> Result<Reply, String> {
    submit(ModelSource::Embedded(bytes),inputs,outputs)
}
/// Downloaded models use the same bounded worker; their weights are not copied
/// into every request or leaked to obtain a static lifetime.
pub(super) fn run_file(path: PathBuf, inputs: Vec<OwnedInput>, outputs: Vec<String>) -> Result<Reply,String> {
    submit(ModelSource::File(path),inputs,outputs)
}
fn submit(model: ModelSource, inputs: Vec<OwnedInput>, outputs: Vec<String>) -> Result<Reply,String> {
    let worker = match WORKER.get_or_init(start) {
        Ok(worker) => worker,
        Err(error) => { acceleration::fallback(error.clone()); return Ok(Reply { inputs, result: Err(error.clone()) }); }
    };
    let (done, receive) = mpsc::sync_channel(1);
    if let Err(error) = worker.send(Job { model, inputs, outputs, done }) {
        let reason = "CUDA worker stopped".to_string();
        acceleration::fallback(reason.clone());
        return Ok(Reply { inputs: error.0.inputs, result: Err(reason) });
    }
    receive.recv().map_err(|_| "CUDA worker stopped before returning its input".into())
}
