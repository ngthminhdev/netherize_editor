struct ScreenUniform {
    screen_size: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> u_screen: ScreenUniform;

struct VsIn {
    @location(0) local_pos: vec2<f32>, // 0..1 unit quad
    @location(1) rect: vec4<f32>,      // x,y,width,height in pixels
    @location(2) color: vec4<f32>,
    @location(3) corner_radii: vec4<f32>, // [top-left, top-right, bottom-right, bottom-left]
};

struct VsOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) local_px: vec2<f32>,
    @location(2) size: vec2<f32>,
    @location(3) corner_radii: vec4<f32>,
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
    out.local_px = in.local_pos * in.rect.zw;
    out.size = in.rect.zw;
    out.corner_radii = in.corner_radii;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // Centered coords; y grows downward (origin top-left), so p.y >= 0 is the
    // bottom half and p.x >= 0 the right half.
    let p = in.local_px - in.size * 0.5;

    // Pick this fragment's nearest corner radius:
    //   corner_radii = [top-left, top-right, bottom-right, bottom-left]
    var r: f32;
    if p.x >= 0.0 {
        // right side: top -> TR (.y), bottom -> BR (.z)
        r = select(in.corner_radii.y, in.corner_radii.z, p.y >= 0.0);
    } else {
        // left side: top -> TL (.x), bottom -> BL (.w)
        r = select(in.corner_radii.x, in.corner_radii.w, p.y >= 0.0);
    }
    let radius = clamp(r, 0.0, min(in.size.x, in.size.y) * 0.5);
    if radius <= 0.0 {
        return in.color;
    }

    let b = in.size * 0.5 - vec2<f32>(radius, radius);
    let q = abs(p) - b;
    let dist = length(max(q, vec2<f32>(0.0, 0.0))) + min(max(q.x, q.y), 0.0) - radius;
    let aa = 1.0;
    let alpha = 1.0 - smoothstep(0.0, aa, dist);
    return vec4<f32>(in.color.rgb, in.color.a * alpha);
}
