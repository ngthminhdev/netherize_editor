struct ScreenUniform {
  screen_size: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> screen: ScreenUniform;

struct VsIn {
  @location(0) local_pos: vec2<f32>,
  @location(1) screen_pos: vec2<f32>,
  @location(2) caret_size: vec2<f32>,
  @location(3) color: vec4<f32>,
};

struct VsOut {
  @builtin(position) clip_position: vec4<f32>,
  @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(input: VsIn) -> VsOut {
  var out: VsOut;
  let pixel_pos = input.screen_pos + input.local_pos * input.caret_size;
  let ndc_x = (pixel_pos.x / screen.screen_size.x) * 2.0 - 1.0;
  let ndc_y = 1.0 - (pixel_pos.y / screen.screen_size.y) * 2.0;

  out.clip_position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
  out.color = input.color;
  return out;
}

@fragment
fn fs_main(input: VsOut) -> @location(0) vec4<f32> {
  return input.color;
}
