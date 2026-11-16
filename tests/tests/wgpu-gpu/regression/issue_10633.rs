use wgpu::util::DeviceExt;
use wgpu_test::{
    apply, gpu_test, FailureCase, GpuTestConfiguration, GpuTestInitializer, TestParameters,
    TestingContext,
};

pub fn all_tests(tests: &mut Vec<GpuTestInitializer>) {
    tests.push(DX12_FMA_CORRECTED_DIVISION);
}

#[apply(gpu_test!)]
static DX12_FMA_CORRECTED_DIVISION: GpuTestConfiguration = GpuTestConfiguration::new()
    .parameters(
        TestParameters::default()
            .test_features_limits()
            // Debug's -Od disables the DXC reassociation this test must catch.
            .remove_instance_flags(wgpu::InstanceFlags::DEBUG)
            .skip(FailureCase::backend(
                wgpu::Backends::all() - wgpu::Backends::DX12,
            )),
    )
    .run_async(dx12_fma_corrected_division);

async fn dx12_fma_corrected_division(ctx: TestingContext) {
    let inputs: [[f32; 2]; 8] = [
        [49.87991, 139.44571],
        [24.945532, 189.52682],
        [111.09612, 50.863766],
        [31.287197, 23.160624],
        [109.71373, 104.92078],
        [25.646706, 44.17301],
        [120.74079, 70.65736],
        [53.069073, 192.94165],
    ];
    let device = &ctx.device;
    let input = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&inputs),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let output = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: (inputs.len() * 8) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: output.size(),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(
            "
            @group(0) @binding(0) var<storage, read> inputs: array<vec2f>;
            @group(0) @binding(1) var<storage, read_write> outputs: array<vec2u>;

            @compute @workgroup_size(8)
            fn main(@builtin(global_invocation_id) id: vec3u) {
                let pair = inputs[id.x];
                let q = pair.x / pair.y;
                let r = fma(-q, pair.y, pair.x);
                outputs[id.x] = vec2u(bitcast<u32>(q), bitcast<u32>(fma(r / pair.y, 1.0, q)));
            }
            "
            .into(),
        ),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: None,
        module: &shader,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: input.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: output.as_entire_binding(),
            },
        ],
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(1, 1, 1);
    }
    encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, output.size());
    ctx.queue.submit([encoder.finish()]);
    let slice = readback.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| ());
    ctx.async_poll(wgpu::PollType::wait_indefinitely())
        .await
        .unwrap();
    let bytes = slice.get_mapped_range().unwrap();
    for (i, pair) in bytes.chunks_exact(8).enumerate() {
        assert_eq!(&pair[..4], &pair[4..], "quotient mismatch at input {i}");
    }
}
