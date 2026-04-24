pub const STANDARD_ALPHA_BLENDING: wgpu::BlendState = wgpu::BlendState {
    color: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::SrcAlpha,
        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
        operation: wgpu::BlendOperation::Add,
    },
    alpha: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::One,
        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
        operation: wgpu::BlendOperation::Add,
    },
};

pub fn srgb_color_target_state(format: wgpu::TextureFormat) -> wgpu::ColorTargetState {
    debug_assert!(
        format.is_srgb(),
        "render targets must be sRGB so the GPU performs the final gamma encode"
    );

    wgpu::ColorTargetState {
        format,
        blend: Some(STANDARD_ALPHA_BLENDING),
        write_mask: wgpu::ColorWrites::ALL,
    }
}
