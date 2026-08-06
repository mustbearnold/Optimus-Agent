//! One core per home (criterion C3, docs/architecture/north-star-2026-07.md).
//!
//! The record implementation lives in optimus-host
//! (`crates/optimus-host/src/record.rs`) so the serve process and every
//! Phase-B surface share one read/write/probe implementation (spec-015 A1);
//! this module is the desktop shell's re-export shim and stays the desktop's
//! documented call surface.
//!
//! Record version 2 (spec-015 A1): the surviving `--host-only` writer emits
//! v2 with `transport:"http"`; `optimus serve` emits v2 with `"ws"`;
//! `read_record` is known-version-tolerant (v1 records read back as
//! HTTP-mode holders).

pub use optimus_host::{healthy_serving_port, write_record};
