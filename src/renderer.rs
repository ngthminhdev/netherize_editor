use std::sync::Arc;
use winit::window::Window;

pub struct Renderer<'a> {
    // Surface: Đầu ra gắn với Window (Giống như Websocket connection xuống Client)
    surface: wgpu::Surface<'a>,
    // Device: Kênh giao tiếp với GPU (Giống như Database Connection)
    device: wgpu::Device,
    // Queue: Nơi chứa các lệnh đẩy xuống GPU (Giống như Kafka/RabbitMQ)
    queue: wgpu::Queue,
    // Config: Cấu hình độ phân giải, VSync
    config: wgpu::SurfaceConfiguration,
    pub size: winit::dpi::PhysicalSize<u32>,
}

#[derive(Debug)]
pub enum RenderError {
    Timeout,
    Occluded,
    Outdated,
    Lost,
    Validation,
}

impl<'a> Renderer<'a> {
    // Hàm khởi tạo này là Async vì nó phải gọi xuống Driver của hệ điều hành
    pub async fn new(window: Arc<Window>) -> Self {
        let size = window.inner_size();

        // 1. Instance: Khởi tạo Backend. Trên máy bạn nó sẽ tự chọn Metal.
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });

        // 2. Surface: Xin OS một "mặt phẳng" trên Window để GPU vẽ vào
        let surface = instance.create_surface(window).unwrap();

        // 3. Adapter: Tìm card đồ họa (GPU) vật lý
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance, // Ép dùng GPU mạnh nhất
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .unwrap();

        // 4. Device & Queue: Mở kết nối chính thức tới GPU
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                label: Some("Netherize Device"),
                experimental_features: Default::default(),
                memory_hints: Default::default(),
                trace: Default::default(),
            })
            .await
            .unwrap();

        // 5. Cấu hình Surface (Độ phân giải, Format màu)
        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(surface_caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::Fifo, // Fifo = Bật VSync (tránh xé hình, lock fps theo màn hình)
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };

        surface.configure(&device, &config);

        Self {
            surface,
            device,
            queue,
            config,
            size,
        }
    }

    // Hàm gọi khi User resize cửa sổ
    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
        }
    }

    // Hàm Render cốt lõi - Gọi mỗi khi màn hình cần vẽ lại (120 lần/s)
    pub fn render(&mut self, clear_color: wgpu::Color) -> Result<(), RenderError> {
        // 1. Lấy ra cái frame (bức tranh) hiện tại từ Surface
        let output = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(output) => output,
            wgpu::CurrentSurfaceTexture::Suboptimal(output) => output,
            wgpu::CurrentSurfaceTexture::Timeout => return Err(RenderError::Timeout),
            wgpu::CurrentSurfaceTexture::Occluded => return Err(RenderError::Occluded),
            wgpu::CurrentSurfaceTexture::Outdated => return Err(RenderError::Outdated),
            wgpu::CurrentSurfaceTexture::Lost => return Err(RenderError::Lost),
            wgpu::CurrentSurfaceTexture::Validation => return Err(RenderError::Validation),
        };
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        // 2. Tạo một Command Encoder (Người đóng gói lệnh)
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        // 3. Bắt đầu Render Pass (Tô màu toàn bộ màn hình)
        {
            let _render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Clear Screen Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear_color), // Tô màu nền
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            // Ngoặc nhọn {} ở đây rất quan trọng trong Rust!
            // Khi block kết thúc, `_render_pass` bị drop (giải phóng), trả lại quyền mượn (borrow) cho `encoder`.
        }

        // 4. Submit lệnh xuống Queue và yêu cầu GPU hiện lên màn hình
        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }
}
