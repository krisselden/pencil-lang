//! Draws a figure's emitted WGSL with wgpu. The pipeline is shared by the native
//! offscreen path and the browser, so both run the same shader.

use pencil_core::eval::srgb_encode;
use pencil_core::frame::{
    GEOMETRY_VEC4S, INK_TEXTURE_WIDTH, Prepared, SIDE_TEXTURE_WIDTH, VALUE_VEC4S, side_rows,
};
use pencil_core::nearest;
use pencil_core::{Figure, stroke, style, wgsl};

/// A frame's inputs, matching `Uniforms` in `pencil_core::wgsl`.
#[derive(Clone, Copy, Debug)]
pub struct FrameInputs {
    pub time: f32,
    pub width: u32,
    pub height: u32,
    /// Device pixels per `px`, so `width: 3px` looks the same on any screen.
    pub dpr: f32,
    /// Device pixels per `em`, the reader's text size.
    pub em: f32,
    /// Where this panel's corner is on the canvas. `width` and `height` are the panel's.
    pub origin: [u32; 2],
}

impl FrameInputs {
    fn bytes(self, prepared: &Prepared) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(UNIFORM_BYTES);
        bytes.extend_from_slice(&self.time.to_le_bytes());
        bytes.extend_from_slice(&(self.width as f32).to_le_bytes());
        bytes.extend_from_slice(&(self.height as f32).to_le_bytes());
        bytes.extend_from_slice(&self.dpr.to_le_bytes());
        let view = &prepared.view;
        for v in [view.x[0], view.x[1], view.y[0], view.y[1]] {
            bytes.extend_from_slice(&(v as f32).to_le_bytes());
        }
        // A vec2 and the em, then padding, since the array after it starts on a vec4.
        for v in [self.origin[0] as f32, self.origin[1] as f32, self.em, 0.0] {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        for v in &prepared.values {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        bytes.resize(UNIFORM_BYTES, 0);
        bytes
    }
}

const UNIFORM_BYTES: usize = 48 + VALUE_VEC4S * 16;
const GEOMETRY_BYTES: usize = GEOMETRY_VEC4S * 16;

const COVERAGE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R8Unorm;
/// A stroke whose lines each have their own color: coverage, and which color, a byte at a time.
const INKED_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
/// How little a segment covers a pixel, so a depth test keeps the one that covers it most.
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// One coverage layer per traced curve, and one per stroke whose lines each have their own
/// color. Each is always an array of at least two layers, because wgpu's GL backend can't treat
/// a one-layer texture as an array.
struct Coverage {
    texture: wgpu::Texture,
    /// A one-texel stand-in when no stroke has lines of their own color, as the layout needs one.
    inked: wgpu::Texture,
    /// Only when a stroke has lines of their own color.
    depth: Option<wgpu::TextureView>,
    bind_group: wgpu::BindGroup,
    width: u32,
    height: u32,
}

/// The texture of lines' own colors, `rows` high, or a texel standing in for it when there are none.
fn ink_texture(device: &wgpu::Device, rows: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("pencil inks"),
        size: wgpu::Extent3d {
            width: if rows > 0 {
                INK_TEXTURE_WIDTH as u32
            } else {
                1
            },
            height: rows.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba32Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
        texture_binding_view_dimension: None,
    })
}

/// The texture of traced curves, `rows` high.
fn nearest_texture(device: &wgpu::Device, rows: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("pencil nearest"),
        size: wgpu::Extent3d {
            width: nearest::TEXTURE_WIDTH as u32,
            height: rows.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba32Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
        texture_binding_view_dimension: None,
    })
}

/// A compiled figure. The shader encodes sRGB itself, so `format` must be a
/// non-sRGB format. That keeps output identical across backends.
pub struct FigurePipeline {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    uniforms: wgpu::Buffer,
    geometry: wgpu::Buffer,
    /// Each collection's sides, per instance. Always there, since the layout says so.
    sides: wgpu::Texture,
    sides_view: wgpu::TextureView,
    /// Traced curves, tiled, for distances to them. It grows as a frame needs more rows.
    nearest: wgpu::Texture,
    nearest_view: wgpu::TextureView,
    /// Each line's own color, for strokes whose lines each have one. It grows as needed.
    inks: wgpu::Texture,
    inks_view: wgpu::TextureView,
    coverage_pipeline: wgpu::RenderPipeline,
    /// Only when a stroke has lines of their own color.
    inked_pipeline: Option<wgpu::RenderPipeline>,
    coverage_bind_group: wgpu::BindGroup,
    coverage: Option<Coverage>,
    layers: u32,
    inked_layers: u32,
    /// Per line stroke, in mark order, whether its lines each have their own color.
    inked: Vec<bool>,
}

