use crate::{
    buffer::BufferWrapper,
    flow::{FlowModel, FlowModelInit, WindowSize},
    grid::{Grid, Positioned},
    util::least_power_of_2_greater,
};
use bytemuck::{Pod, Zeroable};
use rand::{thread_rng, Rng};
use std::{borrow::Cow, f32::consts::PI, mem::size_of, sync::Arc, time::Duration};
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingResource, BindingType, BlendState, BufferAddress,
    BufferBindingType, BufferUsages, Color, ColorTargetState, ColorWrites, CommandBuffer,
    CommandEncoderDescriptor, Device, FragmentState, FrontFace, LoadOp, MultisampleState,
    Operations, PipelineLayoutDescriptor, PolygonMode, PrimitiveState, PrimitiveTopology, Queue,
    RenderPassColorAttachment, RenderPassDescriptor, RenderPipeline, RenderPipelineDescriptor,
    ShaderModuleDescriptor, ShaderSource, ShaderStages, TextureView, VertexAttribute,
    VertexBufferLayout, VertexState, VertexStepMode,
};

#[cfg(feature = "timer")]
use crate::timer::Timer;

const LINE_LENGTH: f32 = 200f32;
const POINT_COUNT: usize = 200;
const BACKGROUND_COLOR: Color = Color { r: 0.0, g: 0.005, b: 0.01, a: 1.0 };
const LINE_COLOR: Color = Color { r: 0.0, g: 0.4, b: 0.6, a: 1.0 };

const SHADER_SRC: &str = include_str!("shader.wgsl");

pub struct NeonetApp {
    size: WindowSize,
    points: Grid<Point>,
    device: Arc<Device>,
    queue: Arc<Queue>,
    queued_commands: Vec<CommandBuffer>,
    uniform_buffer: BufferWrapper<UniformData>,
    vertex_buffer_tmp: Vec<GPUPoint>,
    vertex_buffer: BufferWrapper<GPUPoint>,
    index_buffer_tmp: Vec<PointIndex>,
    index_buffer: Option<BufferWrapper<PointIndex>>,
    uniforms_bind_group: BindGroup,
    pipeline: RenderPipeline,
}

#[derive(Debug, Copy, Clone)]
struct Point {
    index: usize,
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
}

impl Positioned for Point {
    fn x(&self) -> f32 {
        self.x
    }

    fn y(&self) -> f32 {
        self.y
    }

    fn x_mut(&mut self) -> &mut f32 {
        &mut self.x
    }

    fn y_mut(&mut self) -> &mut f32 {
        &mut self.y
    }
}

#[repr(C, align(16))]
#[derive(Debug, Copy, Clone)]
struct GPUPosition([f32; 2]);

#[repr(C, align(16))]
#[derive(Debug, Copy, Clone)]
struct GPUColor([f32; 3]);

#[repr(C)]
#[derive(Debug, Copy, Clone)]
struct GPUPoint {
    position: GPUPosition,
    color: GPUColor,
}

unsafe impl Zeroable for GPUPoint {}
unsafe impl Pod for GPUPoint {}

