#![cfg(all(feature = "wgsl-in", hlsl_out))]

use naga::{back::hlsl, valid};

#[test]
fn matrix_member_stores_modify_the_callers_struct() {
    let source = r#"
        struct R { m: mat2x2<f32> }
        @group(0) @binding(0) var<storage, read_write> out: array<f32>;

        fn make(a: f32) -> R {
            var r: R;
            r.m = mat2x2<f32>(vec2<f32>(1.0, 2.0), vec2<f32>(3.0, a));
            let index = i32(a);
            r.m[index] = vec2<f32>(5.0, 6.0);
            r.m[index][1] = 7.0;
            return r;
        }

        @compute @workgroup_size(1)
        fn main() {
            let r = make(0.5);
            out[0] = r.m[0][0];
            out[1] = r.m[0][1];
            out[2] = r.m[1][0];
            out[3] = r.m[1][1];
        }
    "#;
    let module = naga::front::wgsl::parse_str(source).unwrap();
    let info = valid::Validator::new(valid::ValidationFlags::all(), valid::Capabilities::all())
        .validate(&module)
        .unwrap();

    let mut output = String::new();
    hlsl::Writer::new(
        &mut output,
        &hlsl::Options::default(),
        &hlsl::PipelineOptions::default(),
    )
    .write(&module, &info, None)
    .unwrap();

    for helper in ["SetMatmOnR", "SetMatVecmOnR", "SetMatScalarmOnR"] {
        assert!(
            output.contains(&format!("void {helper}(inout R obj,")),
            "{helper} must modify its caller's struct, not a by-value copy:\n{output}"
        );
        assert!(
            output.contains(&format!("{helper}(r,")),
            "{helper} must be exercised by the regression shader:\n{output}"
        );
    }
}
