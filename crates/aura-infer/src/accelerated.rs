//! ONNX Runtime, for the models too large for the bundled interpreter. ADR-0103.
//!
//! The interpreter (ADR-0007) runs the small models AURA has always shipped. The segmentation
//! networks a Lightroom-class mask needs - a subject model at 1024 pixels, a sky model, an
//! object model - take seconds to minutes on it, so they run here instead: Microsoft's ONNX
//! Runtime, on the graphics card through DirectML where there is one, and on the processor
//! where there is not.
//!
//! Three rules:
//!
//! * **Loaded, never linked.** `onnxruntime.dll` is opened at run time from beside the
//!   application (or `AURA_ORT_DYLIB`). A machine without it, or with a damaged copy, gets
//!   `None` and every caller falls back to what it did before; nothing fails to start.
//! * **A model is verified before it is opened.** Its SHA-256 is checked against the pinned
//!   value on every load, so a truncated download or a damaged disk is a refusal with a reason,
//!   never a session that returns noise.
//! * **The card first, then the processor.** DirectML is registered first; ONNX Runtime places
//!   on the processor whatever DirectML cannot run, and the device that ran is reported.
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock, PoisonError};

use ort::ep;
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::Tensor;

/// The runtime, once loaded: the library's path, or why it could not be.
static RUNTIME: OnceLock<Result<PathBuf, String>> = OnceLock::new();

/// The directory the application's own executable is in.
fn beside_executable() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
}

/// Where the runtime library is looked for, in order.
fn candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(path) = std::env::var_os("AURA_ORT_DYLIB") {
        out.push(PathBuf::from(path));
    }
    if let Some(dir) = beside_executable() {
        out.push(dir.join("onnxruntime.dll"));
        // Test executables live one level below the build's own directory.
        if let Some(parent) = dir.parent() {
            out.push(parent.join("onnxruntime.dll"));
        }
    }
    out
}

/// Load ONNX Runtime. `Err` says why there is none; callers treat that as "not on this
/// machine" and use their fallback.
///
/// # Errors
/// The library is missing, or it is not a runtime this build can talk to.
pub fn runtime() -> Result<&'static Path, String> {
    RUNTIME
        .get_or_init(|| {
            let found = candidates()
                .into_iter()
                .find(|p| p.is_file())
                .ok_or_else(|| "onnxruntime.dll is not installed beside AURA".to_string())?;
            let builder = ort::init_from(&found).map_err(|e| format!("ONNX Runtime: {e}"))?;
            builder.with_name("aura").commit();
            Ok(found)
        })
        .as_ref()
        .map(PathBuf::as_path)
        .map_err(Clone::clone)
}

/// Where the large models live: `AURA_MODELS_DIR`, else `models` beside the application.
#[must_use]
pub fn models_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("AURA_MODELS_DIR") {
        return Some(PathBuf::from(dir));
    }
    let dir = beside_executable()?;
    [dir.join("models"), dir.parent()?.join("models")]
        .into_iter()
        .find(|d| d.is_dir())
}

/// A tensor in or out: its shape and its values, row-major.
#[derive(Debug, Clone, PartialEq)]
pub struct Array {
    pub shape: Vec<usize>,
    pub values: Vec<f32>,
}

/// One loaded model.
#[derive(Debug)]
pub struct Model {
    path: PathBuf,
    /// The session and what it was built for: `directml` or `cpu`.
    session: Mutex<(Session, &'static str)>,
    inputs: Vec<String>,
    outputs: Vec<String>,
}

/// A session for `path`, on the graphics card or the processor.
fn build(path: &Path, card: bool) -> Result<Session, String> {
    let builder = Session::builder()
        .map_err(|e| e.to_string())?
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .map_err(|e| e.to_string())?;
    let mut builder = if card {
        builder
            .with_execution_providers([ep::DirectML::default().build()])
            .map_err(|e| e.to_string())?
    } else {
        builder
    };
    builder.commit_from_file(path).map_err(|e| e.to_string())
}

/// SHA-256 of a file, as lowercase hex.
fn sha256(path: &Path) -> Result<String, String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut hasher = <sha2::Sha256 as sha2::Digest>::new();
    let mut buffer = vec![0_u8; 1 << 20];
    loop {
        let n = file
            .read(&mut buffer)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        if n == 0 {
            break;
        }
        sha2::Digest::update(&mut hasher, buffer.get(..n).unwrap_or_default());
    }
    let digest = sha2::Digest::finalize(hasher);
    let mut hex = String::with_capacity(64);
    for byte in digest {
        let _ = std::fmt::Write::write_fmt(&mut hex, format_args!("{byte:02x}"));
    }
    Ok(hex)
}

