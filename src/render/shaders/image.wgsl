struct VsIn { @location(0) position: vec2<f32>, @location(1) uv: vec2<f32> };
struct VsOut { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32> };
@vertex fn vs_main(in: VsIn) -> VsOut { var out: VsOut; out.pos = vec4<f32>(in.position, 0.0, 1.0); out.uv = in.uv; return out; }
@group(0) @binding(0) var image_tex: texture_2d<f32>;
@group(0) @binding(1) var image_sampler: sampler;
@fragment fn fs_main(in: VsOut) -> @location(0) vec4<f32> { return textureSample(image_tex, image_sampler, in.uv); }
