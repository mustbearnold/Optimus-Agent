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
        summary: format!(
            "offline vision: {}",
            req.prompt.chars().take(120).collect::<String>()
        ),
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

    #[test]
    fn media_fixtures_reject_missing_required_fields() {
        // Every offline fixture validates its required field; an empty or
        // whitespace-only value must fail closed rather than produce a stub.
        let v = vision_analyze_offline(&VisionAnalyzeRequest {
            image_ref: "".into(),
            prompt: "p".into(),
        });
        assert_eq!(v, Err(MediaError::Msg("image_ref required".into())));

        let v = vision_analyze_offline(&VisionAnalyzeRequest {
            image_ref: "   ".into(),
            prompt: "p".into(),
        });
        assert_eq!(v, Err(MediaError::Msg("image_ref required".into())));

        let i = image_generate_offline(&ImageGenerateRequest {
            prompt: "  ".into(),
            size: "512x512".into(),
        });
        assert_eq!(i, Err(MediaError::Msg("prompt required".into())));

        let s = stt_offline(&SttRequest {
            audio_ref: "".into(),
        });
        assert_eq!(s, Err(MediaError::Msg("audio_ref required".into())));

        let t = tts_offline(&TtsRequest { text: "".into() });
        assert_eq!(t, Err(MediaError::Msg("text required".into())));
    }

    #[test]
    fn vision_summary_truncates_long_prompts_to_120_chars() {
        // The offline vision summary caps the echoed prompt at 120 characters
        // so a huge prompt cannot blow up the reported summary. Pin both the
        // boundary and the count (chars, not bytes) so multi-byte input stays
        // bounded as well.
        let long = "x".repeat(500);
        let v = vision_analyze_offline(&VisionAnalyzeRequest {
            image_ref: "file:x.png".into(),
            prompt: long,
        })
        .unwrap();
        assert_eq!(v.summary.len(), "offline vision: ".len() + 120);

        let multi = "é".repeat(200); // 2-byte chars
        let v = vision_analyze_offline(&VisionAnalyzeRequest {
            image_ref: "file:x.png".into(),
            prompt: multi,
        })
        .unwrap();
        assert_eq!(v.summary.chars().count(), "offline vision: ".len() + 120);
    }
}
