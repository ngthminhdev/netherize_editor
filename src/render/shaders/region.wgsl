struct ScreenUniform {
    screen_size: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> u_screen: ScreenUniform;

struct VsIn {
    @location(0) local_pos: vec2<f32>, // 0..1 unit quad
    @location(1) rect: vec4<f32>,      // x,y,width,height in pixels
    @location(2) color: vec4<f32>,
};

struct VsOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(in: VsIn) -> VsOut {
    let pixel_pos = in.rect.xy + in.local_pos * in.rect.zw;
    let screen = u_screen.screen_size.xy;

    // Pixel -> NDC (origin top-left, y xuống dưới).
    let ndc_x = (pixel_pos.x / screen.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (pixel_pos.y / screen.y) * 2.0;

    var out: VsOut;
    out.clip_pos = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.color = in.color;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return in.color;
}
