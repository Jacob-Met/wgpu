use wgpu_test::{
    apply, fail, gpu_test, image::ReadbackBuffers, GpuTestConfiguration, GpuTestInitializer,
    TestParameters, TestingContext,
};

pub fn all_tests(vec: &mut Vec<GpuTestInitializer>) {
    vec.extend([
        DONT_CARE,
        DONT_CARE_COLOR_STRICT_WEBGPU_COMPLIANCE,
        DONT_CARE_DEPTH_STRICT_WEBGPU_COMPLIANCE,
        DONT_CARE_STENCIL_STRICT_WEBGPU_COMPLIANCE,
        SCISSOR_RESOLVE_BLITS_ENTIRE_ATTACHMENT,
    ]);
}

// The scissor applied to a draw must not clip the MSAA color resolve.
// This reproduces the stale-pixel artifacts from gfx-rs/wgpu#10620 on GL.
#[apply(gpu_test!)]
static SCISSOR_RESOLVE_BLITS_ENTIRE_ATTACHMENT: GpuTestConfiguration =
    GpuTestConfiguration::new().run_async(scissor_resolve_blits_entire_attachment);

async fn scissor_resolve_blits_entire_attachment(ctx: TestingContext) {
    const SIZE: u32 = 4;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let texture = |sample_count, usage| {
        ctx.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: SIZE,
                height: SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            view_formats: &[],
            texture_binding_view_dimension: None,
        })
    };
    let msaa = texture(4, wgpu::TextureUsages::RENDER_ATTACHMENT);
    let resolve = texture(
        1,
        wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
    );
    let msaa_view = msaa.create_view(&Default::default());
    let resolve_view = resolve.create_view(&Default::default());
    let shader = ctx
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(
                r#"
                @vertex
                fn vs_main(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4f {
                    let p = array<vec2f, 3>(vec2f(-1.0, -1.0), vec2f(3.0, -1.0), vec2f(-1.0, 3.0));
                    return vec4f(p[vi], 0.0, 1.0);
                }
                @fragment
                fn fs_main() -> @location(0) vec4f { return vec4f(1.0); }
            "#
                .into(),
            ),
        });
    let pipeline = ctx
        .device
        .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 4,
                ..Default::default()
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(format.into())],
            }),
            multiview_mask: None,
            cache: None,
        });
    let readback = ReadbackBuffers::new(&ctx.device, &resolve);
    let mut encoder = ctx.device.create_command_encoder(&Default::default());
    {
        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("prime resolve target red"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &resolve_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::RED),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
    }
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("scissored multisample pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &msaa_view,
                depth_slice: None,
                resolve_target: Some(&resolve_view),
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&pipeline);
        pass.set_scissor_rect(SIZE / 2, SIZE / 2, SIZE / 2, SIZE / 2);
        pass.draw(0..3, 0..1);
    }
    readback.copy_from(&ctx.device, &mut encoder, &resolve);
    ctx.queue.submit([encoder.finish()]);

    // Resolve must overwrite *every* pixel, including outside the final draw
    // scissor. The source is all grayscale (black plus white coverage), while
    // the pre-existing resolve target is red. A clipped blit leaves red behind.
    let data = readback.retrieve(&ctx, wgpu::TextureAspect::All).await;
    let pixels: Vec<_> = data.all().chunks_exact(4).collect();
    assert_eq!(pixels.len(), (SIZE * SIZE) as usize);
    for (index, pixel) in pixels.iter().enumerate() {
        assert_eq!(
            pixel[0], pixel[1],
            "unresolved red pixel at {index}: {pixels:?}"
        );
        assert_eq!(
            pixel[1], pixel[2],
            "unexpected color at {index}: {pixels:?}"
        );
        assert_eq!(pixel[3], 255, "unexpected alpha at {index}: {pixels:?}");
    }
    // The clear and the scissored white draw must both contribute.
    assert_eq!(pixels[0], [0, 0, 0, 255]);
    assert_eq!(pixels[(SIZE * SIZE - 1) as usize], [255, 255, 255, 255]);
}

#[apply(gpu_test!)]
static DONT_CARE: GpuTestConfiguration = GpuTestConfiguration::new()
    .parameters(TestParameters::default())
    .run_async(dont_care);

