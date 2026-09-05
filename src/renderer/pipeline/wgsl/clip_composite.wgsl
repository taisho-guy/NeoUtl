struct VOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VOut {
    var uv = vec2<f32>(f32((vertex_index << 1u) & 2u), f32(vertex_index & 2u));
    var out: VOut;
    out.position = vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
    out.uv = uv;
    return out;
}

struct ClipUniform {
    mode: u32,
    chroma_hue: f32,
    chroma_tolerance: f32,
    blend_edge: u32,
};

@group(0) @binding(0) var content_tex: texture_2d<f32>;
@group(0) @binding(1) var mold_tex: texture_2d<f32>;
@group(0) @binding(2) var clip_sampler: sampler;
@group(0) @binding(3) var content_depth: texture_depth_2d;
@group(0) @binding(4) var<uniform> u: ClipUniform;

fn rgb_to_hue(c: vec3<f32>) -> f32 {
    let mx = max(c.r, max(c.g, c.b));
    let mn = min(c.r, min(c.g, c.b));
    let delta = mx - mn;
    if (delta == 0.0) {
        return 0.0;
    }
    if (mx == c.r) {
        return 60.0 * (((c.g - c.b) / delta) % 6.0);
    }
    if (mx == c.g) {
        return 60.0 * ((c.b - c.r) / delta + 2.0);
    }
    return 60.0 * ((c.r - c.g) / delta + 4.0);
}

fn hue_distance(a: f32, b: f32) -> f32 {
    let d = ((a - b) % 360.0 + 360.0) % 360.0;
    return min(d, 360.0 - d);
}

struct FOut {
    @location(0) color: vec4<f32>,
    @builtin(frag_depth) depth: f32,
};

@fragment
fn fs_main(in: VOut) -> FOut {
    let content = textureSample(content_tex, clip_sampler, in.uv);
    let mold = textureSample(mold_tex, clip_sampler, in.uv);
    var mask: f32;
    switch (u.mode) {
        case 0u: { mask = mold.a; }
        case 1u: { mask = 1.0 - mold.a; }
        case 2u: { mask = dot(mold.rgb, vec3<f32>(0.2126, 0.7152, 0.0722)); }
        case 3u: { mask = 1.0 - dot(mold.rgb, vec3<f32>(0.2126, 0.7152, 0.0722)); }
        default: {
            let d = hue_distance(rgb_to_hue(mold.rgb), u.chroma_hue);
            mask = select(0.0, 1.0, d > u.chroma_tolerance);
        }
    }
    if (u.blend_edge == 0u) {
        mask = select(0.0, 1.0, mask > 0.5);
    }
    var out: FOut;
    out.color = vec4<f32>(content.rgb, content.a * mask);
    out.depth = textureLoad(content_depth, vec2<i32>(in.position.xy), 0);
    return out;
}
