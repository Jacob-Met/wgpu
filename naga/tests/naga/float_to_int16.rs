#![cfg(all(feature = "wgsl-in", hlsl_out, msl_out))]

use naga::{
    back::{hlsl, msl},
    valid,
};

// These conversions used to panic in both backends: only 32- and 64-bit
// float-to-integer helper names were handled, despite SHADER_INT16 validation.
#[test]
fn runtime_float_to_int16_is_clamped_in_hlsl_and_msl() {
    let source = r#"
        enable wgpu_int16;
        enable f16;
        var<private> x: f32 = 1.5;
        var<private> h: f16 = 1.5h;
        @compute @workgroup_size(1)
        fn main() {
            _ = i16(x);
            _ = u16(x);
            _ = i16(h);
            _ = u16(h);
            _ = vec2<i16>(vec2<f32>(x, -x));
            _ = vec2<u16>(vec2<f16>(h, h));
        }
    "#;
    let module = naga::front::wgsl::parse_str(source).unwrap();
    let info = valid::Validator::new(valid::ValidationFlags::all(), valid::Capabilities::all())
        .validate(&module)
        .unwrap();

    let hlsl_options = hlsl::Options {
        shader_model: hlsl::ShaderModel::V6_2,
        ..Default::default()
    };
    let mut hlsl_output = String::new();
    hlsl::Writer::new(
        &mut hlsl_output,
        &hlsl_options,
        &hlsl::PipelineOptions::default(),
    )
    .write(&module, &info, None)
    .unwrap();
    let (msl_output, _) = msl::write_string(
        &module,
        &info,
        &msl::Options::default(),
        &msl::PipelineOptions::default(),
    )
    .unwrap();

    for (backend, output) in [("hlsl", hlsl_output), ("msl", msl_output)] {
        for kind in ["i", "u"] {
            for source_type in ["float", "half"] {
                assert!(
                    output.contains(&format!("naga_f2{kind}16({source_type}")),
                    "{backend} must define a wrapper for {source_type} -> {kind}16:\n{output}"
                );
            }
        }
        assert!(output.contains("-32768"), "{backend}: {output}");
        assert!(output.contains("32767"), "{backend}: {output}");
        assert!(output.contains("65535"), "{backend}: {output}");
        assert!(
            output.contains("naga_f2i16(") && output.contains("naga_f2u16("),
            "{backend}: {output}"
        );
    }
}
