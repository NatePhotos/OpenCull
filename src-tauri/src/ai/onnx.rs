//! Running a model file.

use std::cell::RefCell;

use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::Tensor;

/// One input to a model: its name, its shape, and its numbers.
pub type Input<'a> = (&'a str, Vec<usize>, Vec<f32>);

/// Opens a model for the thread that asks.
///
/// Each thread keeps its own, because running a model needs it to itself and
/// photos are analysed several at a time. A model is told to use one thread:
/// the several-at-a-time already fills the processor, and models competing
/// with each other for it is slower than each keeping to its own.
fn open(bytes: &[u8]) -> Result<Session, String> {
    let fail = |e: String| format!("could not load a model: {e}");
    let builder = Session::builder().map_err(|e| fail(e.to_string()))?;
    let builder = builder.with_optimization_level(GraphOptimizationLevel::Level3).map_err(|e| fail(e.to_string()))?;
    let mut builder = builder.with_intra_threads(1).map_err(|e| fail(e.to_string()))?;
    builder.commit_from_memory(bytes).map_err(|e| fail(e.to_string()))
}

/// Runs `bytes` as a model on this thread and hands each named output, with
/// its shape, to `read`.
pub fn run<T>(
    slot: &'static std::thread::LocalKey<RefCell<Option<Session>>>,
    bytes: &'static [u8],
    inputs: Vec<Input>,
    outputs: &[&str],
    read: impl FnOnce(&[(Vec<usize>, &[f32])]) -> T,
) -> Result<T, String> {
    slot.with(|cell| {
        let mut cell = cell.borrow_mut();
        if cell.is_none() {
            *cell = Some(open(bytes)?);
        }
        let session = cell.as_mut().expect("just opened");

        let mut given = Vec::with_capacity(inputs.len());
        for (name, shape, data) in inputs {
            let tensor = Tensor::from_array((shape, data)).map_err(|e| e.to_string())?;
            given.push((name.to_string(), tensor));
        }
        let results = session.run(given).map_err(|e| format!("a model failed to run: {e}"))?;
        let mut taken = Vec::with_capacity(outputs.len());
        for name in outputs {
            let output = results.get(*name).ok_or_else(|| format!("the model gave no output called {name}"))?;
            let (shape, data) = output.try_extract_tensor::<f32>().map_err(|e| e.to_string())?;
            taken.push((shape.iter().map(|d| *d as usize).collect::<Vec<usize>>(), data));
        }
        Ok(read(&taken))
    })
}
