//! Draws a figure's emitted WGSL with wgpu. The pipeline is shared by the native
//! offscreen path and the browser, so both run the same shader.

use pencil_core::frame::{GEOMETRY_VEC4S, Prepared, VALUE_VEC4S};
use pencil_core::{Figure, stroke, wgsl};
use wgpu::util::DeviceExt;

/// A frame's inputs, matching `Uniforms` in `pencil_core::wgsl`.
#[derive(Clone, Copy, Debug)]
pub struct FrameInputs {
    pub time: f32,
    pub width: u32,
    pub height: u32,
    /// Device pixels per style pixel, so `width: 3px` looks the same on any screen.
    pub dpr: f32,
}

impl FrameInputs {
    fn bytes(self, values: &[f32]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(UNIFORM_BYTES);
        bytes.extend_from_slice(&self.time.to_le_bytes());
        bytes.extend_from_slice(&(self.width as f32).to_le_bytes());
        bytes.extend_from_slice(&(self.height as f32).to_le_bytes());
        bytes.extend_from_slice(&self.dpr.to_le_bytes());
        for v in values {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        bytes.resize(UNIFORM_BYTES, 0);
        bytes
    }
}

const UNIFORM_BYTES: usize = 16 + VALUE_VEC4S * 16;
const GEOMETRY_BYTES: usize = GEOMETRY_VEC4S * 16;

const COVERAGE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R8Unorm;

/// One coverage layer per traced curve. It's always an array of at least two layers,
/// because wgpu's GL backend can't treat a one-layer texture as an array.
struct Coverage {
    texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
    width: u32,
    height: u32,
}

/// A compiled figure. The shader encodes sRGB itself, so `format` must be a
/// non-sRGB format. That keeps output identical across backends.
pub struct FigurePipeline {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    uniforms: wgpu::Buffer,
    geometry: wgpu::Buffer,
    coverage_pipeline: wgpu::RenderPipeline,
    coverage_bind_group: wgpu::BindGroup,
    coverage: Option<Coverage>,
    layers: u32,
}

impl FigurePipeline {
    pub fn new(device: &wgpu::Device, figure: &Figure, format: wgpu::TextureFormat) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("pencil figure"),
            source: wgpu::ShaderSource::Wgsl(wgsl::emit(figure).into()),
        });
        let uniforms = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("pencil uniforms"),
            contents: &[0; UNIFORM_BYTES],
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let geometry = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("pencil geometry"),
            contents: &[0; GEOMETRY_BYTES],
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
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
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
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
        let coverage_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
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

        let layers = figure
            .scene
            .marks
            .iter()
            .zip(&figure.styles.marks)
            .filter(|(m, s)| s.stroke_is_line(&m.form))
            .count() as u32;
        Self {
            pipeline,
            layout,
            uniforms,
            geometry,
            coverage_pipeline,
            coverage_bind_group,
            coverage: None,
            layers: layers.max(2),
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
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
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
            ],
        });
        self.coverage = Some(Coverage {
            texture,
            bind_group,
            width,
            height,
        });
    }

    /// Draws a frame from its CPU side, which comes from `frame::prepare`.
    pub fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        inputs: FrameInputs,
        prepared: &Prepared,
    ) {
        queue.write_buffer(&self.uniforms, 0, &inputs.bytes(&prepared.values));
        let geometry: Vec<u8> = prepared
            .geometry
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();
        queue.write_buffer(&self.geometry, 0, &geometry);
        self.coverage_for(device, inputs.width, inputs.height);
        let coverage = self.coverage.as_ref().expect("just created");

        for (layer, segments) in prepared.strokes.iter().enumerate() {
            let view = coverage.texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2),
                base_array_layer: layer as u32,
                array_layer_count: Some(1),
                ..Default::default()
            });
            let bytes: Vec<u8> = segments.iter().flat_map(|f| f.to_le_bytes()).collect();
            let instances = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("pencil segments"),
                contents: &bytes,
                usage: wgpu::BufferUsages::VERTEX,
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
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if !segments.is_empty() {
                pass.set_pipeline(&self.coverage_pipeline);
                pass.set_bind_group(0, &self.coverage_bind_group, &[]);
                pass.set_vertex_buffer(0, instances.slice(..));
                pass.draw(
                    0..4,
                    0..(segments.len() / stroke::FLOATS_PER_SEGMENT) as u32,
                );
            }
        }

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("pencil figure"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
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

        /// Compiles a figure's shaders, which every frame of it can then share.
        pub fn pipeline(&self, figure: &Figure) -> Result<FigurePipeline, RenderError> {
            let scope = self.device.push_error_scope(wgpu::ErrorFilter::Validation);
            let pipeline = FigurePipeline::new(&self.device, figure, FORMAT);
            match pollster::block_on(scope.pop()) {
                Some(e) => Err(RenderError(format!("invalid figure pipeline: {e}"))),
                None => Ok(pipeline),
            }
        }

        /// Renders one frame to tightly packed RGBA8.
        pub fn render(
            &self,
            pipeline: &mut FigurePipeline,
            prepared: &Prepared,
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
            let inputs = FrameInputs {
                time,
                width,
                height,
                dpr: prepared.dpr as f32,
            };
            pipeline.draw(
                &self.device,
                &self.queue,
                &mut encoder,
                &view,
                inputs,
                prepared,
            );
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
