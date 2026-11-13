#![cfg(all(feature = "wgsl-in", hlsl_out))]

use naga::{back::hlsl, valid};

#[test]
fn i64_literals_keep_all_bits_and_signed_comparisons_in_dxc() {
    let source = r#"
        @group(0) @binding(0) var<storage, read_write> result: array<u64>;
        @compute @workgroup_size(1)
        fn main() {
            var x: i64 = 4294967296li;
            result[0] = bitcast<u64>(x);
            result[1] = 18446744073709551615lu;
            result[2] = 1lu << 40u;
            if (x < 0li) {
                result[3] = 1lu;
            }
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
    for typed_literal in ["4294967296LL", "18446744073709551615uLL", "1uLL", "0LL"] {
        assert!(
            output.contains(typed_literal),
            "64-bit literals must be typed explicitly for DXC -HV 2018; expected {typed_literal} in {output}"
        );
    }
}
