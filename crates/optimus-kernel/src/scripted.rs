//! The offline scripted model.
//!
//! Split out of `lib.rs` under architectural law 21. It is a `ModelProvider`
//! like any other — the fact that its answers come from a `Vec` instead of a
//! network is an implementation detail of one provider, not kernel machinery,
//! and it has no business sitting between the turn loop's own types.

use super::*;

/// Deterministic offline model: pops scripted responses in order.
#[derive(Debug, Default)]
pub struct ScriptedModel {
    pub script: Vec<CompletionResponse>,
    pub seen: Vec<CompletionRequest>,
    /// When true, `complete_streaming` emits text in small chunks (Playwright).
    pub stream_chunks: bool,
}

impl ScriptedModel {
    pub fn new(script: Vec<CompletionResponse>) -> Self {
        Self {
            script,
            seen: Vec::new(),
            stream_chunks: true,
        }
    }
}

impl ModelProvider for ScriptedModel {
    fn identity(&self) -> (String, String) {
        ("offline".into(), "offline-scripted".into())
    }

    fn complete(&mut self, request: CompletionRequest) -> Result<CompletionResponse> {
        self.seen.push(request);
        if self.script.is_empty() {
            return Err(KernelError::Model("script exhausted".into()));
        }
        Ok(self.script.remove(0))
    }

    fn complete_streaming(
        &mut self,
        request: CompletionRequest,
        sink: &mut dyn FnMut(StreamEvent),
    ) -> Result<CompletionResponse> {
        let resp = self.complete(request)?;
        if let Some(t) = &resp.text {
            if self.stream_chunks && !t.is_empty() {
                // ~12 char chunks → visible progressive paint in UI tests
                let mut rest = t.as_str();
                while !rest.is_empty() {
                    let mut end = rest.len().min(12);
                    while !rest.is_char_boundary(end) {
                        end -= 1;
                    }
                    if end == 0 {
                        end = rest.chars().next().map(|c| c.len_utf8()).unwrap_or(1);
                    }
                    let (chunk, tail) = rest.split_at(end);
                    sink(StreamEvent::TextDelta(chunk.to_string()));
                    rest = tail;
                }
            } else if !t.is_empty() {
                sink(StreamEvent::TextDelta(t.clone()));
            }
        }
        Ok(resp)
    }
}
