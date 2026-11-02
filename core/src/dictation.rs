//! Dictation (B11): speech to text on this device, with whisper.cpp.
//!
//! The app records the microphone and hands over 16 kHz mono 16-bit PCM;
//! [`transcribe`] returns the words. Nothing leaves the device. The speech
//! model (Whisper "base.en", 142 MB) is not shipped with the app: it is
//! downloaded once, on first use and only when asked, into
//! `Graphite/models` in the user's local data folder, and checked against
//! the SHA-1 whisper.cpp publishes for it.
//!
//! The recognition itself needs the `dictation` feature (whisper.cpp built
//! from source, so cmake and a C++ compiler); without it [`available`] is
//! false and [`transcribe`] says so.

use std::fs;
use std::io::Read;
use std::path::PathBuf;

use sha1::{Digest, Sha1};

use crate::error::{Error, Result};
use crate::sources::{fetch_to_file_with, ImportControl};

/// Audio [`transcribe`] expects: mono, this many samples a second.
pub const SAMPLE_RATE: u32 = 16_000;
/// The longest recording transcribed in one go.
pub const MAX_SECONDS: usize = 15 * 60;

const MODEL_FILE: &str = "ggml-base.en.bin";
const MODEL_SHA1: &str = "137c40403d78fd54d454da0f9bd998f78703390c";
/// The model's size in megabytes, to say before downloading it.
pub const MODEL_MB: u32 = 142;

fn other(msg: impl Into<String>) -> Error {
    Error::Io(std::io::Error::other(msg.into()))
}

fn model_url() -> String {
    std::env::var("GRAPHITE_WHISPER_MODEL_URL")
        .unwrap_or_else(|_| format!("https://huggingface.co/ggerganov/whisper.cpp/resolve/main/{MODEL_FILE}"))
}

/// Where the speech model lives: `GRAPHITE_WHISPER_MODEL` if set (any
/// whisper.cpp model, for testing), else `Graphite/models/ggml-base.en.bin`
/// in the user's local data folder.
pub fn model_path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("GRAPHITE_WHISPER_MODEL") {
        return Some(PathBuf::from(p));
    }
    dirs::data_local_dir().map(|d| d.join("Graphite").join("models").join(MODEL_FILE))
}

/// Whether this build can transcribe at all.
pub fn available() -> bool {
    cfg!(feature = "dictation")
}

/// Whether the speech model is on this device.
pub fn model_ready() -> bool {
    model_path().is_some_and(|p| p.is_file())
}

fn sha1_of(path: &std::path::Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha1::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Download the speech model (once). Progress and cancelling go through
/// `ctl`, like any other download.
pub fn download_model(ctl: &dyn ImportControl) -> Result<PathBuf> {
    let dest = model_path().ok_or_else(|| other("no folder for the speech model on this system"))?;
    if dest.is_file() {
        return Ok(dest);
    }
    let dir = dest.parent().ok_or_else(|| other("no folder for the speech model"))?;
    fs::create_dir_all(dir)?;
    let part = dest.with_extension("bin.part");
    let fetched = fetch_to_file_with(&model_url(), &part, ctl, "Downloading the speech model").and_then(|_| {
        if sha1_of(&part)? != MODEL_SHA1 {
            return Err(other("the speech model download did not match its published checksum"));
        }
        Ok(())
    });
    if let Err(e) = fetched {
        let _ = fs::remove_file(&part);
        return Err(e);
    }
    fs::rename(&part, &dest)?;
    Ok(dest)
}

/// Little-endian 16-bit PCM to the -1..1 floats Whisper takes.
pub fn pcm16_to_f32(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(2)
        .map(|b| f32::from(i16::from_le_bytes([b[0], b[1]])) / 32768.0)
        .collect()
}

/// Whisper's segments joined into text: whitespace tidied, and the
/// bracketed non-speech markers it emits for silence or noise
/// (`[BLANK_AUDIO]`, `(wind blowing)`) dropped.
pub fn tidy_transcript<'a>(segments: impl IntoIterator<Item = &'a str>) -> String {
    let mut out: Vec<&str> = Vec::new();
    for seg in segments {
        let seg = seg.trim();
        let marker = (seg.starts_with('[') && seg.ends_with(']')) || (seg.starts_with('(') && seg.ends_with(')'));
        if seg.is_empty() || marker {
            continue;
        }
        out.push(seg);
    }
    out.join(" ").split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(feature = "dictation")]
