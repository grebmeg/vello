// Copyright 2025 the Vello Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A "typical design" scene mixing common element types.
//!
//! For now this only draws a single rectangle in the center of the viewport. It is intended to
//! grow into a representative mixed-content design (images, text, vector paths, gradients) used as
//! the headline workload for performance regression tracking.

use vello_common::color::palette;
use vello_common::kurbo::{Affine, Rect};

use crate::{ExampleScene, RenderingContext};

/// A mixed "typical design" scene.
#[derive(Debug)]
pub struct MixedScene {}

impl ExampleScene for MixedScene {
    fn render<T: RenderingContext>(
        &mut self,
        target: &mut T,
        _resources: &mut T::Resources,
        root_transform: Affine,
    ) {
        render(target, root_transform);
    }
}

impl MixedScene {
    /// Create a new `MixedScene`.
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for MixedScene {
    fn default() -> Self {
        Self::new()
    }
}

/// Draw the mixed design: a centered rectangle covering half the viewport.
pub fn render(ctx: &mut impl RenderingContext, root_transform: Affine) {
    let width = f64::from(ctx.width());
    let height = f64::from(ctx.height());

    let rect_width = width * 0.5;
    let rect_height = height * 0.5;
    let x0 = (width - rect_width) / 2.0;
    let y0 = (height - rect_height) / 2.0;
    let rect = Rect::new(x0, y0, x0 + rect_width, y0 + rect_height);

    ctx.set_transform(root_transform);
    ctx.set_paint(palette::css::REBECCA_PURPLE);
    ctx.fill_rect(&rect);
}
