@group(0) @binding(0) var src_tex: texture_2d<f32>;
@group(0) @binding(1) var<storage, read_write> acc: array<atomic<u32>, 5>;

const SCALE: f32 = 1000000.0;

@compute @workgroup_size(8, 8)
fn cs_main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let dims = textureDimensions(src_tex);
    if (gid.x >= dims.x || gid.y >= dims.y) {
        return;
    }
    let c = textureLoad(src_tex, vec2<i32>(gid.xy), 0);
    atomicAdd(&acc[0], u32(clamp(c.r, 0.0, 1.0) * SCALE));
    atomicAdd(&acc[1], u32(clamp(c.g, 0.0, 1.0) * SCALE));
    atomicAdd(&acc[2], u32(clamp(c.b, 0.0, 1.0) * SCALE));
    atomicAdd(&acc[3], u32(clamp(c.a, 0.0, 1.0) * SCALE));
    atomicAdd(&acc[4], 1u);
}
