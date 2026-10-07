use std::fmt::Write;
use std::path::Path;
use std::sync::{Arc, Mutex};

use aws_lc_rs::digest::{SHA256, digest};
use ort::session::Session;
use ort::value::TensorRef;

use crate::core::error::{AppError, AppFail, AppResult};
use crate::core::parse::Json;
use super::arch::{INPUT_SCHEMA, Input, Meta, Model, Schema, Scores};

const MODEL_BYTES_MAX: u64 = 64 * 1024 * 1024;
const META_BYTES_MAX: u64 = 65_536;

impl Model {

    pub fn load ( name: &str, dir: &Path, features: Option<&Path>, threads: usize ) -> AppResult<Self> {

        let bytes = Self::read(dir, "model.onnx", MODEL_BYTES_MAX)?;
        let meta: Meta = Json::parse(&Self::read(dir, "metadata.json", META_BYTES_MAX)?).map_err(|error| AppError::config("add_model", format!("{name}: invalid metadata.json: {error}")))?;
        let sha256 = Self::hex(digest(&SHA256, &bytes).as_ref());

        Self::check(name, &meta, &sha256)?;

        let features = features.map_or_else(|| dir.join("features.json"), Path::to_path_buf);
        let schema = Schema::load(&features, meta.feature_version, meta.architecture.feature_count)?;

        let session = Session::builder().or_fail("cannot initialize inference")?
            .with_intra_threads(threads.max(1)).map_err(|error| AppError::invalid("inference", format!("cannot set intra threads: {error}")))?
            .with_inter_threads(1).map_err(|error| AppError::invalid("inference", format!("cannot set inter threads: {error}")))?
            .commit_from_memory(&bytes).or_fail_with(|| format!("{name}: cannot load model.onnx"))?;

        let model = Self { name: Arc::from(name), meta, schema, session: Mutex::new(session), sha256 };

        model.predict(&model.empty())?;

        Ok(model)

    }

    pub fn name ( &self ) -> &str {

        &self.name

    }

    pub fn meta ( &self ) -> &Meta {

        &self.meta

    }

    pub fn sha256 ( &self ) -> &str {

        &self.sha256

    }

    pub fn schema ( &self ) -> &Schema {

        &self.schema

    }

    pub fn empty ( &self ) -> Input {

        Input::empty(&self.meta.architecture)

    }

    pub fn predict ( &self, input: &Input ) -> AppResult<Scores> {

        let shape = &self.meta.architecture;

        if !input.fits(shape) { return Err(AppError::invalid("model input", "tensors are out of bounds or mis-sized")); }

        let features = TensorRef::from_array_view(([1, shape.feature_count], input.features.as_slice())).or_fail("cannot build features tensor")?;
        let text = TensorRef::from_array_view(([1, shape.text_streams, shape.text_bytes], input.text.as_slice())).or_fail("cannot build text tensor")?;
        let names = TensorRef::from_array_view(([1, shape.event_count, shape.event_bytes], input.event_text.as_slice())).or_fail("cannot build event text tensor")?;
        let values = TensorRef::from_array_view(([1, shape.event_count, shape.event_values], input.event_values.as_slice())).or_fail("cannot build event values tensor")?;
        let coverage = TensorRef::from_array_view(([1, shape.coverage], input.coverage.as_slice())).or_fail("cannot build coverage tensor")?;

        let mut session = self.session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let outputs = session.run(ort::inputs!["features" => features, "text" => text, "event_text" => names, "event_values" => values, "coverage" => coverage]).or_fail("inference failed")?;

        let read = |name: &str| -> AppResult<f32> {

            let output = outputs.get(name).ok_or_else(|| AppError::invalid("model output", format!("missing `{name}` tensor")))?;
            let ( shape, values ) = output.try_extract_tensor::<f32>().or_fail_with(|| format!("invalid `{name}` tensor"))?;

            match values.first() {
                Some(value) if shape.as_ref() == [1, 1] && values.len() == 1 && value.is_finite() && (0.0..=1.0).contains(value) => Ok(*value),
                _ => Err(AppError::invalid("model output", format!("`{name}` is not a finite [1,1] probability"))),
            }

        };

        let scores = Scores { risk: read("risk")?, content: read("content_risk")?, journey: read("journey_risk")? };

        if (scores.risk - scores.content.max(scores.journey)).abs() > 1e-6 { return Err(AppError::invalid("model output", "combined risk disagrees with its components")); }

        Ok(scores)

    }

    fn check ( name: &str, meta: &Meta, sha256: &str ) -> AppResult<()> {

        let arch = &meta.architecture;

        if meta.input_schema.as_deref() != Some(INPUT_SCHEMA) || arch.name != INPUT_SCHEMA {

            return Err(AppError::config("add_model", format!("{name}: unsupported input schema {:?}", meta.input_schema)));

        }

        let expected = arch.compatible_architectures.iter().find(|entry| entry.model_version == meta.model_version).map_or(arch.parameter_count, |entry| entry.parameter_count);

        if meta.parameter_count != expected { return Err(AppError::config("add_model", format!("{name}: parameter count {} does not match architecture {expected}", meta.parameter_count))); }

        if meta.artifact_sha256 != sha256 { return Err(AppError::config("add_model", format!("{name}: model.onnx sha256 {sha256} does not match metadata"))); }

        if arch.feature_count == 0 || arch.text_bytes == 0 || arch.text_streams == 0 || arch.event_count == 0 || arch.event_bytes == 0 || arch.event_values == 0 || arch.coverage == 0 {

            return Err(AppError::config("add_model", format!("{name}: architecture declares an empty tensor")));

        }

        Ok(())

    }

    fn read ( dir: &Path, file: &str, limit: u64 ) -> AppResult<Vec<u8>> {

        use std::io::Read;

        let path = dir.join(file);
        let mut bytes = Vec::new();

        std::fs::File::open(&path).or_fail_with(|| format!("cannot open {}", path.display()))?.take(limit + 1).read_to_end(&mut bytes).or_fail_with(|| format!("cannot read {}", path.display()))?;

        if bytes.len() as u64 > limit { return Err(AppError::config("add_model", format!("{} exceeds {limit} bytes", path.display()))); }

        Ok(bytes)

    }

    fn hex ( bytes: &[u8] ) -> String {

        bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut text, byte| { let _ = write!(text, "{byte:02x}"); text })

    }

}

impl std::fmt::Debug for Model {

    fn fmt ( &self, formatter: &mut std::fmt::Formatter<'_> ) -> std::fmt::Result {

        formatter.debug_struct("Model").field("name", &self.name).field("version", &self.meta.model_version).field("sha256", &self.sha256).finish()

    }

}
