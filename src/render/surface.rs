use winit::dpi::PhysicalSize;

/// SurfaceState giữ toàn bộ thông tin liên quan đến swapchain/surface.
///
/// Mục tiêu của phase 2 là tách phần "trình bày frame lên cửa sổ"
/// ra khỏi logic pipeline để dễ debug từng lớp.
pub struct SurfaceState {
    pub surface: wgpu::Surface<'static>,
    pub config: wgpu::SurfaceConfiguration,
    pub size: PhysicalSize<u32>,
}

impl SurfaceState {
    pub fn new(
        surface: wgpu::Surface<'static>,
        size: PhysicalSize<u32>,
        adapter: &wgpu::Adapter,
        device: &wgpu::Device,
    ) -> Result<Self, String> {
        let capabilities = surface.get_capabilities(adapter);

        // Bắt buộc sRGB để GPU blend trên linear domain rồi encode đúng ra màn hình.
        let surface_format = capabilities
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .ok_or_else(|| {
                format!(
                    "surface has no supported sRGB texture format (supported: {:?})",
                    capabilities.formats
                )
            })?;

        // Ưu tiên FIFO (vsync ổn định), fallback nếu platform không hỗ trợ.
        let present_mode = if capabilities
            .present_modes
            .contains(&wgpu::PresentMode::Fifo)
        {
            wgpu::PresentMode::Fifo
        } else {
            *capabilities
                .present_modes
                .first()
                .ok_or_else(|| "surface has no present mode".to_string())?
        };

        let alpha_mode = *capabilities
            .alpha_modes
            .first()
            .ok_or_else(|| "surface has no alpha mode".to_string())?;

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode,
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };

        surface.configure(device, &config);

        Ok(Self {
            surface,
            config,
            size,
        })
    }

    /// Resize an toàn: chỉ configure lại khi kích thước hợp lệ.
    pub fn resize(&mut self, device: &wgpu::Device, new_size: PhysicalSize<u32>) {
        if new_size.width == 0 || new_size.height == 0 {
            return;
        }

        self.size = new_size;
        self.config.width = new_size.width;
        self.config.height = new_size.height;
        self.surface.configure(device, &self.config);
    }

    /// Dùng khi surface bị lost/outdated nhưng kích thước không đổi.
    pub fn reconfigure(&self, device: &wgpu::Device) {
        self.surface.configure(device, &self.config);
    }
}
