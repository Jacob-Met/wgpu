#![cfg(all(feature = "wgsl-in", feature = "wgsl-out"))]

use naga::{back::wgsl, valid};

#[test]
fn nested_zero_vector_components_keep_their_lanes() {
    let source = r#"
        const y = vec4u(vec2u(), 7u, 9u).y;
        const m = mat2x2f(vec2f(), vec2f(1.0));
        const v = vec2f(1.0, 2.0) * m;
        @group(0) @binding(0) var<storage, read_write> o: array<u32, 8>;
        @compute @workgroup_size(1) fn main() {
            o[0] = vec4u(vec2u(), 7u, 9u).y;
            o[1] = vec4u(vec2u(), 7u, 9u).z;
            o[2] = vec4u(vec2u(), 7u, 9u)[1];
            o[3] = bitcast<u32>(vec4f(1.0, vec2f(), 2.0).z);
            o[4] = bitcast<u32>(vec4f(vec3f(), 1.0).w);
            o[5] = vec4u(7u, vec2u(), 9u).y;
            o[6] = vec4u(vec4u().zx, vec4u(255u, 3u, 5u, 2836987521u).wy).z;
            o[7] = vec4u(vec2u(), 7u, 9u)[3];
        }
    "#;
    let module = naga::front::wgsl::parse_str(source).unwrap();
    let info = valid::Validator::new(valid::ValidationFlags::all(), valid::Capabilities::all())
        .validate(&module)
        .unwrap();
    let output = wgsl::write_string(&module, &info, wgsl::WriterFlags::empty()).unwrap();
    for expected in [
        "const y: u32 = 0u;",
        "const v: vec2<f32> = vec2<f32>(0f, 3f);",
        "o[0] = 0u;",
        "o[1] = 7u;",
        "o[2] = 0u;",
        "o[3] = bitcast<u32>(0f);",
        "o[4] = bitcast<u32>(1f);",
        "o[5] = 0u;",
        "o[6] = 2836987521u;",
        "o[7] = 9u;",
    ] {
        assert!(
            output.contains(expected),
            "missing '{expected}' in:\n{output}"
        );
    }
}
