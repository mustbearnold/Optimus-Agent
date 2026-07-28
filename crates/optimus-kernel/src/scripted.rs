//! The offline scripted model.
//!
//! Split out of `lib.rs` under architectural law 21. It is a `ModelProvider`
//! like any other — the fact that its answers come from a `Vec` instead of a
//! network is an implementation detail of one provider, not kernel machinery,
//! and it has no business sitting between the turn loop's own types.

use super::*;
use std::time::Duration;

/// Deterministic offline model: pops scripted responses in order.
#[derive(Debug, Default)]
pub struct ScriptedModel {
    pub script: Vec<CompletionResponse>,
    pub seen: Vec<CompletionRequest>,
    /// When true, `complete_streaming` emits text in small chunks (Playwright).
    pub stream_chunks: bool,
    /// Wall-clock pause before each streamed chunk. Zero by default, so every
    /// existing caller is unaffected; see [`ScriptedModel::paced`].
    pace: Duration,
}

/// How long the model may sleep before looking at the cancellation token again.
/// Short enough that an interrupt feels immediate to a person, long enough that
/// a several-second pace is not a busy-wait.
const PACE_SLICE: Duration = Duration::from_millis(20);

impl ScriptedModel {
    pub fn new(script: Vec<CompletionResponse>) -> Self {
        Self {
            script,
            seen: Vec::new(),
            stream_chunks: true,
            pace: Duration::ZERO,
        }
    }

    /// Take `pace` before emitting each chunk of streamed text.
    ///
    /// A fake that answers instantly cannot be interrupted, so everything that
    /// only exists *during* a turn — the interrupt key, the spinner, text
    /// arriving a piece at a time — is unreachable from a deterministic test.
    /// Slowing the fake down is not a hack bolted onto production: this model
    /// stands in for a network round trip, and a round trip takes time.
    /// Cancellation is honoured within [`PACE_SLICE`] rather than at the end of
    /// the pause, so an interrupt during a long pace still feels immediate.
    pub fn paced(mut self, pace: Duration) -> Self {
        self.pace = pace;
        self
    }

    /// Sleep out the pace, giving up early if the turn was cancelled. Returns
    /// whether the caller should keep streaming.
    fn take_pace(&self, cancellation: Option<&CancellationToken>) -> bool {
        let mut left = self.pace;
        while !left.is_zero() {
            if cancellation.is_some_and(CancellationToken::is_cancelled) {
                return false;
            }
            let slice = left.min(PACE_SLICE);
            std::thread::sleep(slice);
            left -= slice;
        }
        !cancellation.is_some_and(CancellationToken::is_cancelled)
    }

    /// Emit `text` the way the wire would: in pieces, with the configured pace
    /// between them, stopping the moment the turn is cancelled.
    fn stream_text(
        &self,
        text: &str,
        sink: &mut dyn FnMut(StreamEvent),
        cancellation: Option<&CancellationToken>,
    ) {
        if text.is_empty() {
            return;
        }
        if !self.stream_chunks {
            if self.take_pace(cancellation) {
                sink(StreamEvent::TextDelta(text.to_string()));
            }
            return;
        }
        // ~12 char chunks → visible progressive paint in UI tests
        let mut rest = text;
        while !rest.is_empty() {
            let mut end = rest.len().min(12);
            while !rest.is_char_boundary(end) {
                end -= 1;
            }
            if end == 0 {
                end = rest.chars().next().map(|c| c.len_utf8()).unwrap_or(1);
            }
            let (chunk, tail) = rest.split_at(end);
            if !self.take_pace(cancellation) {
                return;
            }
            sink(StreamEvent::TextDelta(chunk.to_string()));
            rest = tail;
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
        if let Some(text) = &resp.text {
            self.stream_text(text, sink, None);
        }
        Ok(resp)
    }

    /// Overridden because the default checks cancellation only before and after
    /// the whole call. With a pace set, that would make an interrupt land no
    /// sooner than the turn would have finished anyway — the token has to be
    /// visible *between* chunks to mean anything.
    fn complete_streaming_cancellable(
        &mut self,
        request: CompletionRequest,
        sink: &mut dyn FnMut(StreamEvent),
        cancellation: &CancellationToken,
    ) -> Result<CompletionResponse> {
        check_cancellation(cancellation)?;
        let resp = self.complete(request)?;
        if let Some(text) = &resp.text {
            self.stream_text(text, sink, Some(cancellation));
        }
        check_cancellation(cancellation)?;
        Ok(resp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> CompletionRequest {
        CompletionRequest {
            messages: Vec::new(),
            tools: Vec::new(),
            reasoning_effort: None,
            fast_mode: false,
        }
    }

    fn scripted(text: &str) -> ScriptedModel {
        ScriptedModel::new(vec![CompletionResponse {
            text: Some(text.into()),
            tool_calls: Vec::new(),
        }])
    }

    /// The behaviour the terminal gate depends on, checked here where it costs
    /// milliseconds instead of a spawned binary: a cancel arriving mid-answer
    /// stops the stream then, not after the remaining chunks have been paid for.
    #[test]
    fn a_cancel_mid_answer_stops_the_stream_at_the_next_chunk() {
        // Four chunks at twelve characters, so there is plenty left to emit.
        let mut model = scripted("the quick brown fox jumps over it").paced(PACE_SLICE);
        let cancellation = CancellationToken::new();
        let mut chunks = 0;
        let result = model.complete_streaming_cancellable(
            request(),
            &mut |event| {
                if matches!(event, StreamEvent::TextDelta(_)) {
                    chunks += 1;
                    cancellation.cancel();
                }
            },
            &cancellation,
        );
        assert_eq!(chunks, 1, "the stream ran on past the cancel");
        assert!(
            matches!(result, Err(KernelError::Cancelled)),
            "a cancelled turn must settle as cancelled, not as a short answer"
        );
    }

    /// Every caller that does not ask for a pace must be unaffected, which is
    /// what keeps this a development affordance rather than a tax on the suite.
    #[test]
    fn the_default_model_still_answers_without_pausing() {
        let mut model = scripted("hello there friend");
        let started = std::time::Instant::now();
        let mut chunks = 0;
        model
            .complete_streaming(request(), &mut |_| chunks += 1)
            .expect("the scripted answer");
        assert_eq!(chunks, 2, "eighteen characters in twelve-character chunks");
        assert!(
            started.elapsed() < PACE_SLICE,
            "an unpaced model paused anyway: {:?}",
            started.elapsed()
        );
    }
}
