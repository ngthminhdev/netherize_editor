struct ScreenUniform {
  screen_size: vec4<f32>,
};

@group(0) @binding(0)
var atlas_tex: texture_2d<f32>;

@group(0) @binding(1)
var atlas_sampler: sampler;

@group(0) @binding(2)
var<uniform> screen: ScreenUniform;

struct VsIn {
  @location(0) local_pos: vec2<f32>,
  @location(1) screen_pos: vec2<f32>,
  @location(2) glyph_size: vec2<f32>,
  @location(3) uv_min: vec2<f32>,
  @location(4) uv_max: vec2<f32>,
  @location(5) color: vec4<f32>,
};

struct VsOut {
  @builtin(position) clip_position: vec4<f32>,
  @location(0) uv: vec2<f32>,
  @location(1) color: vec4<f32>,
};

@vertex
fn vs_main(input: VsIn) -> VsOut {
  var out: VsOut;

  // local_pos (0..1) -> pixel position trên màn hình.
  let pixel_pos = input.screen_pos + input.local_pos * input.glyph_size;

  // Convert pixel (origin top-left) sang clip space (-1..1).
  let ndc_x = (pixel_pos.x / screen.screen_size.x) * 2.0 - 1.0;
  let ndc_y = 1.0 - (pixel_pos.y / screen.screen_size.y) * 2.0;
  out.clip_position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);

  // Lerp UV theo local_pos.
  out.uv = input.uv_min + (input.uv_max - input.uv_min) * input.local_pos;
  out.color = input.color;
  return out;
}

@fragment
fn fs_main(input: VsOut) -> @location(0) vec4<f32> {
  let sample_alpha = textureSample(atlas_tex, atlas_sampler, input.uv).r;
  return vec4<f32>(input.color.rgb, input.color.a * sample_alpha);
}