impl Model {
    /// Open `file` under [`models_dir`], after checking it is exactly the pinned model.
    ///
    /// # Errors
    /// No runtime, no such file, a different file, or a model the runtime refuses.
    pub fn open(file: &str, expected_sha256: &str) -> Result<Self, String> {
        runtime()?;
        let path = models_dir()
            .map(|d| d.join(file))
            .filter(|p| p.is_file())
            .ok_or_else(|| format!("{file} is not installed"))?;
        let found = sha256(&path)?;
        if !found.eq_ignore_ascii_case(expected_sha256) {
            return Err(format!(
                "{file} is not the pinned model (SHA-256 {found}); reinstall it"
            ));
        }
        let use_card = std::env::var_os("AURA_ORT_CPU").is_none();
        let (session, device) = match use_card.then(|| build(&path, true)) {
            Some(Ok(session)) => (session, "directml"),
            _ => (build(&path, false)?, "cpu"),
        };
        let inputs = session
            .inputs()
            .iter()
            .map(|o| o.name().to_string())
            .collect();
        let outputs = session
            .outputs()
            .iter()
            .map(|o| o.name().to_string())
            .collect();
        Ok(Self {
            path,
            session: Mutex::new((session, device)),
            inputs,
            outputs,
        })
    }

    /// Each input's declared type and shape, for a status panel and for choosing a size.
    #[must_use]
    pub fn input_types(&self) -> Vec<String> {
        let state = self.session.lock().unwrap_or_else(PoisonError::into_inner);
        state
            .0
            .inputs()
            .iter()
            .map(|o| format!("{}: {:?}", o.name(), o.dtype()))
            .collect()
    }

    /// `directml` or `cpu`: where the model runs now.
    #[must_use]
    pub fn device(&self) -> &'static str {
        self.session
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .1
    }

    /// The model's input names, in order.
    #[must_use]
    pub fn inputs(&self) -> &[String] {
        &self.inputs
    }

    /// The model's output names, in order.
    #[must_use]
    pub fn outputs(&self) -> &[String] {
        &self.outputs
    }

    /// Run with float inputs given by name; every float output comes back by name.
    ///
    /// A run that fails on the graphics card - most often because it has too little memory for
    /// a 1024-pixel network - is repeated once on the processor, which the model then keeps.
    ///
    /// # Errors
    /// A shape the model refuses, or a failure inside the runtime on the processor too.
    pub fn run(&self, inputs: Vec<(String, Array)>) -> Result<Vec<(String, Array)>, String> {
        let all: Vec<&str> = self.outputs.iter().map(String::as_str).collect();
        self.run_for(inputs, &all)
    }

    /// [`Self::run`], copying out only the `wanted` outputs: a network with deep supervision
    /// returns several full-size side outputs nobody reads.
    ///
    /// # Errors
    /// As [`Self::run`].
    pub fn run_for(
        &self,
        inputs: Vec<(String, Array)>,
        wanted: &[&str],
    ) -> Result<Vec<(String, Array)>, String> {
        let names: Vec<String> = self
            .outputs
            .iter()
            .filter(|n| wanted.contains(&n.as_str()))
            .cloned()
            .collect();
        let mut state = self.session.lock().unwrap_or_else(PoisonError::into_inner);
        match Self::run_on(&mut state.0, &names, inputs.clone()) {
            Ok(out) => Ok(out),
            Err(error) if state.1 == "directml" => {
                tracing::warn!(model = %self.path.display(), %error, "the graphics card could not run this model; using the processor");
                *state = (build(&self.path, false)?, "cpu");
                Self::run_on(&mut state.0, &names, inputs)
            }
            Err(error) => Err(error),
        }
    }

    fn run_on(
        session: &mut Session,
        names: &[String],
        inputs: Vec<(String, Array)>,
    ) -> Result<Vec<(String, Array)>, String> {
        let mut values = Vec::with_capacity(inputs.len());
        for (name, array) in inputs {
            let shape: Vec<i64> = array
                .shape
                .iter()
                .map(|d| i64::try_from(*d).unwrap_or(i64::MAX))
                .collect();
            let tensor = Tensor::from_array((shape, array.values)).map_err(|e| e.to_string())?;
            values.push((
                std::borrow::Cow::from(name),
                ort::session::SessionInputValue::from(tensor),
            ));
        }
        let outputs = session.run(values).map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for name in names {
            let Some(value) = outputs.get(name) else {
                continue;
            };
            let Ok((shape, values)) = value.try_extract_tensor::<f32>() else {
                continue;
            };
            out.push((
                name.clone(),
                Array {
                    shape: shape
                        .iter()
                        .map(|d| usize::try_from(*d).unwrap_or(0))
                        .collect(),
                    values: values.to_vec(),
                },
            ));
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_model_is_a_reason_not_a_failure_to_start() {
        let found = Model::open("no-such-model.onnx", "00");
        assert!(found.is_err());
    }
}
