struct Globals {
    view_proj: mat4x4<f32>,
    cam_pos: vec4<f32>,
    fog: vec4<f32>,
};
@group(0) @binding(0) var<uniform> g: Globals;

struct VIn {
    @location(0) pos: vec3<f32>,
    @location(1) color: vec4<f32>,
    @location(2) uv: vec2<f32>,
};
struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) wpos: vec3<f32>,
};

@vertex
fn vs_main(v: VIn) -> VOut {
    var o: VOut;
    o.clip = g.view_proj * vec4<f32>(v.pos, 1.0);
    o.color = v.color;
    o.uv = v.uv;
    o.wpos = v.pos;
    return o;
}

@fragment
fn fs_main(i: VOut) -> @location(0) vec4<f32> {
    // Soft round sprite: full core, smooth edge falloff.
    let d = distance(i.uv, vec2<f32>(0.5, 0.5)) * 2.0;
    let a = clamp(1.0 - d, 0.0, 1.0);
    let dist = distance(i.wpos, g.cam_pos.xyz);
    let f = clamp((dist - g.fog.x) / (g.fog.y - g.fog.x), 0.0, 1.0);
    return vec4<f32>(i.color.rgb * (1.0 - f * 0.85), i.color.a * a);
}
