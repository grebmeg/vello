// Copyright 2025 the Vello Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! This crate implements a CPU-based renderer, optimized for SIMD and multithreaded execution.
//! It is optimized for CPU-bound workloads and serves as a standalone renderer for systems
//! without GPU acceleration.

#![expect(
    clippy::cast_possible_truncation,
    reason = "We cast u16s to u8 in various places where we know for sure that it's < 256"
)]

mod render;

pub mod fine;
pub mod util;

pub use render::RenderContext;
