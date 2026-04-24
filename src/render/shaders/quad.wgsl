struct VsOut {
  @builtin(position) clip_position: vec4<f32>,
  @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(@location(0) in_position: vec2<f32>) -> VsOut {
  var out: VsOut;
  out.clip_position = vec4<f32>(in_position, 0.0, 1.0);
  out.color = vec4<f32>(0.92, 0.92, 0.94, 1.0);
  return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
  return in.color;
}
