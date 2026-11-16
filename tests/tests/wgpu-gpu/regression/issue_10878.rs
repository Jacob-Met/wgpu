use wgpu::*;
use wgpu_test::{
    apply, gpu_test, GpuTestConfiguration, GpuTestInitializer, TestParameters, TestingContext,
};

pub fn all_tests(tests: &mut Vec<GpuTestInitializer>) {
    tests.push(DEPTH_TEXTURE_TEXEL_FETCH);
}

/// WebGL2 considers depth textures incomplete with their default linear/mipmap
/// filters, so textureLoad silently returns zero (gfx-rs/wgpu#10878).
#[apply(gpu_test!)]
static DEPTH_TEXTURE_TEXEL_FETCH: GpuTestConfiguration = GpuTestConfiguration::new()
    .parameters(TestParameters::default().limits(Limits::downlevel_defaults()))
    .run_async(depth_texture_texel_fetch);

async fn depth_texture_texel_fetch(ctx: TestingContext) {
    let device = &ctx.device;
    let size = Extent3d {
        width: 1,
        height: 1,
        depth_or_array_layers: 1,
    };
    let depth = device.create_texture(&TextureDescriptor {
        label: Some("depth texture for texelFetch"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Depth32Float,
        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
        texture_binding_view_dimension: None,
    });
    let depth_view = depth.create_view(&Default::default());
    let color = device.create_texture(&TextureDescriptor {
        label: Some("depth sample output"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
        view_formats: &[],
        texture_binding_view_dimension: None,
    });
    let color_view = color.create_view(&Default::default());
    let readback = device.create_buffer(&BufferDescriptor {
        label: None,
        size: 256,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("depth texelFetch shader"),
        source: ShaderSource::Wgsl(
            "
            @group(0) @binding(0) var depth_tex: texture_2d<f32>;
            @vertex fn vertex(@builtin(vertex_index) index: u32)
                -> @builtin(position) vec4f {
                let p = array<vec2f, 3>(
                    vec2f(-1.0, -1.0), vec2f(3.0, -1.0), vec2f(-1.0, 3.0));
                return vec4f(p[index], 0.0, 1.0);
            }
            @fragment fn fragment() -> @location(0) vec4f {
                let d = textureLoad(depth_tex, vec2i(0, 0), 0).r;
                return vec4f(d, 0.0, 0.0, 1.0);
            }
            "
            .into(),
        ),
    });
    let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: None,
        layout: None,
        vertex: VertexState {
            module: &shader,
            entry_point: Some("vertex"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(FragmentState {
            module: &shader,
            entry_point: Some("fragment"),
            compilation_options: Default::default(),
            targets: &[Some(TextureFormat::Rgba8Unorm.into())],
        }),
        primitive: PrimitiveState::default(),
        depth_stencil: None,
        multisample: MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });
    let bind_group = device.create_bind_group(&BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[BindGroupEntry {
            binding: 0,
            resource: BindingResource::TextureView(&depth_view),
        }],
    });

    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let _pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("write known depth"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &color_view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(Color::BLACK),
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                view: &depth_view,
                depth_ops: Some(Operations {
                    load: LoadOp::Clear(0.625),
                    store: StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
    }
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("fetch depth into color"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &color_view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(Color::BLACK),
                    store: StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
    encoder.copy_texture_to_buffer(
        TexelCopyTextureInfo {
            texture: &color,
            mip_level: 0,
            origin: Origin3d::ZERO,
            aspect: TextureAspect::All,
        },
        TexelCopyBufferInfo {
            buffer: &readback,
            layout: TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(1),
            },
        },
        size,
    );
    ctx.queue.submit([encoder.finish()]);
    readback.slice(..).map_async(MapMode::Read, |_| ());
    ctx.async_poll(PollType::wait_indefinitely()).await.unwrap();
    let pixels = readback.slice(..).get_mapped_range().unwrap();
    assert!(
        (157..=161).contains(&pixels[0]),
        "depth 0.625 should produce a red pixel near 159, got {:?}",
        &pixels[..4]
    );
    assert_eq!(&pixels[1..4], &[0, 0, 255]);
}
