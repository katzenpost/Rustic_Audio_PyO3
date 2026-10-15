use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use std::sync::Mutex;

use crate::voice_note::{VoiceNoteClip, VoiceNoteEngine};

#[pyclass(module = "rustic_audio_tool", name = "VoiceNoteClip")]
struct PyVoiceNoteClip {
    inner: VoiceNoteClip,
}

#[pymethods]
impl PyVoiceNoteClip {
    #[getter]
    fn path(&self) -> String {
        self.inner.path().to_string_lossy().into_owned()
    }

    #[getter]
    fn duration_seconds(&self) -> f64 {
        self.inner.duration_seconds()
    }

    #[getter]
    fn file_size_bytes(&self) -> u64 {
        self.inner.file_size_bytes()
    }
}

#[pyclass(module = "rustic_audio_tool", name = "PttAudioEngine")]
struct PyPttAudioEngine {
    inner: Mutex<VoiceNoteEngine>,
}

#[pymethods]
impl PyPttAudioEngine {
    #[new]
    fn new(cache_dir: String) -> PyResult<Self> {
        Ok(Self {
            inner: Mutex::new(VoiceNoteEngine::new(cache_dir).map_err(to_py_runtime_error)?),
        })
    }

    fn cache_dir(&self) -> PyResult<String> {
        let engine = lock_engine(&self.inner)?;
        Ok(engine.cache_dir().to_string_lossy().into_owned())
    }

    fn set_cache_dir(&self, cache_dir: String) -> PyResult<()> {
        let mut engine = lock_engine(&self.inner)?;
        engine.set_cache_dir(cache_dir).map_err(to_py_runtime_error)
    }

    #[pyo3(signature = (stem=None))]
    fn start_capture(&self, stem: Option<String>) -> PyResult<String> {
        let mut engine = lock_engine(&self.inner)?;
        engine
            .start_capture(stem.as_deref())
            .map(|path| path.to_string_lossy().into_owned())
            .map_err(to_py_runtime_error)
    }

    fn stop_capture(&self) -> PyResult<PyVoiceNoteClip> {
        let mut engine = lock_engine(&self.inner)?;
        engine
            .stop_capture()
            .map(|inner| PyVoiceNoteClip { inner })
            .map_err(to_py_runtime_error)
    }

    fn cancel_capture(&self) -> PyResult<bool> {
        let mut engine = lock_engine(&self.inner)?;
        engine.cancel_capture().map_err(to_py_runtime_error)
    }

    fn play_preview(&self, path: String) -> PyResult<()> {
        let mut engine = lock_engine(&self.inner)?;
        engine.play_preview(path).map_err(to_py_runtime_error)
    }

    fn play_received(&self, path: String) -> PyResult<()> {
        let mut engine = lock_engine(&self.inner)?;
        engine.play_received(path).map_err(to_py_runtime_error)
    }

    fn stop_playback(&self) -> PyResult<()> {
        let mut engine = lock_engine(&self.inner)?;
        engine.stop_playback().map_err(to_py_runtime_error)
    }

    fn take_playback_error(&self) -> PyResult<Option<String>> {
        let engine = lock_engine(&self.inner)?;
        Ok(engine.take_playback_error())
    }

    fn is_recording(&self) -> PyResult<bool> {
        let engine = lock_engine(&self.inner)?;
        Ok(engine.is_recording())
    }

    fn is_playing(&self) -> PyResult<bool> {
        let engine = lock_engine(&self.inner)?;
        Ok(engine.is_playing())
    }
}

#[pymodule]
fn rustic_audio_tool(_py: Python<'_>, module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyPttAudioEngine>()?;
    module.add_class::<PyVoiceNoteClip>()?;
    Ok(())
}

fn lock_engine(engine: &Mutex<VoiceNoteEngine>) -> PyResult<std::sync::MutexGuard<'_, VoiceNoteEngine>> {
    engine
        .lock()
        .map_err(|_| PyRuntimeError::new_err("audio engine mutex poisoned"))
}

fn to_py_runtime_error(err: String) -> PyErr {
    PyRuntimeError::new_err(err)
}