impl FigurePipeline {
    /// Makes a figure's pipeline on `device`, for a target of `format`. Its three render pipelines
    /// are made with `create_render_pipeline_async`, all at once: on WebGPU the browser compiles
    /// their shaders off the thread that composites the page, where `create_render_pipeline`
    /// stopped every frame on the page until they were done. Elsewhere the future is ready when
    /// this returns. A pipeline that can't be made is the future's error, and so is a validation
    /// error in what's made for them, such as the shader module: an error scope catches those. One
    /// that runs out of memory, or an internal error, making a buffer or texture still goes to the
    /// device's uncaptured-error handler, as before.
    pub fn build(
        device: &wgpu::Device,
        figure: &Figure,
        format: wgpu::TextureFormat,
    ) -> impl Future<Output = Result<Self, wgpu::Error>> + use<> {
        // The pipelines say their own errors. What's made for them says its validation errors in
        // this scope.
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("pencil figure"),
            source: wgpu::ShaderSource::Wgsl(wgsl::emit(figure).into()),
        });
        // Buffers start as zeros, and none is mapped when it's made: a browser that has lost its
        // GPU throws on a mapped one, which wgpu can't return, and a page would trap.
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("pencil uniforms"),
            size: UNIFORM_BYTES as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let geometry = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("pencil geometry"),
            size: GEOMETRY_BYTES as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let uniform_entry = |visibility| wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                uniform_entry(wgpu::ShaderStages::FRAGMENT),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    ..uniform_entry(wgpu::ShaderStages::FRAGMENT)
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 6,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline_async(&wgpu::RenderPipelineDescriptor {
            label: Some("pencil figure"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(format.into())],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });

        let coverage_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("pencil coverage"),
            source: wgpu::ShaderSource::Wgsl(wgsl::COVERAGE_SHADER.into()),
        });
        let coverage_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[uniform_entry(wgpu::ShaderStages::VERTEX)],
        });
        let coverage_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &coverage_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms.as_entire_binding(),
            }],
        });
        let coverage_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[Some(&coverage_layout)],
                immediate_size: 0,
            });
        let max = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Max,
        };
        let coverage_pipeline =
            device.create_render_pipeline_async(&wgpu::RenderPipelineDescriptor {
                label: Some("pencil coverage"),
                layout: Some(&coverage_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &coverage_module,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: (stroke::FLOATS_PER_SEGMENT * 4) as u64,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &wgpu::vertex_attr_array![
                            0 => Float32x4,
                            1 => Float32x4,
                            2 => Float32x4,
                        ],
                    })],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &coverage_module,
                    entry_point: Some("fs"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: COVERAGE_FORMAT,
                        blend: Some(wgpu::BlendState {
                            color: max,
                            alpha: max,
                        }),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleStrip,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            });

        // The segments and, in a buffer of their own, each one's row of colors.
        let segment_buffer = wgpu::VertexBufferLayout {
            array_stride: (stroke::FLOATS_PER_SEGMENT * 4) as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &wgpu::vertex_attr_array![
                0 => Float32x4,
                1 => Float32x4,
                2 => Float32x4,
            ],
        };
        let inked: Vec<bool> = figure
            .scene
            .marks
            .iter()
            .zip(&figure.styles.marks)
            .filter(|(m, s)| s.stroke_is_line(&m.form))
            .map(|(m, s)| style::inked(m, s))
            .collect();
        let inked_layers = inked.iter().filter(|i| **i).count() as u32;
        let layers = inked.len() as u32 - inked_layers;
        let inked_pipeline = (inked_layers > 0).then(|| {
            device.create_render_pipeline_async(&wgpu::RenderPipelineDescriptor {
                label: Some("pencil inked coverage"),
                layout: Some(&coverage_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &coverage_module,
                    entry_point: Some("vs_inked"),
                    compilation_options: Default::default(),
                    buffers: &[
                        Some(segment_buffer),
                        Some(wgpu::VertexBufferLayout {
                            array_stride: 4,
                            step_mode: wgpu::VertexStepMode::Instance,
                            attributes: &wgpu::vertex_attr_array![3 => Float32],
                        }),
                    ],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &coverage_module,
                    entry_point: Some("fs_inked"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: INKED_FORMAT,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleStrip,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::Less),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        });
        let sides = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("pencil sides"),
            size: wgpu::Extent3d {
                width: SIDE_TEXTURE_WIDTH as u32,
                height: side_rows(&figure.sided()) as u32,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
            texture_binding_view_dimension: None,
        });
        let sides_view = sides.create_view(&Default::default());
        let nearest = nearest_texture(device, 1);
        let nearest_view = nearest.create_view(&Default::default());
        let inks = ink_texture(device, u32::from(inked_layers > 0));
        let inks_view = inks.create_view(&Default::default());
        let made = scope.pop();
        async move {
            if let Some(error) = made.await {
                return Err(error);
            }
            let pipeline = pipeline.await?;
            let coverage_pipeline = coverage_pipeline.await?;
            let inked_pipeline = match inked_pipeline {
                Some(inked) => Some(inked.await?),
                None => None,
            };
            Ok(Self {
                pipeline,
                layout,
                uniforms,
                geometry,
                sides,
                sides_view,
                nearest,
                nearest_view,
                inks,
                inks_view,
                coverage_pipeline,
                inked_pipeline,
                coverage_bind_group,
                coverage: None,
                layers: layers.max(2),
                inked_layers,
                inked,
            })
        }
    }

    fn coverage_for(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        let current = self.coverage.as_ref();
        if current.is_some_and(|c| c.width == width && c.height == height) {
            return;
        }
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("pencil coverage"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: self.layers,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: COVERAGE_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
            texture_binding_view_dimension: None,
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        // A figure with no lines of their own color pays for none: a texel stands in for the layout.
        let any = self.inked_layers > 0;
        let inked = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("pencil inked coverage"),
            size: wgpu::Extent3d {
                width: if any { width } else { 1 },
                height: if any { height } else { 1 },
                depth_or_array_layers: self.inked_layers.max(2),
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: INKED_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
            texture_binding_view_dimension: None,
        });
        let inked_view = inked.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        let depth = any.then(|| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some("pencil coverage depth"),
                    size: wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: DEPTH_FORMAT,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                    texture_binding_view_dimension: None,
                })
                .create_view(&Default::default())
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.uniforms.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.geometry.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&self.sides_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&self.nearest_view),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(&inked_view),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(&self.inks_view),
                },
            ],
        });
        self.coverage = Some(Coverage {
            texture,
            inked,
            depth,
            bind_group,
            width,
            height,
        });
    }

    /// Draws a frame from its CPU side, which comes from `frame::prepare`. It goes in its
    /// panel's part of `target`. `clear` starts the canvas, for the first panel of a frame.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        inputs: FrameInputs,
        prepared: &Prepared,
        clear: Option<[f64; 3]>,
    ) {
        queue.write_buffer(&self.uniforms, 0, &inputs.bytes(prepared));
        let geometry: Vec<u8> = prepared
            .geometry
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();
        queue.write_buffer(&self.geometry, 0, &geometry);
        if !prepared.sides.is_empty() {
            let bytes: Vec<u8> = prepared
                .sides
                .iter()
                .flat_map(|f| f.to_le_bytes())
                .collect();
            queue.write_texture(
                self.sides.as_image_copy(),
                &bytes,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(SIDE_TEXTURE_WIDTH as u32 * 16),
                    rows_per_image: None,
                },
                self.sides.size(),
            );
        }
        let rows = (prepared.nearest.len() / (nearest::TEXTURE_WIDTH * 4)) as u32;
        if rows > self.nearest.height() {
            self.nearest = nearest_texture(device, rows);
            self.nearest_view = self.nearest.create_view(&Default::default());
            // The bind group names the texture, so it's made again.
            self.coverage = None;
        }
        if rows > 0 {
            let bytes: Vec<u8> = prepared
                .nearest
                .iter()
                .flat_map(|f| f.to_le_bytes())
                .collect();
            queue.write_texture(
                self.nearest.as_image_copy(),
                &bytes,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(nearest::TEXTURE_WIDTH as u32 * 16),
                    rows_per_image: None,
                },
                wgpu::Extent3d {
                    width: nearest::TEXTURE_WIDTH as u32,
                    height: rows,
                    depth_or_array_layers: 1,
                },
            );
        }
        let rows = (prepared.colors.len() / 4).div_ceil(INK_TEXTURE_WIDTH) as u32;
        if rows > self.inks.height() {
            self.inks = ink_texture(device, rows);
            self.inks_view = self.inks.create_view(&Default::default());
            self.coverage = None;
        }
        if rows > 0 {
            let mut colors = prepared.colors.clone();
            colors.resize(rows as usize * INK_TEXTURE_WIDTH * 4, 0.0);
            let bytes: Vec<u8> = colors.iter().flat_map(|f| f.to_le_bytes()).collect();
            queue.write_texture(
                self.inks.as_image_copy(),
                &bytes,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(INK_TEXTURE_WIDTH as u32 * 16),
                    rows_per_image: None,
                },
                wgpu::Extent3d {
                    width: INK_TEXTURE_WIDTH as u32,
                    height: rows,
                    depth_or_array_layers: 1,
                },
            );
        }
        self.coverage_for(device, inputs.width, inputs.height);
        let coverage = self.coverage.as_ref().expect("just created");

        let (mut plain, mut inked) = (0, 0);
        for (layer, segments) in prepared.strokes.iter().enumerate() {
            let is_inked = self.inked.get(layer).copied().unwrap_or(false);
            let (texture, index) = if is_inked {
                inked += 1;
                (&coverage.inked, inked - 1)
            } else {
                plain += 1;
                (&coverage.texture, plain - 1)
            };
            let view = texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2),
                base_array_layer: index as u32,
                array_layer_count: Some(1),
                ..Default::default()
            });
            let bytes: Vec<u8> = segments.iter().flat_map(|f| f.to_le_bytes()).collect();
            // Written through the queue, not mapped when it's made (see `new`).
            let instances = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("pencil segments"),
                size: bytes.len() as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            if !bytes.is_empty() {
                queue.write_buffer(&instances, 0, &bytes);
            }
            let count = segments.len() / stroke::FLOATS_PER_SEGMENT;
            let rows = is_inked.then(|| {
                let mut rows = prepared.inks.get(layer).cloned().unwrap_or_default();
                rows.resize(count, 0.0);
                let bytes: Vec<u8> = rows.iter().flat_map(|f| f.to_le_bytes()).collect();
                let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("pencil segment inks"),
                    size: bytes.len() as u64,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                if !bytes.is_empty() {
                    queue.write_buffer(&buffer, 0, &bytes);
                }
                buffer
            });
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("pencil coverage"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: coverage.depth.as_ref().filter(|_| is_inked).map(
                    |depth| wgpu::RenderPassDepthStencilAttachment {
                        view: depth,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: wgpu::StoreOp::Discard,
                        }),
                        stencil_ops: None,
                    },
                ),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if count > 0 {
                match &rows {
                    Some(rows) => {
                        let Some(pipeline) = &self.inked_pipeline else {
                            continue;
                        };
                        pass.set_pipeline(pipeline);
                        pass.set_vertex_buffer(1, rows.slice(..));
                    }
                    None => pass.set_pipeline(&self.coverage_pipeline),
                }
                pass.set_bind_group(0, &self.coverage_bind_group, &[]);
                pass.set_vertex_buffer(0, instances.slice(..));
                pass.draw(0..4, 0..count as u32);
            }
        }

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("pencil figure"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: match clear {
                        // The target isn't sRGB, and the shader encodes, so this has to as well.
                        Some(c) => wgpu::LoadOp::Clear(wgpu::Color {
                            r: srgb_encode(c[0]),
                            g: srgb_encode(c[1]),
                            b: srgb_encode(c[2]),
                            a: 1.0,
                        }),
                        None => wgpu::LoadOp::Load,
                    },
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        let [x, y] = inputs.origin;
        pass.set_viewport(
            x as f32,
            y as f32,
            inputs.width as f32,
            inputs.height as f32,
            0.0,
            1.0,
        );
        pass.set_scissor_rect(x, y, inputs.width, inputs.height);
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &coverage.bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::{Offscreen, RenderError};

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::{FigurePipeline, FrameInputs};
    use pencil_core::Figure;
    use pencil_core::frame::Prepared;

    const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

    #[derive(Debug)]
    pub struct RenderError(pub String);

    impl std::fmt::Display for RenderError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str(&self.0)
        }
    }

    impl std::error::Error for RenderError {}

    /// A headless GPU, for PNG frames and eventually video.
    pub struct Offscreen {
        device: wgpu::Device,
        queue: wgpu::Queue,
        pub adapter_name: String,
    }

    impl Offscreen {
        pub fn new() -> Result<Self, RenderError> {
            let instance = wgpu::Instance::default();
            let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
                .map_err(|e| RenderError(format!("no GPU adapter: {e}")))?;
            let adapter_name = adapter.get_info().name;
            let (device, queue) = pollster::block_on(adapter.request_device(&Default::default()))
                .map_err(|e| RenderError(format!("no GPU device: {e}")))?;
            Ok(Self {
                device,
                queue,
                adapter_name,
            })
        }

        /// Compiles a figure's shaders, which every frame of it can then share. Natively the
        /// pipelines are made before `build` returns, so this waits on nothing.
        pub fn pipeline(&self, figure: &Figure) -> Result<FigurePipeline, RenderError> {
            pollster::block_on(FigurePipeline::build(&self.device, figure, FORMAT))
                .map_err(|e| RenderError(format!("invalid figure pipeline: {e}")))
        }

        /// Renders one panel on its own to tightly packed RGBA8.
        pub fn render(
            &self,
            pipeline: &mut FigurePipeline,
            prepared: &Prepared,
            time: f32,
            width: u32,
            height: u32,
        ) -> Result<Vec<u8>, RenderError> {
            self.render_panels(
                std::slice::from_mut(pipeline),
                &[(prepared, [0, 0], width, height)],
                time,
                width,
                height,
            )
        }

        /// Renders panels to one canvas, as tightly packed RGBA8. Each is `prepared`, its
        /// corner, and its size.
        pub fn render_panels(
            &self,
            pipelines: &mut [FigurePipeline],
            panels: &[(&Prepared, [u32; 2], u32, u32)],
            time: f32,
            width: u32,
            height: u32,
        ) -> Result<Vec<u8>, RenderError> {
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("pencil frame"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
                texture_binding_view_dimension: None,
            });
            let row = width * 4;
            let aligned = row.next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
            let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("pencil readback"),
                size: (aligned * height) as u64,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });

            let mut encoder = self.device.create_command_encoder(&Default::default());
            let view = texture.create_view(&Default::default());
            for (i, (pipeline, (prepared, origin, w, h))) in
                pipelines.iter_mut().zip(panels).enumerate()
            {
                let inputs = FrameInputs {
                    time,
                    width: *w,
                    height: *h,
                    dpr: prepared.dpr as f32,
                    em: prepared.em as f32,
                    origin: *origin,
                };
                pipeline.draw(
                    &self.device,
                    &self.queue,
                    &mut encoder,
                    &view,
                    inputs,
                    prepared,
                    (i == 0).then_some(prepared.paper),
                );
            }
            encoder.copy_texture_to_buffer(
                texture.as_image_copy(),
                wgpu::TexelCopyBufferInfo {
                    buffer: &readback,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(aligned),
                        rows_per_image: None,
                    },
                },
                texture.size(),
            );
            self.queue.submit([encoder.finish()]);

            let slice = readback.slice(..);
            slice.map_async(wgpu::MapMode::Read, |_| {});
            self.device
                .poll(wgpu::PollType::wait_indefinitely())
                .map_err(|e| RenderError(format!("GPU wait failed: {e}")))?;
            let mapped = slice
                .get_mapped_range()
                .map_err(|e| RenderError(format!("can't read the frame back: {e}")))?;
            let mut out = Vec::with_capacity((row * height) as usize);
            for line in mapped.chunks(aligned as usize) {
                out.extend_from_slice(&line[..row as usize]);
            }
            Ok(out)
        }
    }
}
