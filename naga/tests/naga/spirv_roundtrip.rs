fn words_to_bytes(words: &[u32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(words.len() * 4);
    for word in words {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    bytes
}

#[test]
fn atomic_compare_exchange_roundtrip_to_spirv() {
    let source = include_str!("../in/wgsl/atomicCompareExchange.wgsl");
    let module = naga::front::wgsl::parse_str(source).unwrap();

    let mut validator = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::empty(),
    );
    let info = validator.validate(&module).unwrap();

    let pipeline_options = naga::back::spv::PipelineOptions {
        entry_point: "test_atomic_compare_exchange_u32".to_string(),
        shader_stage: naga::ShaderStage::Compute,
    };
    let options = naga::back::spv::Options::default();
    let spv_words =
        naga::back::spv::write_vec(&module, &info, &options, Some(&pipeline_options)).unwrap();

    let spv_bytes = words_to_bytes(&spv_words);
    let parsed = naga::front::spv::parse_u8_slice(
        &spv_bytes,
        &naga::front::spv::Options {
            adjust_coordinate_space: true,
            strict_capabilities: false,
            block_ctx_dump_prefix: None,
        },
    )
    .unwrap();

    let parsed_info = validator.validate(&parsed).unwrap();
    naga::back::spv::write_vec(&parsed, &parsed_info, &options, Some(&pipeline_options)).unwrap();
}

// SPIR-V producers may declare only a vec3<i32> coordinate for a 2D arrayed
// storage image. Naga must synthesize vec2<i32> when splitting off the layer.
#[cfg_attr(miri, ignore)]
#[test]
fn arrayed_storage_image_without_short_coordinate_type() {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };

    let assembly = r#"
OpCapability Shader
OpMemoryModel Logical GLSL450
OpEntryPoint GLCompute %main "main" %gOut
OpExecutionMode %main LocalSize 1 1 1
OpDecorate %gOut DescriptorSet 0
OpDecorate %gOut Binding 0
%void = OpTypeVoid
%function = OpTypeFunction %void
%float = OpTypeFloat 32
%v4float = OpTypeVector %float 4
%int = OpTypeInt 32 1
%v3int = OpTypeVector %int 3
%image = OpTypeImage %float 2D 0 1 0 2 Rgba32f
%ptr = OpTypePointer UniformConstant %image
%gOut = OpVariable %ptr UniformConstant
%zero = OpConstant %int 0
%coords = OpConstantComposite %v3int %zero %zero %zero
%one = OpConstant %float 1
%color = OpConstantComposite %v4float %one %one %one %one
%main = OpFunction %void None %function
%label = OpLabel
%loaded = OpLoad %image %gOut
OpImageWrite %loaded %coords %color
OpReturn
OpFunctionEnd
"#;
    let mut assembler = Command::new("spirv-as")
        .args(["--target-env", "spv1.5", "-o", "-", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spirv-as is needed for SPIR-V frontend tests");
    assembler
        .stdin
        .take()
        .unwrap()
        .write_all(assembly.as_bytes())
        .unwrap();
    let output = assembler.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "spirv-as failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let module = naga::front::spv::parse_u8_slice(
        &output.stdout,
        &naga::front::spv::Options {
            adjust_coordinate_space: true,
            strict_capabilities: false,
            block_ctx_dump_prefix: None,
        },
    )
    .unwrap();
    let info = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
    let wgsl =
        naga::back::wgsl::write_string(&module, &info, naga::back::wgsl::WriterFlags::empty())
            .unwrap();
    assert!(wgsl.contains("texture_storage_2d_array"));
    assert!(wgsl.contains("textureStore("));
}
