use wgpu::util::DeviceExt;
use wgpu_test::{
    apply, gpu_test, image::ReadbackBuffers, GpuTestConfiguration, GpuTestInitializer,
    TestParameters, TestingContext,
};

pub fn all_tests(tests: &mut Vec<GpuTestInitializer>) {
    tests.push(PIPELINE_LAYOUT_GAP_REBINDS_HIGHER_GROUP);
}

#[apply(gpu_test!)]
static PIPELINE_LAYOUT_GAP_REBINDS_HIGHER_GROUP: GpuTestConfiguration = GpuTestConfiguration::new()
    .parameters(TestParameters::default().test_features_limits())
    .run_async(pipeline_layout_gap_rebinds_higher_group);

async fn pipeline_layout_gap_rebinds_higher_group(ctx: TestingContext) {
    let device = &ctx.device;
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(
            "
            @group(0) @binding(0) var<uniform> a: vec4f;
            @group(1) @binding(0) var<uniform> b: vec4f;

            @vertex fn vertex(@builtin(vertex_index) index: u32)
                -> @builtin(position) vec4f {
                let positions = array<vec2f, 3>(
                    vec2f(-1.0, -1.0), vec2f(3.0, -1.0), vec2f(-1.0, 3.0));
                return vec4f(positions[index], 0.0, 1.0);
            }

            @fragment fn both_groups() -> @location(0) vec4f {
                return vec4f(a.x, b.x, 0.0, 1.0);
            }

            @fragment fn higher_group_only() -> @location(0) vec4f {
                return vec4f(b.x, 0.0, 0.0, 1.0);
            }
            "
            .into(),
        ),
    });

    let make_layout = || {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        })
    };
    let first_layout = make_layout();
    let second_layout = make_layout();
    let first_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&[0.2f32, 0.0, 0.0, 0.0]),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let second_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&[0.8f32, 0.0, 0.0, 0.0]),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let make_group = |layout: &wgpu::BindGroupLayout, buffer: &wgpu::Buffer| {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        })
    };
    let first_group = make_group(&first_layout, &first_buffer);
    let second_group = make_group(&second_layout, &second_buffer);
    let full_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[Some(&first_layout), Some(&second_layout)],
        immediate_size: 0,
    });
    let gapped_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[None, Some(&second_layout)],
        immediate_size: 0,
    });
    let make_pipeline = |layout: &wgpu::PipelineLayout, entry_point: &str| {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some(entry_point),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        })
    };
    let full_pipeline = make_pipeline(&full_layout, "both_groups");
    let gapped_pipeline = make_pipeline(&gapped_layout, "higher_group_only");
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: 2,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
        texture_binding_view_dimension: None,
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_bind_group(0, &first_group, &[]);
        pass.set_bind_group(1, &second_group, &[]);
        pass.set_pipeline(&full_pipeline);
        pass.set_scissor_rect(0, 0, 1, 1);
        pass.draw(0..3, 0..1);

        pass.set_pipeline(&gapped_pipeline);
        pass.set_bind_group(1, &second_group, &[]);
        pass.set_scissor_rect(1, 0, 1, 1);
        pass.draw(0..3, 0..1);
    }

    let readback = ReadbackBuffers::new(device, &texture);
    readback.copy_from(device, &mut encoder, &texture);
    ctx.queue.submit([encoder.finish()]);
    readback
        .assert_buffer_contents(&ctx, &[51, 204, 0, 255, 204, 0, 0, 255])
        .await;
}