mod engine {
    use std::sync::Mutex;

    use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

    use super::{model_path, other, tidy_transcript, MAX_SECONDS, SAMPLE_RATE};
    use crate::error::Result;

    /// The loaded model, kept between dictations (loading takes a moment).
    static MODEL: Mutex<Option<WhisperContext>> = Mutex::new(None);

    pub fn transcribe(samples: &[f32]) -> Result<String> {
        if samples.len() > MAX_SECONDS * SAMPLE_RATE as usize {
            return Err(other(
                "That recording is too long; dictate in parts of up to 15 minutes.",
            ));
        }
        // Under half a second of audio is too little to recognise.
        if samples.len() < SAMPLE_RATE as usize / 2 {
            return Ok(String::new());
        }
        let mut guard = MODEL.lock().unwrap_or_else(|e| e.into_inner());
        if guard.is_none() {
            let path = model_path()
                .filter(|p| p.is_file())
                .ok_or_else(|| other("The speech model is not downloaded yet."))?;
            whisper_rs::install_logging_hooks();
            let ctx = WhisperContext::new_with_params(&path, WhisperContextParameters::default())
                .map_err(|e| other(format!("could not load the speech model: {e}")))?;
            *guard = Some(ctx);
        }
        let ctx = guard.as_ref().expect("loaded above");
        let mut state = ctx
            .create_state()
            .map_err(|e| other(format!("speech recognition failed: {e}")))?;
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).clamp(1, 8);
        params.set_n_threads(threads as i32);
        params.set_language(Some("en"));
        params.set_no_context(true);
        params.set_suppress_blank(true);
        params.set_suppress_nst(true);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_special(false);
        params.set_print_timestamps(false);
        state
            .full(params, samples)
            .map_err(|e| other(format!("speech recognition failed: {e}")))?;
        let segments: Vec<String> = state
            .as_iter()
            .filter_map(|s| s.to_str_lossy().ok().map(|t| t.into_owned()))
            .collect();
        Ok(tidy_transcript(segments.iter().map(String::as_str)))
    }
}

/// The words in `samples` (16 kHz mono, -1..1), recognised on this device.
pub fn transcribe(samples: &[f32]) -> Result<String> {
    #[cfg(feature = "dictation")]
    {
        engine::transcribe(samples)
    }
    #[cfg(not(feature = "dictation"))]
    {
        let _ = samples;
        Err(other("Dictation is not part of this build of Graphite."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pcm_converts_and_transcripts_are_tidied() {
        let bytes = [0x00, 0x80, 0x00, 0x00, 0xff, 0x7f];
        let s = pcm16_to_f32(&bytes);
        assert_eq!(s.len(), 3);
        assert_eq!(s[0], -1.0);
        assert_eq!(s[1], 0.0);
        assert!((s[2] - 1.0).abs() < 0.001);
        assert_eq!(
            tidy_transcript([" Buy  more", "[BLANK_AUDIO]", " (wind blowing) ", " 18650 cells. "]),
            "Buy more 18650 cells."
        );
        assert_eq!(tidy_transcript(["[MUSIC]"]), "");
    }

    #[test]
    fn the_model_lives_in_the_local_data_folder() {
        if std::env::var_os("GRAPHITE_WHISPER_MODEL").is_none() {
            if let Some(p) = model_path() {
                assert!(p.ends_with("Graphite/models/ggml-base.en.bin"));
            }
        }
    }

    /// Runs only with `--features dictation` and `GRAPHITE_WHISPER_MODEL`
    /// pointing at a model, plus `GRAPHITE_WHISPER_SAMPLE` at a 16 kHz
    /// mono 16-bit WAV (whisper.cpp's `samples/jfk.wav`).
    #[cfg(feature = "dictation")]
    #[test]
    fn transcribes_a_sample_when_one_is_given() {
        let (Some(_), Some(wav)) = (
            std::env::var_os("GRAPHITE_WHISPER_MODEL"),
            std::env::var_os("GRAPHITE_WHISPER_SAMPLE"),
        ) else {
            return;
        };
        let bytes = fs::read(wav).unwrap();
        let text = transcribe(&pcm16_to_f32(&bytes[44..])).unwrap();
        assert!(text.to_lowercase().contains("ask not"), "{text}");
    }
}
