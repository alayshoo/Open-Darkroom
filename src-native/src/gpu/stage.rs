// src-native/src/gpu/stage.rs
//
// Every stage of the render chain is the same shape: draw a full-screen quad,
// read some bound resources, write one target. Mirrors renderStagePipeline.ts.

use super::Gpu;

pub struct RenderStage {
    pub label: &'static str,
    pub pipeline: wgpu::RenderPipeline,
    pub layout: wgpu::BindGroupLayout,
}

pub struct StageDesc<'a> {
    pub label: &'static str,
    pub code: &'a str,
    pub format: wgpu::TextureFormat,
    pub bindings: &'a [wgpu::BindGroupLayoutEntry],
    pub entry_point: &'a str,
    pub blend: Option<wgpu::BlendState>,
    pub constants: &'a [(&'a str, f64)],
}

impl<'a> StageDesc<'a> {
    pub fn new(label: &'static str, code: &'a str, format: wgpu::TextureFormat, bindings: &'a [wgpu::BindGroupLayoutEntry]) -> Self {
        Self { label, code, format, bindings, entry_point: "fs_main", blend: None, constants: &[] }
    }
}

pub const PREMULTIPLIED_OVER: wgpu::BlendState = wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING;

pub fn shader(gpu: &Gpu, label: &str, code: &str) -> wgpu::ShaderModule {
    gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(code.into()),
    })
}

pub fn create(gpu: &Gpu, d: StageDesc<'_>) -> RenderStage {
    let module = shader(gpu, d.label, d.code);
    let layout = gpu.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some(d.label),
        entries: d.bindings,
    });
    let pipeline = render_pipeline(gpu, d.label, &module, &layout, d.entry_point, d.format, d.blend, d.constants);
    RenderStage { label: d.label, pipeline, layout }
}

#[allow(clippy::too_many_arguments)]
pub fn render_pipeline(
    gpu: &Gpu,
    label: &str,
    module: &wgpu::ShaderModule,
    layout: &wgpu::BindGroupLayout,
    entry_point: &str,
    format: wgpu::TextureFormat,
    blend: Option<wgpu::BlendState>,
    constants: &[(&str, f64)],
) -> wgpu::RenderPipeline {
    let pipeline_layout = gpu.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(label),
        bind_group_layouts: &[layout],
        push_constant_ranges: &[],
    });
    gpu.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module,
            entry_point: Some("vs_main"),
            buffers: &[],
            compilation_options: wgpu::PipelineCompilationOptions { constants, ..Default::default() },
        },
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some(entry_point),
            targets: &[Some(wgpu::ColorTargetState { format, blend, write_mask: wgpu::ColorWrites::ALL })],
            compilation_options: wgpu::PipelineCompilationOptions { constants, ..Default::default() },
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    })
}

pub fn compute_pipeline(gpu: &Gpu, label: &str, code: &str, layout: &wgpu::BindGroupLayout) -> wgpu::ComputePipeline {
    let module = shader(gpu, label, code);
    let pipeline_layout = gpu.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(label),
        bind_group_layouts: &[layout],
        push_constant_ranges: &[],
    });
    gpu.device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some(label),
        layout: Some(&pipeline_layout),
        module: &module,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    })
}

pub fn layout(gpu: &Gpu, label: &str, entries: &[wgpu::BindGroupLayoutEntry]) -> wgpu::BindGroupLayout {
    gpu.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some(label), entries })
}

pub fn bind_group(gpu: &Gpu, label: &str, layout: &wgpu::BindGroupLayout, resources: &[wgpu::BindingResource<'_>]) -> wgpu::BindGroup {
    gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(label),
        layout,
        entries: &super::bind::entries(resources),
    })
}

#[derive(Default)]
pub struct PassOptions<'a> {
    pub clear: Option<wgpu::Color>,
    /// Only this part of the target is drawn, as [x, y, width, height].
    pub scissor: Option<[u32; 4]>,
    /// Keep what the target held rather than clearing it.
    pub load: bool,
    pub timestamps: Option<wgpu::RenderPassTimestampWrites<'a>>,
}

/// Draw a full-screen quad from `stage` into `target`.
pub fn full_screen_pass(
    encoder: &mut wgpu::CommandEncoder,
    stage: &RenderStage,
    target: &wgpu::TextureView,
    bind_group: &wgpu::BindGroup,
    options: PassOptions<'_>,
) {
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some(stage.label),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: target,
            resolve_target: None,
            ops: wgpu::Operations {
                load: if options.load {
                    wgpu::LoadOp::Load
                } else {
                    wgpu::LoadOp::Clear(options.clear.unwrap_or(wgpu::Color::BLACK))
                },
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: options.timestamps,
        occlusion_query_set: None,
    });
    if let Some([x, y, w, h]) = options.scissor {
        if w == 0 || h == 0 {
            return;
        }
        pass.set_scissor_rect(x, y, w, h);
    }
    pass.set_pipeline(&stage.pipeline);
    pass.set_bind_group(0, bind_group, &[]);
    pass.draw(0..6, 0..1);
}
