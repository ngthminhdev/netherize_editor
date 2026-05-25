struct VsIn {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
};

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_main(in: VsIn) -> VsOut {
    var out: VsOut;
    out.pos = vec4<f32>(in.position, 0.0, 1.0);
    out.uv = in.uv;
    out.color = in.color;
    return out;
}

@group(0) @binding(0) var icon_tex: texture_2d<f32>;
@group(0) @binding(1) var icon_sampler: sampler;

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let px = textureSample(icon_tex, icon_sampler, in.uv);
    return vec4<f32>(px.rgb * in.color.rgb, px.a * in.color.a);
}
