// P0xx(NV12/P010LE/P012LE/P016LE)平面テクスチャ -> Rgba8Unorm合成。
// neo-media-swscale::ops_chain::SwscaleUniformsとフィールド順・バイト幅を一致させる。
// tap_count_h/v==1固定経路のみ実装(ScaleH/ScaleV opはgraph.rsが比率1:1時に省略するため
// 通常到達しない。tap_buffer_h/vはバインディング形状維持のため宣言のみ、本経路では未使用)。
struct SwscaleUniforms {
    color_matrix: u32,
    color_range: u32,
    bit_depth: u32,
    storage_bits: u32,
    src_width: u32,
    src_height: u32,
    dst_width: u32,
    dst_height: u32,
    tap_count_h: u32,
    tap_count_v: u32,
    _pad0: u32,
    _pad1: u32,
};

@group(0) @binding(0) var<uniform> uniforms: SwscaleUniforms;
@group(0) @binding(1) var plane_y: texture_2d<u32>;
@group(0) @binding(2) var plane_uv: texture_2d<u32>;
@group(0) @binding(3) var<storage, read> tap_buffer_h: array<f32>;
@group(0) @binding(4) var<storage, read> tap_buffer_v: array<f32>;
@group(0) @binding(5) var dst_rgba8: texture_storage_2d<rgba8unorm, write>;

fn normalize_shift() -> u32 {
    return uniforms.storage_bits - uniforms.bit_depth;
}

fn normalize_max() -> f32 {
    return f32((1u << uniforms.bit_depth) - 1u);
}

fn load_y_normalized(coord: vec2<i32>) -> f32 {
    let raw = textureLoad(plane_y, coord, 0).r;
    return f32(raw >> normalize_shift()) / normalize_max();
}

fn load_uv_normalized(coord: vec2<i32>) -> vec2<f32> {
    let raw = textureLoad(plane_uv, coord, 0).rg;
    return vec2<f32>(raw >> vec2<u32>(normalize_shift(), normalize_shift())) / normalize_max();
}

fn yuv_to_rgb(y: f32, u: f32, v: f32, matrix: u32) -> vec3<f32> {
    var kr: f32;
    var kb: f32;
    if (matrix == 0u) {
        kr = 0.299;
        kb = 0.114;
    } else if (matrix == 2u) {
        kr = 0.2627;
        kb = 0.0593;
    } else {
        kr = 0.2126;
        kb = 0.0722;
    }
    let kg = 1.0 - kr - kb;
    let r = y + 2.0 * (1.0 - kr) * v;
    let b = y + 2.0 * (1.0 - kb) * u;
    let g = (y - kr * r - kb * b) / kg;
    return vec3<f32>(r, g, b);
}

@compute @workgroup_size(8, 8, 1)
fn cs_main(@builtin(global_invocation_id) tid: vec3<u32>) {
    if (tid.x >= uniforms.dst_width || tid.y >= uniforms.dst_height) {
        return;
    }

    let src_coord = vec2<i32>(
        i32(tid.x * uniforms.src_width / uniforms.dst_width),
        i32(tid.y * uniforms.src_height / uniforms.dst_height),
    );

    let y_raw = load_y_normalized(src_coord);
    let uv_raw = load_uv_normalized(src_coord / vec2<i32>(2, 2));

    var y: f32;
    var u: f32;
    var v: f32;
    if (uniforms.color_range == 1u) {
        y = y_raw;
        u = uv_raw.x - 0.5;
        v = uv_raw.y - 0.5;
    } else {
        y = (y_raw - 16.0 / 255.0) * (255.0 / 219.0);
        u = (uv_raw.x - 16.0 / 255.0) * (255.0 / 224.0) - 0.5;
        v = (uv_raw.y - 16.0 / 255.0) * (255.0 / 224.0) - 0.5;
    }

    let rgb = clamp(yuv_to_rgb(y, u, v, uniforms.color_matrix), vec3<f32>(0.0), vec3<f32>(1.0));
    textureStore(dst_rgba8, vec2<i32>(tid.xy), vec4<f32>(rgb, 1.0));
}
