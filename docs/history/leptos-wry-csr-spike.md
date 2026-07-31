---
doc_id: history-leptos-wry-csr-spike
doc_type: history
plane: history
status: historical
authority: historical
summary: Preserved verdict and measurements from the retired standalone Leptos CSR and Wry IPC experiment, which is not an active Optimus product direction.
reviewed_on: 2026-08-01
review_by: never
---

# Retired Leptos CSR and Wry experiment

## Historical question

Spike 001 tested whether a standalone Leptos 0.8 CSR/WASM client could render a
compact desktop shell and use the existing `window.optimus` IPC contract through
the Wry desktop backend without changing production code.

## Preserved verdict

The result was partial. A release Trunk build mounted successfully, completed a
real `doctor` call and two offline chat turns, reused session state, maintained
reactive busy/input/message state, and passed a bounded auto-scroll measurement.
The optimized distribution measured 261,465 bytes: 217,025 bytes WASM, 38,425
bytes loader JavaScript, 4,157 bytes CSS, plus small HTML and bridge files.

It did not prove native Wry packaging, production asset embedding, control
parity, or Playwright parity. The test used the Rust HTTP mode and a Trunk proxy,
not the packaged native executable.

One reusable technical finding remains: `serde_wasm_bindgen::to_value` encoded
JSON objects as JavaScript `Map` values, which `JSON.stringify` reduced to `{}`.
Using `Serializer::json_compatible()` preserved the IPC parameter contract.

## Retirement

The active desktop interface is React, hosted by Electron on Linux, while Wry
remains a bounded fallback and current Windows surface. No production code,
workspace member, gate, or current plan depends on the Leptos experiment. Its
standalone Cargo workspace and private lockfile were removed on 2026-08-01 after
this durable verdict captured the only non-reproducible knowledge worth keeping.

This record is historical evidence, not permission to infer that Leptos is an
active or planned product direction.
