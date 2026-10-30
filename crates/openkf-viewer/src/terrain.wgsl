struct Globals {
    view_proj: mat4x4<f32>,
    cam_pos: vec4<f32>,
    fog: vec4<f32>,
};
@group(0) @binding(0) var<uniform> g: Globals;

// x,y = world units per repeat; z = alpha map uses red channel; w = has alpha map
struct Layer { params: vec4<f32> };
@group(1) @binding(0) var layer_tex: texture_2d<f32>;
@group(1) @binding(1) var alpha_tex: texture_2d<f32>;
@group(1) @binding(2) var wrap: sampler;
@group(1) @binding(3) var clampy: sampler;
@group(1) @binding(4) var<uniform> layer: Layer;

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
    // Converted space: UE X -> x, UE Y -> z.
    let tuv = vec2<f32>(i.wpos.x, i.wpos.z) / layer.params.xy;
    let c = textureSample(layer_tex, wrap, tuv);
    let a = textureSample(alpha_tex, clampy, i.uv);
    var w = select(a.a, a.r, layer.params.z > 0.5);
    w = select(1.0, w, layer.params.w > 0.5);
    let n = normalize(i.nrm);
    let l = normalize(vec3<f32>(0.35, 0.8, 0.45));
    let hemi = mix(0.45, 0.75, n.y * 0.5 + 0.5);
    var rgb = c.rgb * (hemi + 0.35 * max(dot(n, l), 0.0));
    let d = distance(i.wpos, g.cam_pos.xyz);
    let f = clamp((d - g.fog.x) / (g.fog.y - g.fog.x), 0.0, 1.0);
    rgb = mix(rgb, vec3<f32>(0.06, 0.06, 0.07), f * 0.85);
    return vec4<f32>(rgb, w);
}
