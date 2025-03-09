// Copyright 2024 the Vello Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

#![allow(missing_docs, reason = "will add them later")]
#![allow(missing_debug_implementations, reason = "prototyping")]
#![allow(clippy::todo, reason = "still a prototype")]
#![allow(clippy::cast_possible_truncation, reason = "we need to do this a lot")]

pub mod common;
mod gpu;
mod gpu2;
#[cfg(feature = "perf_measurement")]
mod perf_measurement;
mod render;

pub use gpu::{Config, RenderData, Renderer};
#[cfg(feature = "perf_measurement")]
pub use perf_measurement::PerfMeasurement;
pub use render::RenderContext;
