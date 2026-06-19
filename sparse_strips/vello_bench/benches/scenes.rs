// Copyright 2025 the Vello Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Scene-level performance regression benchmarks.
//!
//! These render whole example "designs" end to end through the single-threaded `vello_cpu`
//! pipeline. They use the `CodSpeed` Criterion compatibility layer so they can be tracked for
//! performance regressions in CI, while still behaving like ordinary Criterion benchmarks when run
//! locally with `cargo bench`.

#![allow(missing_docs, reason = "Not needed for benchmarks")]

use codspeed_criterion_compat::{Criterion, criterion_group, criterion_main};
use vello_common::kurbo::Affine;
use vello_common::pixmap::Pixmap;
use vello_cpu::{Level, RenderContext, RenderSettings, Resources};
use vello_example_scenes::ExampleScene;
use vello_example_scenes::mixed::MixedScene;

const VIEWPORT_WIDTH: u16 = 1280;
const VIEWPORT_HEIGHT: u16 = 960;

/// Benchmark full-frame rendering of a single scene with the single-threaded CPU renderer.
///
/// The renderer, resources and target pixmap are created once outside the timed loop so the
/// measurement reflects scene encoding plus rasterization rather than buffer allocation.
fn bench_scene(c: &mut Criterion, name: &str, mut scene: impl ExampleScene) {
    // Pin the SIMD level and force single-threaded rendering for deterministic, CodSpeed-safe
    // measurements. `baseline` (scalar) avoids SIMD instructions that the simulation instrument
    // may not support; switch to `Level::try_detect()` once running on a walltime runner if SIMD
    // throughput numbers are desired.
    let settings = RenderSettings {
        level: Level::baseline(),
        num_threads: 0,
    };
    let mut ctx = RenderContext::new_with(VIEWPORT_WIDTH, VIEWPORT_HEIGHT, settings);
    let mut resources = Resources::default();
    let mut pixmap = Pixmap::new(VIEWPORT_WIDTH, VIEWPORT_HEIGHT);

    c.bench_function(name, |b| {
        b.iter(|| {
            ctx.reset();
            scene.render(&mut ctx, &mut resources, Affine::IDENTITY);
            ctx.flush();
            ctx.render(&mut pixmap, &mut resources);
            std::hint::black_box(&pixmap);
        });
    });
}

/// Benchmark all curated scene workloads.
pub fn scenes(c: &mut Criterion) {
    bench_scene(c, "scenes/typical_design", MixedScene::new());
}

criterion_group!(scenes_group, scenes);
criterion_main!(scenes_group);
