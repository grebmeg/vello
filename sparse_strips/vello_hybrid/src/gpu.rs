// Copyright 2025 the Vello Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! GPU rendering module for the sparse strips CPU/GPU rendering engine.
//!
//! This module provides the GPU-side implementation of the hybrid rendering system.
//! It handles:
//! - GPU resource management (buffers, textures, pipelines)
//! - Surface/window management and presentation
//! - Shader execution and rendering
//!
//! The hybrid approach combines CPU-side path processing with efficient GPU rendering
//! to balance flexibility and performance.

use std::fmt::Debug;

use bytemuck::{Pod, Zeroable};
use std::sync::Arc;
use wgpu::{
    BindGroup, BindGroupLayout, BlendState, Buffer, ColorTargetState, ColorWrites, Device,
    PipelineCompilationOptions, Queue, RenderPipeline, Surface, SurfaceConfiguration, Texture,
    WindowHandle, util::DeviceExt,
};

/// Represents the target for rendering - either a window or specific dimensions
#[derive(Debug)]
pub enum RenderTarget {
    /// Render to a window
    Surface {
        /// The window to render to
        target: Arc<dyn SurfaceTarget>,
        /// Width of the window in pixels
        width: u32,
        /// Height of the window in pixels
        height: u32,
    },
    /// Render to a texture with specific dimensions
    Headless {
        /// Width of the texture in pixels
        width: u32,
        /// Height of the texture in pixels
        height: u32,
    },
}

impl RenderTarget {
    /// Get the dimensions of the render target
    pub fn dimensions(&self) -> (u32, u32) {
        match self {
            Self::Surface { width, height, .. } => (*width, *height),
            Self::Headless { width, height } => (*width, *height),
        }
    }
}

/// Trait for a surface target that can be rendered to
pub trait SurfaceTarget: WindowHandle + Debug {}
impl<T> SurfaceTarget for T where T: WindowHandle + Debug {}

/// Contains all GPU resources needed for rendering
#[derive(Debug)]
struct GpuResources {
    /// Buffer for strip data
    pub strips_buffer: Buffer,
    /// Texture for alpha values
    pub alphas_texture: Texture,
    /// Bind group for rendering
    pub render_bind_group: BindGroup,
}

/// GPU renderer for the hybrid rendering system
#[derive(Debug)]
pub struct Renderer {
    /// The GPU device
    pub device: Device,
    /// Command queue for the GPU
    pub queue: Queue,
    /// Surface for presenting rendered content, if applicable
    pub surface: Option<Surface<'static>>,
    /// Configuration for the surface
    pub surface_config: SurfaceConfiguration,
    /// Bind group layout for rendering
    pub render_bind_group_layout: BindGroupLayout,
    /// Pipeline for rendering
    pub render_pipeline: RenderPipeline,
    /// Buffer for configuration data
    pub config_buf: Buffer,
    /// GPU resources for rendering (created during prepare)
    resources: Option<GpuResources>,
}

/// Configuration for the GPU renderer
#[repr(C)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
pub struct Config {
    /// Width of the rendering target
    pub width: u32,
    /// Height of the rendering target
    pub height: u32,
    /// Height of a strip in the rendering
    pub strip_height: u32,
}

/// Contains the data needed for rendering
#[derive(Debug, Default)]
pub struct RenderData {
    /// GPU strips to be rendered
    pub strips: Vec<GpuStrip>,
    /// Alpha values used in rendering
    pub alphas: Vec<u32>,
}

/// Represents a GPU strip for rendering
#[repr(C)]
#[derive(Debug, Clone, Copy, Zeroable, Pod)]
pub struct GpuStrip {
    /// X coordinate of the strip
    pub x: u16,
    /// Y coordinate of the strip
    pub y: u16,
    /// Width of the strip
    pub width: u16,
    /// Width of the portion where alpha blending should be applied.
    pub dense_width: u16,
    /// Index into the alpha texture where this strip's alpha values begin.
    pub col: u32,
    /// RGBA color value
    pub rgba: u32,
}

