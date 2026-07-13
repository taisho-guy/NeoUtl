struct Uniforms { p: array<vec4<f32>, 1> }
@group(0) @binding(2) var<uniform> u: Uniforms;

fn rgb_to_hsv(c: vec3<f32>) -> vec3<f32> {
    let maxc = max(c.r, max(c.g, c.b));
    let minc = min(c.r, min(c.g, c.b));
    let v = maxc;
    let delta = maxc - minc;
    var h = 0.0;
    var s = 0.0;
    if (maxc > 0.0) { s = delta / maxc; }
    if (delta > 0.0001) {
        if (maxc == c.r) { h = (c.g - c.b) / delta; }
        else if (maxc == c.g) { h = 2.0 + (c.b - c.r) / delta; }
        else { h = 4.0 + (c.r - c.g) / delta; }
        h = h / 6.0;
        if (h < 0.0) { h += 1.0; }
    }
    return vec3<f32>(h, s, v);
}

fn hsv_to_rgb(c: vec3<f32>) -> vec3<f32> {
    let h = c.x * 6.0;
    let s = c.y;
    let v = c.z;
    let i = floor(h);
    let f = h - i;
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    let m = i32(i) % 6;
    if (m == 0) { return vec3<f32>(v, t, p); }
    if (m == 1) { return vec3<f32>(q, v, p); }
    if (m == 2) { return vec3<f32>(p, v, t); }
    if (m == 3) { return vec3<f32>(p, q, v); }
    if (m == 4) { return vec3<f32>(t, p, v); }
    return vec3<f32>(v, p, q);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let brightness = u.p[0].x;
    let contrast = u.p[0].y;
    let saturation = u.p[0].z;
    let hue = u.p[0].w;
    var c = textureSample(input_tex, input_sampler, in.uv);
    var hsv = rgb_to_hsv(c.rgb);
    hsv.x = fract(hsv.x + hue / 360.0);
    hsv.y = clamp(hsv.y * (1.0 + saturation), 0.0, 1.0);
    var rgb = hsv_to_rgb(hsv);
    rgb = (rgb - vec3<f32>(0.5)) * (1.0 + contrast) + vec3<f32>(0.5) + vec3<f32>(brightness);
    return vec4<f32>(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)), c.a);
}
