struct Globals {
    view_proj: mat4x4<f32>,
    cam_pos: vec4<f32>,
    // x = fog start, y = fog end
    fog: vec4<f32>,
};
@group(0) @binding(0) var<uniform> g: Globals;

struct Material { params: vec4<f32> }; // x = alpha test
@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;
@group(1) @binding(2) var<uniform> mat: Material;
@group(2) @binding(0) var lightmap: texture_2d<f32>;
@group(2) @binding(1) var lm_samp: sampler;

struct VIn {
    @location(0) pos: vec3<f32>,
    @location(1) nrm: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) light: vec4<f32>,
    @location(4) lm_uv: vec2<f32>,
};
struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) nrm: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) wpos: vec3<f32>,
    @location(3) light: vec4<f32>,
    @location(4) lm_uv: vec2<f32>,
};

@vertex
fn vs_main(v: VIn) -> VOut {
    var o: VOut;
    o.clip = g.view_proj * vec4<f32>(v.pos, 1.0);
    o.nrm = v.nrm;
    o.uv = v.uv;
    o.wpos = v.pos;
    o.light = v.light;
    o.lm_uv = v.lm_uv;
    return o;
}

@fragment
fn fs_main(i: VOut) -> @location(0) vec4<f32> {
    let c = textureSample(tex, samp, i.uv);
    let lm = textureSample(lightmap, lm_samp, i.lm_uv).rgb;
    if (mat.params.x > 0.5 && c.a < 0.5) {
        discard;
    }
    // No lightmaps yet: hemispheric ambient + one key light.
    let n = normalize(i.nrm);
    let l = normalize(vec3<f32>(0.35, 0.8, 0.45));
    let hemi = mix(0.45, 0.75, n.y * 0.5 + 0.5);
    let approx = vec3<f32>(hemi + 0.35 * max(dot(n, l), 0.0));
    // UE2 static mesh lighting: texture * 2 * baked vertex color.
    // light.a: 1.0 = baked vertex color, ~0.25 = BSP lightmap, 0 = approximate.
    // Baked black (stale/unrebuilt instance colors) would render pitch black,
    // so floor the vertex-color branch at 45% of the approximate shading: deep
    // shadow stays dark but textured instead of a flat black slab.
    var shade = select(approx, max(i.light.rgb * 2.0, approx * 0.45), i.light.a > 0.5);
    shade = select(shade, lm * 2.0, i.light.a > 0.1 && i.light.a < 0.5);
    var rgb = c.rgb * shade;
    let d = distance(i.wpos, g.cam_pos.xyz);
    let f = clamp((d - g.fog.x) / (g.fog.y - g.fog.x), 0.0, 1.0);
    rgb = mix(rgb, vec3<f32>(0.06, 0.06, 0.07), f * 0.85);
    return vec4<f32>(rgb, 1.0);
}

// Translucent kinds (alpha / additive / modulate): unlit texture color, fogged.
@fragment
fn fs_translucent(i: VOut) -> @location(0) vec4<f32> {
    let c = textureSample(tex, samp, i.uv);
    let d = distance(i.wpos, g.cam_pos.xyz);
    let f = clamp((d - g.fog.x) / (g.fog.y - g.fog.x), 0.0, 1.0);
    return vec4<f32>(c.rgb * (1.0 - f * 0.85), c.a);
}