impl GPUPoint {
    fn from(point: Point) -> GPUPoint {
        GPUPoint {
            position: GPUPosition([point.x, point.y]),
            color: GPUColor([
                LINE_COLOR.r as f32,
                LINE_COLOR.g as f32,
                LINE_COLOR.b as f32,
            ]),
        }
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
struct UniformData {
    screen_width: f32,
    screen_height: f32,
}

unsafe impl Zeroable for UniformData {}
unsafe impl Pod for UniformData {}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
struct PointIndex {
    me: u32,
    other: u32,
    distance_sqr: f32,
}

unsafe impl Zeroable for PointIndex {}
unsafe impl Pod for PointIndex {}

impl PointIndex {
    const ATTRIBS: [VertexAttribute; 3] =
        wgpu::vertex_attr_array![0 => Uint32, 1 => Uint32, 2 => Float32];

    fn desc<'a>() -> VertexBufferLayout<'a> {
        VertexBufferLayout {
            array_stride: size_of::<PointIndex>() as BufferAddress,
            step_mode: VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

#[async_trait]
impl FlowModel for NeonetApp {
    async fn init(init: FlowModelInit) -> NeonetApp {
        let size = init.window_size;
        let width = size.width;
        let height = size.height;
        let queue = init.queue;
        let device = init.device;
        let frame_format = init.frame_format;

        let mut vertex_buffer_tmp = vec![];
        vertex_buffer_tmp.reserve(POINT_COUNT);

        let mut points = Grid::new(
            LINE_LENGTH,
            LINE_LENGTH,
            width + LINE_LENGTH * 2.0,
            height + LINE_LENGTH * 2.0,
        );
        let mut rng = thread_rng();
        for i in 0..POINT_COUNT {
            let angle = rng.gen_range(0.0..(PI * 2.0));
            let speed = rng.gen_range(20.0..100.0f32);
            let point = Point {
                index: i,
                x: rng.gen_range(-LINE_LENGTH..width + LINE_LENGTH),
                y: rng.gen_range(-LINE_LENGTH..height + LINE_LENGTH),
                vx: angle.cos() * speed,
                vy: angle.sin() * speed,
            };
            points.insert(point);
            vertex_buffer_tmp.push(GPUPoint::from(point));
        }

        let mut cbs = vec![];

        let (uniform_buffer, cb) = BufferWrapper::from_data(
            &device,
            &[UniformData {
                screen_width: width,
                screen_height: height,
            }],
            BufferUsages::UNIFORM,
        );
        cbs.push(cb);

        // The actual vertex buffer will be a uniform.
        let (vertex_buffer, cb) =
            BufferWrapper::from_data(&device, &vertex_buffer_tmp, BufferUsages::UNIFORM);
        cbs.push(cb);

        // Then we can specify our own per-index data as a vertex buffer.
        let mut index_buffer_tmp = Vec::new();
        index_buffer_tmp.reserve(POINT_COUNT * 2);

        queue.submit(cbs);

        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("Shader Module"),
            source: ShaderSource::Wgsl(Cow::Borrowed(SHADER_SRC)),
        });

        let uniforms_bind_group_layout =
            device.create_bind_group_layout(&BindGroupLayoutDescriptor {
                label: Some("Uniforms Bind Group Layout"),
                entries: &[
                    BindGroupLayoutEntry {
                        binding: 0,
                        visibility: ShaderStages::VERTEX,
                        ty: BindingType::Buffer {
                            ty: BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    BindGroupLayoutEntry {
                        binding: 1,
                        visibility: ShaderStages::VERTEX,
                        ty: BindingType::Buffer {
                            ty: BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });

        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("Render Pipeline Layout"),
            bind_group_layouts: &[&uniforms_bind_group_layout],
            push_constant_ranges: &[],
        });

        let uniforms_bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("Uniforms Bind Group"),
            layout: &uniforms_bind_group_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::Buffer(
                        uniform_buffer.buffer().as_entire_buffer_binding(),
                    ),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::Buffer(
                        vertex_buffer.buffer().as_entire_buffer_binding(),
                    ),
                },
            ],
        });

        let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &shader,
                entry_point: "vert_main",
                buffers: &[PointIndex::desc()],
            },
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: "frag_main",
                targets: &[Some(ColorTargetState {
                    format: frame_format,
                    blend: Some(BlendState::ALPHA_BLENDING),
                    write_mask: ColorWrites::ALL,
                })],
            }),
            primitive: PrimitiveState {
                topology: PrimitiveTopology::LineList,
                strip_index_format: None,
                front_face: FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
        });

        NeonetApp {
            size,
            points,
            device,
            queue,
            queued_commands: vec![],
            uniform_buffer,
            vertex_buffer_tmp,
            vertex_buffer,
            index_buffer_tmp,
            index_buffer: None,
            uniforms_bind_group,
            pipeline,
        }
    }

    async fn resize(&mut self, size: WindowSize) {
        self.size = size;
        self.points.set_size(
            size.width + LINE_LENGTH * 2.0,
            size.height + LINE_LENGTH * 2.0,
        );

        self.queued_commands.push(
            self.uniform_buffer
                .replace_all(
                    &self.device,
                    &[UniformData {
                        screen_width: size.width,
                        screen_height: size.height,
                    }],
                )
                .await
                .unwrap(),
        );
    }

    async fn update(&mut self, delta: Duration) {
        #[cfg(feature = "timer")]
        let _timer = Timer::from_str("Model::update");

        // Move the points

        self.points.all_mut(|point| {
            point.x += point.vx * delta.as_secs_f32();
            point.y += point.vy * delta.as_secs_f32();

            if point.x < -LINE_LENGTH {
                point.x += self.size.width + LINE_LENGTH * 2.0;
            } else if point.x > self.size.width + LINE_LENGTH {
                point.x -= self.size.width + LINE_LENGTH * 2.0;
            }

            if point.y < -LINE_LENGTH {
                point.y += self.size.height + LINE_LENGTH * 2.0;
            } else if point.y > self.size.height + LINE_LENGTH {
                point.y -= self.size.height + LINE_LENGTH * 2.0;
            }

            self.vertex_buffer_tmp[point.index] = GPUPoint::from(*point);
        });

        self.queued_commands.push(
            self.vertex_buffer
                .replace_all(&self.device, &self.vertex_buffer_tmp)
                .await
                .unwrap(),
        );

        // Draw the lines

        self.index_buffer_tmp.clear();
        self.points.pairs(|point, other, distance_sqr| {
            // #[cfg(debug_assertions)]
            // let _timer1 = Timer::new(format!("Model::render point={:?} other={:?}",
            // point, other)); let alpha = ((1.0 - distance_sqr.sqrt() /
            // LINE_LENGTH) * 255.0) as u8;

            self.index_buffer_tmp.push(PointIndex {
                me: point.index as u32,
                other: other.index as u32,
                distance_sqr,
            });
            self.index_buffer_tmp.push(PointIndex {
                me: other.index as u32,
                other: point.index as u32,
                distance_sqr,
            });
        });

        // Make sure the buffer is large enough
        if self.index_buffer.is_none()
            || self.index_buffer.as_ref().unwrap().capacity()
                < self.index_buffer_tmp.len() as BufferAddress
        {
            self.index_buffer = Some(BufferWrapper::new(
                &self.device,
                least_power_of_2_greater(self.index_buffer_tmp.len() as u64),
                BufferUsages::VERTEX,
            ));
        }

        {
            let buffer = self.index_buffer.as_mut().unwrap();
            self.queued_commands.push(
                buffer
                    .replace_all(&self.device, &self.index_buffer_tmp)
                    .await
                    .unwrap(),
            );
        }
    }

    fn render(&mut self, view: &TextureView, _delta: Duration) {
        #[cfg(feature = "timer")]
        let _timer = Timer::from_str("Model::render");

        let mut encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("Render Command Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(BACKGROUND_COLOR),
                        store: true,
                    },
                })],
                depth_stencil_attachment: None,
            });

            let index_buffer = self.index_buffer.as_ref().unwrap();
            render_pass.set_pipeline(&self.pipeline);
            render_pass.set_vertex_buffer(0, index_buffer.buffer().slice(..));
            render_pass.set_bind_group(0, &self.uniforms_bind_group, &[]);
            render_pass.draw(0..index_buffer.len() as u32, 0..1);
        }

        self.queued_commands.push(encoder.finish());

        self.queue.submit(self.queued_commands.drain(..));
    }

    fn shutdown(&mut self) {}
}
