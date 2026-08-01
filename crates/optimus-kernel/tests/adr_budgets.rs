//! The ADR-0047/0048 budgets, pinned so a refactor cannot quietly starve
//! turns or shrink history back to the numbers those decisions replaced.

use optimus_kernel::{CompressionConfig, KernelConfig};

#[test]
fn the_turn_budget_is_thirty_two_steps_of_at_most_eight_calls() {
    let config = KernelConfig::default();
    assert_eq!(config.max_steps, 32, "ADR-0047: eight starved real turns");
    assert_eq!(
        config.max_tool_calls_per_step, 8,
        "ADR-0047 keeps the per-step call cap; only the step count changed"
    );
}

#[test]
fn the_history_budget_converges_rather_than_churns() {
    let compression = CompressionConfig::default();
    assert_eq!(compression.max_message_chars, 200_000, "ADR-0048");
    assert_eq!(compression.max_tool_result_chars, 24_000, "ADR-0048");
    assert!(
        compression.keep_tail_messages * compression.max_tool_result_chars
            < compression.max_message_chars,
        "a kept tail larger than the whole budget makes compression churn \
         instead of converge (ADR-0048)"
    );
}
