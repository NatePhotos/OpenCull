//! Running a model file. CPU sessions stay thread-local; CUDA uses one worker.
use ort::session::{
    builder::{GraphOptimizationLevel, SessionBuilder},
    Session,
};
use ort::value::TensorRef;
use std::cell::RefCell;

/// One input to a model: its name, its shape, and its numbers.
pub type Input<'a> = (&'a str, Vec<usize>, Vec<f32>);
pub(super) type OwnedInput = super::file_backend::Input;

pub(super) fn builder() -> Result<SessionBuilder, String> {
    let builder = Session::builder().map_err(|e| e.to_string())?;
    let builder = builder
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .map_err(|e| e.to_string())?;
    builder.with_intra_threads(1).map_err(|e| e.to_string())
}
fn open(bytes: &[u8]) -> Result<Session, String> {
    builder()?
        .commit_from_memory(bytes)
        .map_err(|e| format!("could not load a model: {e}"))
}

/// Borrows caller-owned tensors while running, avoiding an extra image copy.
pub(super) fn evaluate<T>(
    session: &mut Session,
    inputs: &[OwnedInput],
    outputs: &[String],
    read: impl FnOnce(&[(Vec<usize>, &[f32])]) -> T,
) -> Result<T, String> {
    let mut given = Vec::with_capacity(inputs.len());
    for (name, shape, data) in inputs {
        let tensor = TensorRef::from_array_view((shape.clone(), data.as_slice()))
            .map_err(|e| e.to_string())?;
        given.push((name.as_str(), tensor));
    }
    let results = session
        .run(given)
        .map_err(|e| format!("a model failed to run: {e}"))?;
    let mut taken = Vec::with_capacity(outputs.len());
    for name in outputs {
        let (shape, data) = results[name.as_str()]
            .try_extract_tensor::<f32>()
            .map_err(|e| e.to_string())?;
        taken.push((
            shape.iter().map(|d| *d as usize).collect::<Vec<usize>>(),
            data,
        ));
    }
    Ok(read(&taken))
}

/// Runs `bytes` on this thread, or on the shared CUDA worker when enabled.
pub fn run<T>(
    slot: &'static std::thread::LocalKey<RefCell<Option<Session>>>,
    bytes: &'static [u8],
    inputs: Vec<Input>,
    outputs: &[&str],
    read: impl FnOnce(&[(Vec<usize>, &[f32])]) -> T,
) -> Result<T, String> {
    let inputs = inputs
        .into_iter()
        .map(|(name, shape, data)| (name.to_owned(), shape, data))
        .collect::<Vec<_>>();
    let outputs = outputs.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    #[cfg(all(feature = "cuda", target_os = "linux", target_arch = "x86_64"))]
    let inputs = if super::acceleration::enabled() {
        let reply = super::cuda::run(bytes, inputs, outputs.clone())?;
        match reply.result {
            Ok(result) => {
                return Ok(read(
                    &result
                        .iter()
                        .map(|(shape, data)| (shape.clone(), data.as_slice()))
                        .collect::<Vec<_>>(),
                ))
            }
            Err(_) => reply.inputs,
        }
    } else {
        inputs
    };
    slot.with(|cell| {
        let mut cell = cell.borrow_mut();
        if cell.is_none() {
            *cell = Some(open(bytes)?);
        }
        evaluate(
            cell.as_mut().expect("model opened"),
            &inputs,
            &outputs,
            read,
        )
    })
}

#[cfg(all(test, feature = "cuda", target_os = "linux", target_arch = "x86_64"))]
mod cuda_tests {
    use super::*;
    #[test]
    #[ignore = "requires OPENCULL_AI=cuda, NVIDIA GPU, CUDA 13 and cuDNN 9"]
    fn concurrent_callers_share_cuda_models_and_match_cpu() {
        assert!(
            crate::ai::acceleration::enabled(),
            "run with OPENCULL_AI=cuda"
        );
        let jobs = (0..4)
            .map(|_| {
                std::thread::spawn(|| {
                    for n in [1, 8] {
                        let cases: Vec<(&'static [u8], Vec<(&str, Vec<usize>)>, Vec<&str>)> = vec![
                            (
                                include_bytes!("../../models/finder.onnx"),
                                vec![("input", vec![1, 3, 64, 96])],
                                vec!["cls_8", "bbox_8", "kps_8"],
                            ),
                            (
                                include_bytes!("../../models/judge.onnx"),
                                vec![
                                    ("face", vec![n, 3, 224, 224]),
                                    ("eyes", vec![n, 3, 64, 224]),
                                ],
                                vec!["validity", "eyes_state", "quality"],
                            ),
                            (
                                include_bytes!("../../models/eyes.onnx"),
                                vec![("pair", vec![n, 3, 64, 192]), ("turn", vec![n, 1])],
                                vec!["away", "open", "down"],
                            ),
                            (
                                include_bytes!("../../models/mouth.onnx"),
                                vec![("band", vec![n, 3, 64, 128])],
                                vec!["expression"],
                            ),
                        ];
                        for (bytes, shapes, outputs) in cases {
                            let inputs = shapes
                                .into_iter()
                                .map(|(name, shape)| {
                                    let data = (0..shape.iter().product())
                                        .map(|i: usize| {
                                            if name == "turn" {
                                                0.0
                                            } else {
                                                (i % 251) as f32
                                            }
                                        })
                                        .collect();
                                    (name.to_owned(), shape, data)
                                })
                                .collect::<Vec<_>>();
                            let outputs =
                                outputs.into_iter().map(str::to_owned).collect::<Vec<_>>();
                            let cpu =
                                evaluate(&mut open(bytes).unwrap(), &inputs, &outputs, |out| {
                                    out.iter()
                                        .map(|(s, d)| (s.clone(), d.to_vec()))
                                        .collect::<Vec<_>>()
                                })
                                .unwrap();
                            let gpu = crate::ai::cuda::run(bytes, inputs, outputs)
                                .unwrap()
                                .result
                                .unwrap();
                            assert_eq!(cpu.len(), gpu.len());
                            for ((cs, cd), (gs, gd)) in cpu.iter().zip(gpu.iter()) {
                                assert_eq!(cs, gs);
                                for (a, b) in cd.iter().zip(gd.iter()) {
                                    assert!(
                                        (a - b).abs() <= 0.001 + 0.001 * a.abs(),
                                        "CPU {a}, CUDA {b}"
                                    );
                                }
                            }
                        }
                    }
                })
            })
            .collect::<Vec<_>>();
        for job in jobs {
            job.join().unwrap();
        }
        assert!(
            crate::ai::acceleration::failure().is_none(),
            "CUDA silently fell back"
        );
    }
}
