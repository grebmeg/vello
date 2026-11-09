// Copyright 2026 the Vello Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Temporary diagnostics for the Windows (ANGLE D3D11) WebGL probe failure.
//!
//! Copied into `vello_tests/tests/` by `.github/workflows/windows-probe-diag.yml` and run at
//! several commits. Always panics so the report is printed.

use std::fmt::Write as _;

use vello_common::probe::{ALL_PROBE_ELEMENTS, Probe, ProbeFeature};
use vello_gpu::WebGlProbeStatus;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use wasm_bindgen_test::wasm_bindgen_test;
use web_sys::{HtmlCanvasElement, WebGl2RenderingContext};

const CELL: usize = 14;
const PER_ROW: usize = 3;

const CANVA_ELEMENTS: &[ProbeFeature] = &[
    ProbeFeature::SolidRect,
    ProbeFeature::AlphaBlending,
    ProbeFeature::LinearGradient,
    ProbeFeature::RadialGradient,
    ProbeFeature::ImageNearest,
    ProbeFeature::ImageBilinear,
    ProbeFeature::OpacityLayer,
    ProbeFeature::Blending,
    ProbeFeature::Transformed,
    ProbeFeature::DepthBuffer,
    ProbeFeature::Filter,
];

fn canvas() -> HtmlCanvasElement {
    let document = web_sys::window().unwrap().document().unwrap();
    let canvas = document
        .create_element("canvas")
        .unwrap()
        .dyn_into::<HtmlCanvasElement>()
        .unwrap();
    canvas.set_width(200);
    canvas.set_height(200);
    canvas
}

fn renderer_string() -> String {
    let gl = canvas()
        .get_context("webgl2")
        .unwrap()
        .unwrap()
        .dyn_into::<WebGl2RenderingContext>()
        .unwrap();
    let _ = gl.get_extension("WEBGL_debug_renderer_info");
    let vendor = gl.get_parameter(0x9245).ok().and_then(|v| v.as_string());
    let renderer = gl.get_parameter(0x9246).ok().and_then(|v| v.as_string());
    format!("vendor={vendor:?} renderer={renderer:?}")
}

async fn wait_for_animation_frame() {
    let promise = web_sys::js_sys::Promise::new(&mut |resolve, _reject| {
        web_sys::window()
            .unwrap()
            .request_animation_frame(&resolve)
            .unwrap();
    });
    JsFuture::from(promise).await.unwrap();
}

/// Mirrors `PlatformCanvas::begin_hybrid_context` in Canva's renderer.
fn canva_settings() -> vello_gpu::RenderSettings {
    use vello_gpu::{AllocationStrategy, AtlasConfig, LayersConfig, MemorySettings, SizeU16};
    vello_gpu::RenderSettings::default().with_memory_settings(MemorySettings {
        image_atlas_config: AtlasConfig {
            initial_atlas_count: 1,
            max_atlases: 8,
            atlas_size: (4096, 4096),
            auto_grow: true,
            allocation_strategy: AllocationStrategy::FirstFit,
        },
        layers_config: LayersConfig {
            max_textures: Some(3),
            min_texture_size: SizeU16::new(512),
            max_texture_size: SizeU16::new(4096),
        },
    })
}

async fn canva_renderer(depth: bool) -> Result<vello_gpu::WebGlRenderer, String> {
    use vello_gpu::WebGlRendererInitStatus;
    let (mut init, _) = vello_gpu::WebGlRenderer::begin_with(&canvas(), canva_settings(), depth)
        .map_err(|e| format!("begin_with failed: {e:?}"))?;
    for _ in 0..600 {
        match init.try_finish().map_err(|e| format!("init failed: {e:?}"))? {
            WebGlRendererInitStatus::Complete(renderer) => return Ok(renderer),
            WebGlRendererInitStatus::Pending(next) => {
                init = next;
                wait_for_animation_frame().await;
            }
        }
    }
    Err("init timed out".into())
}

async fn run(report: &mut String, label: &str, elements: &[ProbeFeature], depth: bool) {
    let _ = writeln!(report, "\n=== {label} (depth={depth}) ===");
    let created = if label.starts_with("canva config") {
        canva_renderer(depth).await
    } else {
        vello_gpu::WebGlRenderer::new_with(&canvas(), vello_gpu::RenderSettings::default(), depth)
            .map(|(r, _)| r)
            .map_err(|e| format!("{e:?}"))
    };
    let mut renderer = match created {
        Ok(r) => r,
        Err(e) => {
            let _ = writeln!(report, "renderer creation failed: {e}");
            return;
        }
    };
    let mut pending = match renderer.probe(elements) {
        Ok(p) => p,
        Err(e) => {
            let _ = writeln!(report, "probe() failed: {e:?}");
            return;
        }
    };
    for _ in 0..600 {
        match pending.try_finish() {
            Ok(WebGlProbeStatus::Complete(Probe::Success)) => {
                let _ = writeln!(report, "SUCCESS");
                return;
            }
            Ok(WebGlProbeStatus::Complete(Probe::RenderError(e))) => {
                let _ = writeln!(report, "RENDER ERROR: {e:?}");
                return;
            }
            Ok(WebGlProbeStatus::Complete(Probe::Error(result))) => {
                let _ = writeln!(report, "MISMATCH");
                for s in result.statistics.iter().filter(|s| s.different_pixel_count != 0) {
                    let _ = writeln!(
                        report,
                        "  {:?}: {} px, max diff {:?}",
                        s.feature, s.different_pixel_count, s.max_channel_discrepancy
                    );
                }
                for (index, feature) in elements.iter().enumerate() {
                    if !matches!(feature, ProbeFeature::Filter | ProbeFeature::DropShadow) {
                        continue;
                    }
                    let (x0, y0) = ((index % PER_ROW) * CELL, (index / PER_ROW) * CELL);
                    let width = usize::from(result.actual.width);
                    let _ = writeln!(report, "  actual {feature:?} cell (RGBA hex):");
                    for y in y0..y0 + CELL {
                        let _ = write!(report, "   ");
                        for x in x0..x0 + CELL {
                            let p = &result.actual.data[(y * width + x) * 4..][..4];
                            let _ = write!(report, " {:02x}{:02x}{:02x}{:02x}", p[0], p[1], p[2], p[3]);
                        }
                        let _ = writeln!(report);
                    }
                }
                return;
            }
            Ok(WebGlProbeStatus::Pending(next)) => {
                pending = next;
                wait_for_animation_frame().await;
            }
            Err(e) => {
                let _ = writeln!(report, "readback failed: {e:?}");
                return;
            }
        }
    }
    let _ = writeln!(report, "TIMED OUT");
}

#[wasm_bindgen_test]
async fn probe_diag_report() {
    let mut report = String::new();
    let _ = writeln!(report, "{}", renderer_string());
    run(&mut report, "filter only", &[ProbeFeature::Filter], true).await;
    run(&mut report, "filter only", &[ProbeFeature::Filter], false).await;
    run(&mut report, "drop shadow only", &[ProbeFeature::DropShadow], true).await;
    run(&mut report, "canva elements", CANVA_ELEMENTS, true).await;
    run(&mut report, "canva config, canva elements", CANVA_ELEMENTS, true).await;
    run(&mut report, "canva config, canva elements", CANVA_ELEMENTS, false).await;
    run(&mut report, "canva config, filter only", &[ProbeFeature::Filter], true).await;
    run(&mut report, "all elements", ALL_PROBE_ELEMENTS, true).await;
    panic!("probe diagnostics report:\n{report}");
}