async fn dont_care(ctx: TestingContext) {
    let shader_src = "
        const triangles = array<vec2f, 3>(vec2f(-1.0, -1.0), vec2f(3.0, -1.0), vec2f(-1.0, 3.0));

        @vertex
        fn vs_main(@builtin(vertex_index) vertex_index: u32) -> @builtin(position) vec4f {
            return vec4f(triangles[vertex_index], 0.0, 1.0);
        }

        @fragment
        fn fs_main() -> @location(0) vec4f {
            return vec4f(127.0 / 255.0);
        }
    ";

    let shader = ctx
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(shader_src.into()),
        });

    let pipeline_desc = wgpu::RenderPipelineDescriptor {
        label: None,
        layout: None,
        vertex: wgpu::VertexState {
            buffers: &[],
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba8Unorm,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    };
    let pipeline = ctx.device.create_render_pipeline(&pipeline_desc);

    let out_texture = ctx.device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: 1,
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

    let readbacks = ReadbackBuffers::new(&ctx.device, &out_texture);

    let mut encoder = ctx
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

    let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: None,
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: &out_texture.create_view(&wgpu::TextureViewDescriptor::default()),
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::DontCare(unsafe { wgpu::LoadOpDontCare::enabled() }),
                store: wgpu::StoreOp::Store,
            },
        })],
        ..Default::default()
    });
    rpass.set_pipeline(&pipeline);
    rpass.draw(0..3, 0..1);

    drop(rpass);

    readbacks.copy_from(&ctx.device, &mut encoder, &out_texture);

    ctx.queue.submit([encoder.finish()]);

    // Assert that DONT_CARE load op was fully overridden by the draw.
    readbacks
        .assert_buffer_contents(&ctx, &[127, 127, 127, 127])
        .await;
}

#[apply(gpu_test!)]
static DONT_CARE_COLOR_STRICT_WEBGPU_COMPLIANCE: GpuTestConfiguration = GpuTestConfiguration::new()
    .parameters(
        TestParameters::default()
            .instance_flags(wgpu::InstanceFlags::STRICT_WEBGPU_COMPLIANCE)
            .enable_noop(),
    )
    .run_sync(|ctx| {
        let tex = ctx.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
            texture_binding_view_dimension: None,
        });
        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = ctx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::DontCare(unsafe { wgpu::LoadOpDontCare::enabled() }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        fail(
            &ctx.device,
            || encoder.finish(),
            Some("STRICT_WEBGPU_COMPLIANCE"),
        );
    });

#[apply(gpu_test!)]
static DONT_CARE_DEPTH_STRICT_WEBGPU_COMPLIANCE: GpuTestConfiguration = GpuTestConfiguration::new()
    .parameters(
        TestParameters::default()
            .instance_flags(wgpu::InstanceFlags::STRICT_WEBGPU_COMPLIANCE)
            .enable_noop(),
    )
    .run_sync(|ctx| {
        let tex = ctx.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth16Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
            texture_binding_view_dimension: None,
        });
        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = ctx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::DontCare(unsafe { wgpu::LoadOpDontCare::enabled() }),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
        fail(
            &ctx.device,
            || encoder.finish(),
            Some("STRICT_WEBGPU_COMPLIANCE"),
        );
    });

#[apply(gpu_test!)]
static DONT_CARE_STENCIL_STRICT_WEBGPU_COMPLIANCE: GpuTestConfiguration =
    GpuTestConfiguration::new()
        .parameters(
            TestParameters::default()
                .instance_flags(wgpu::InstanceFlags::STRICT_WEBGPU_COMPLIANCE)
                .enable_noop(),
        )
        .run_sync(|ctx| {
            let tex = ctx.device.create_texture(&wgpu::TextureDescriptor {
                label: None,
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Stencil8,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
                texture_binding_view_dimension: None,
            });
            let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
            let mut encoder = ctx
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
            encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &view,
                    depth_ops: None,
                    stencil_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::DontCare(unsafe { wgpu::LoadOpDontCare::enabled() }),
                        store: wgpu::StoreOp::Store,
                    }),
                }),
                ..Default::default()
            });
            fail(
                &ctx.device,
                || encoder.finish(),
                Some("STRICT_WEBGPU_COMPLIANCE"),
            );
        });
