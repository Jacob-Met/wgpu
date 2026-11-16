#![cfg(all(feature = "wgsl-in", msl_out))]

use naga::{back::msl, valid};

#[test]
fn baked_binding_array_storage_texture_preserves_access() {
    // A `let` binding of a storage texture from a binding array has no
    // per-global access context. The MSL temporary must retain the texture's
    // declared access, not panic while printing its type.
    for access in ["read", "write", "read_write"] {
        let source = format!(
            r#"
                enable wgpu_binding_array;
                @group(0) @binding(0)
                var t: binding_array<texture_storage_2d<rgba32float, {access}>, 5>;
                fn f() {{
                    let x = t[1u];
                }}
                @compute @workgroup_size(1) fn main() {{ f(); }}
            "#
        );
        let module = naga::front::wgsl::parse_str(&source).unwrap();
        let info = valid::Validator::new(valid::ValidationFlags::all(), valid::Capabilities::all())
            .validate(&module)
            .unwrap();
        let options = msl::Options {
            lang_version: (3, 0),
            ..Default::default()
        };
        let (output, _) =
            msl::write_string(&module, &info, &options, &msl::PipelineOptions::default()).unwrap();
        let expected = format!("metal::texture2d<float, metal::access::{access}> x = t[1].inner;");
        assert!(output.contains(&expected), "{access}: {output}");
    }
}
