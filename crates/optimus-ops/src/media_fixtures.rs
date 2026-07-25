//! Offline media fixtures (Track Z.7–Z.9) — vision analyze, image generate, STT/TTS mocks.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MediaError {
    #[error("{0}")]
    Msg(String),
}

pub type Result<T> = std::result::Result<T, MediaError>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VisionAnalyzeRequest {
    /// Data URL or path label (offline fixture ignores bytes).
    pub image_ref: String,
    pub prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VisionAnalyzeResult {
    pub summary: String,
    pub labels: Vec<String>,
    pub offline: bool,
}

pub fn vision_analyze_offline(req: &VisionAnalyzeRequest) -> Result<VisionAnalyzeResult> {
    if req.image_ref.trim().is_empty() {
        return Err(MediaError::Msg("image_ref required".into()));
    }
    Ok(VisionAnalyzeResult {
        summary: format!("offline vision: {}", req.prompt.chars().take(120).collect::<String>()),
        labels: vec!["fixture".into(), "offline".into()],
        offline: true,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImageGenerateRequest {
    pub prompt: String,
    #[serde(default = "default_size")]
    pub size: String,
}

fn default_size() -> String {
    "512x512".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImageGenerateResult {
    pub image_ref: String,
    pub prompt: String,
    pub offline: bool,
}

pub fn image_generate_offline(req: &ImageGenerateRequest) -> Result<ImageGenerateResult> {
    if req.prompt.trim().is_empty() {
        return Err(MediaError::Msg("prompt required".into()));
    }
    Ok(ImageGenerateResult {
        image_ref: format!("offline-image:{}:{}", req.size, req.prompt.len()),
        prompt: req.prompt.clone(),
        offline: true,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SttRequest {
    pub audio_ref: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SttResult {
    pub text: String,
    pub offline: bool,
}

pub fn stt_offline(req: &SttRequest) -> Result<SttResult> {
    if req.audio_ref.trim().is_empty() {
        return Err(MediaError::Msg("audio_ref required".into()));
    }
    Ok(SttResult {
        text: "[offline stt transcript]".into(),
        offline: true,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TtsRequest {
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TtsResult {
    pub audio_ref: String,
    pub offline: bool,
}

pub fn tts_offline(req: &TtsRequest) -> Result<TtsResult> {
    if req.text.trim().is_empty() {
        return Err(MediaError::Msg("text required".into()));
    }
    Ok(TtsResult {
        audio_ref: format!("offline-audio:{}", req.text.len()),
        offline: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_offline_fixtures() {
        let v = vision_analyze_offline(&VisionAnalyzeRequest {
            image_ref: "file:x.png".into(),
            prompt: "what is this".into(),
        })
        .unwrap();
        assert!(v.offline);
        let i = image_generate_offline(&ImageGenerateRequest {
            prompt: "a cat".into(),
            size: "256x256".into(),
        })
        .unwrap();
        assert!(i.image_ref.contains("offline-image"));
        let s = stt_offline(&SttRequest {
            audio_ref: "a.wav".into(),
        })
        .unwrap();
        assert!(s.text.contains("stt"));
        let t = tts_offline(&TtsRequest {
            text: "hello".into(),
        })
        .unwrap();
        assert!(t.audio_ref.contains("offline-audio"));
    }
}
