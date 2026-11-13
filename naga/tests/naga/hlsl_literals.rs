#![cfg(all(feature = "wgsl-in", hlsl_out))]

use naga::{back::hlsl, valid};

#[test]
fn f32_literals_are_typed_for_dxc() {
    let source = r#"
        @fragment
        fn main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
            let m = select(-1f, 1f, uv.y > 0.5f)
                * select(1f, -1f, uv.x > 0.5f);
            return vec4<f32>(m, 0f, 0f, 1f);
        }
    "#;
    let module = naga::front::wgsl::parse_str(source).unwrap();
    let info = valid::Validator::new(valid::ValidationFlags::all(), valid::Capabilities::all())
        .validate(&module)
        .unwrap();
    let options = hlsl::Options {
        shader_model: hlsl::ShaderModel::V6_0,
        ..Default::default()
    };
    let pipeline_options = hlsl::PipelineOptions::default();
    let mut output = String::new();
    hlsl::Writer::new(&mut output, &options, &pipeline_options)
        .write(&module, &info, None)
        .unwrap();
    assert!(
        output.contains("1.0f") && output.contains("-1.0f") && output.contains("0.5f"),
        "HLSL f32 literals must be explicitly typed to prevent DXC from promoting constant-only expressions to double: {output}"
    );
}

#[test]
fn hlsl_storage_constant_index_overflow_does_not_alias_first_element() {
    fn compile(index: u32) -> Result<String, hlsl::Error> {
        let source = format!(
            "@group(0) @binding(0) var<storage, read_write> buf: array<u32>;\n@compute @workgroup_size(1) fn main() {{ buf[{index}] = 42u; }}"
        );
        let module = naga::front::wgsl::parse_str(&source).unwrap();
        let info = valid::Validator::new(valid::ValidationFlags::all(), valid::Capabilities::all())
            .validate(&module)
            .unwrap();
        let options = hlsl::Options {
            shader_model: hlsl::ShaderModel::V6_0,
            ..Default::default()
        };
        let mut output = String::new();
        hlsl::Writer::new(&mut output, &options, &hlsl::PipelineOptions::default())
            .write(&module, &info, None)?;
        Ok(output)
    }

    let below_overflow = compile(1_073_741_823).unwrap();
    assert!(below_overflow.contains("4294967292"), "{below_overflow}");

    let err = compile(1_073_741_824).unwrap_err();
    assert!(
        matches!(
            err,
            hlsl::Error::StorageAccessOffsetOverflow {
                index: 1_073_741_824,
                stride: 4
            }
        ),
        "{err}"
    );
}