impl GpuStrip {
    /// Vertex attributes for the strip
    pub fn vertex_attributes() -> [wgpu::VertexAttribute; 4] {
        wgpu::vertex_attr_array![
            0 => Uint32,
            1 => Uint32,
            2 => Uint32,
            3 => Uint32,
        ]
    }
}

impl Renderer {
    /// Creates a new renderer
    ///
    /// The target parameter determines if we render to a window or headless
    pub async fn new(render_target: RenderTarget) -> Self {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            flags: wgpu::InstanceFlags::empty(),
            backend_options: wgpu::BackendOptions::default(),
        });

        let (dimensions, surface) = match render_target {
            RenderTarget::Surface {
                target,
                width,
                height,
            } => {
                let surface = instance
                    .create_surface(Arc::clone(&target))
                    .expect("Failed to create surface");
                ((width, height), Some(surface))
            }
            RenderTarget::Headless { width, height } => ((width, height), None),
        };

        // Get adapter with or without surface
        let adapter = if let Some(surface) = &surface {
            instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: Default::default(),
                    force_fallback_adapter: false,
                    compatible_surface: Some(surface),
                })
                .await
                .expect("Failed to find an appropriate adapter")
        } else {
            instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: Default::default(),
                    force_fallback_adapter: false,
                    compatible_surface: None,
                })
                .await
                .expect("Failed to find an appropriate adapter")
        };

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("Vello Hybrid"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::downlevel_webgl2_defaults(),
                    memory_hints: wgpu::MemoryHints::default(),
                },
                None,
            )
            .await
            .expect("Failed to create device");

        let (width, height) = dimensions;
        let format = wgpu::TextureFormat::Bgra8Unorm;
        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width,
            height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: wgpu::CompositeAlphaMode::Opaque,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };

        // Configure surface if it exists
        if let Some(surface) = &surface {
            surface.configure(&device, &surface_config);
        }

        let render_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../shaders/sparse_strip_renderer.wgsl").into(),
            ),
        });
        let render_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: None,
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Uint,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&render_bind_group_layout],
            push_constant_ranges: &[],
        });
        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &render_shader,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<GpuStrip>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &GpuStrip::vertex_attributes(),
                }],
                compilation_options: PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &render_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(ColorTargetState {
                    format,
                    blend: Some(BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: ColorWrites::ALL,
                })],
                compilation_options: PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let config = Config {
            width,
            height,
            strip_height: 4,
        };
        let config_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Config Buffer"),
            contents: bytemuck::bytes_of(&config),
            usage: wgpu::BufferUsages::UNIFORM,
        });

        Self {
            device,
            queue,
            surface,
            surface_config,
            render_bind_group_layout,
            render_pipeline,
            config_buf,
            resources: None,
        }
    }

    /// Prepare the GPU buffers for rendering
    pub fn prepare(&mut self, render_data: &RenderData) {
        let required_strips_size =
            std::mem::size_of::<GpuStrip>() as u64 * render_data.strips.len() as u64;

        // Check if we need to create new resources or resize existing ones
        let needs_new_resources = match &self.resources {
            Some(resources) => required_strips_size > resources.strips_buffer.size(),
            None => true,
        };

        if needs_new_resources {
            let strips_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Strips Buffer"),
                size: required_strips_size,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            let max_texture_dimension_2d = self.device.limits().max_texture_dimension_2d;
            let alpha_len = render_data.alphas.len();
            let alpha_texture_height =
                (u32::try_from(alpha_len).unwrap()).div_ceil(max_texture_dimension_2d);

            // Ensure dimensions don't exceed WebGL2 limits
            assert!(
                alpha_texture_height <= max_texture_dimension_2d,
                "Alpha texture height exceeds WebGL2 limit"
            );

            let alphas_texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Alpha Texture"),
                size: wgpu::Extent3d {
                    width: max_texture_dimension_2d,
                    height: alpha_texture_height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R32Uint,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let alphas_texture_view =
                alphas_texture.create_view(&wgpu::TextureViewDescriptor::default());

            let render_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Render Bind Group"),
                layout: &self.render_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&alphas_texture_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: self.config_buf.as_entire_binding(),
                    },
                ],
            });

            self.resources = Some(GpuResources {
                strips_buffer,
                alphas_texture,
                render_bind_group,
            });
        }

        // Now that we have resources, we can update the data
        if let Some(resources) = &self.resources {
            // TODO: Explore using `write_buffer_with` to avoid copying the data twice
            self.queue.write_buffer(
                &resources.strips_buffer,
                0,
                bytemuck::cast_slice(&render_data.strips),
            );

            // Prepare alpha data for the texture
            let texture_width = resources.alphas_texture.width();
            let texture_height = resources.alphas_texture.height();
            let mut alpha_data = vec![0_u32; render_data.alphas.len()];
            alpha_data[..].copy_from_slice(&render_data.alphas[..]);
            alpha_data.resize((texture_width * texture_height) as usize, 0);

            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &resources.alphas_texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x: 0, y: 0, z: 0 },
                    aspect: wgpu::TextureAspect::All,
                },
                bytemuck::cast_slice(&alpha_data),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    // 4 bytes per u32
                    bytes_per_row: Some(texture_width * 4),
                    rows_per_image: Some(texture_height),
                },
                wgpu::Extent3d {
                    width: texture_width,
                    height: texture_height,
                    depth_or_array_layers: 1,
                },
            );
        }
    }

    /// Render to the surface
    pub fn render_to_surface(&self, render_data: &RenderData) {
        // If we don't have the required resources or surface, return early
        let (resources, surface) = match (&self.resources, &self.surface) {
            (Some(r), Some(s)) => (r, s),
            _ => return,
        };

        let frame = surface
            .get_current_texture()
            .expect("Failed to get current texture");

        let mut encoder = self.device.create_command_encoder(&Default::default());

        {
            let view = frame
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_bind_group(0, &resources.render_bind_group, &[]);
            render_pass.set_vertex_buffer(0, resources.strips_buffer.slice(..));
            let strips_to_draw = render_data.strips.len();
            render_pass.draw(0..4, 0..u32::try_from(strips_to_draw).unwrap());
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        frame.present();
    }

    /// Render to a texture
    pub fn render_to_texture(&self, render_data: &RenderData, width: u32, height: u32) -> Vec<u8> {
        // If we don't have the required resources, return empty data
        let Some(resources) = &self.resources else {
            return Vec::new();
        };

        // Create a texture to render to
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Render to Texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        // Create a buffer to copy the texture data to
        let bytes_per_row = (width * 4).next_multiple_of(256);
        let texture_copy_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Texture Copy to Buffer"),
            size: bytes_per_row as u64 * height as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder = self.device.create_command_encoder(&Default::default());

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Redner to Texture Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &texture_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_bind_group(0, &resources.render_bind_group, &[]);
            render_pass.set_vertex_buffer(0, resources.strips_buffer.slice(..));
            let strips_to_draw = render_data.strips.len();
            render_pass.draw(0..4, 0..u32::try_from(strips_to_draw).unwrap());
        }

        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &texture_copy_buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bytes_per_row),
                    rows_per_image: None,
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        self.queue.submit([encoder.finish()]);

        // Map the buffer to read the data
        texture_copy_buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                if result.is_err() {
                    panic!("Failed to map texture for reading");
                }
            });
        self.device.poll(wgpu::Maintain::Wait);

        // Copy the data into a Vec<u8>
        let mut img_data = Vec::with_capacity((width * height * 4) as usize);
        for row in texture_copy_buffer
            .slice(..)
            .get_mapped_range()
            .chunks_exact(bytes_per_row as usize)
        {
            img_data.extend_from_slice(&row[0..width as usize * 4]);
        }

        texture_copy_buffer.unmap();

        img_data
    }
}
