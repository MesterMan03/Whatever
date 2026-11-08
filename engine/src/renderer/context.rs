use anyhow::{Context, bail};
use std::{
    sync::{Arc, mpsc},
    time::Instant,
};
use winit::window::Window;

pub struct WgpuContext {
    pub adapter_name: String,
    pub surface: Arc<wgpu::Surface<'static>>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    window: Arc<Window>,
}

pub(crate) struct AcquiredFrame {
    pub output: wgpu::SurfaceTexture,
    pub acquire_ms: f64,
}

type Acquisition = Result<AcquiredFrame, wgpu::SurfaceError>;

fn acquire(surface: &wgpu::Surface<'_>) -> Acquisition {
    let start = Instant::now();
    let output = surface.get_current_texture()?;
    Ok(AcquiredFrame {
        output,
        acquire_ms: start.elapsed().as_secs_f64() * 1000.0,
    })
}

/// At most one outstanding acquire. The window thread owns rendering and surface configuration.
pub(crate) struct FrameAcquirer {
    request: mpsc::Sender<()>,
    response: mpsc::Receiver<Acquisition>,
    pub pending: bool,
}

impl FrameAcquirer {
    pub fn spawn(ctx: &WgpuContext) -> anyhow::Result<Self> {
        let (request, requests) = mpsc::channel();
        let (responses, response) = mpsc::channel();
        let surface = Arc::clone(&ctx.surface);
        let window = Arc::clone(&ctx.window);
        std::thread::Builder::new()
            .name("whatever-frame-acquire".into())
            .spawn(move || {
                while requests.recv().is_ok() {
                    if responses.send(acquire(&surface)).is_err() {
                        break;
                    }
                    window.request_redraw();
                }
            })?;
        Ok(Self {
            request,
            response,
            pending: false,
        })
    }

    pub fn request(&mut self) -> anyhow::Result<()> {
        debug_assert!(!self.pending);
        self.request.send(()).context("request a surface frame")?;
        self.pending = true;
        Ok(())
    }

    pub fn take_ready(&mut self) -> anyhow::Result<Option<Acquisition>> {
        match self.response.try_recv() {
            Ok(result) => {
                self.pending = false;
                Ok(Some(result))
            }
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(error) => Err(error).context("receive a surface frame"),
        }
    }
}

impl WgpuContext {
    pub async fn new(window: Arc<Window>) -> anyhow::Result<Self> {
        let size = window.inner_size();

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            // PRIMARY = Vulkan + Metal + DX12; skip GL/GLES to avoid libEGL noise on Linux
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });

        let surface = Arc::new(
            instance
                .create_surface(Arc::clone(&window))
                .context("create surface")?,
        );

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await;

        let Some(adapter) = adapter else {
            bail!("no suitable GPU adapter found");
        };

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("Whatever device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: Default::default(),
                },
                None,
            )
            .await
            .context("request device")?;

        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 1,
        };
        surface.configure(&device, &config);

        Ok(WgpuContext {
            adapter_name: format!(
                "{} ({:?})",
                adapter.get_info().name,
                adapter.get_info().backend
            ),
            surface,
            device,
            queue,
            config,
            window,
        })
    }

    pub(crate) fn acquire_frame(&self) -> anyhow::Result<Option<AcquiredFrame>> {
        self.resolve_frame(acquire(&self.surface))
    }

    pub(crate) fn resolve_frame(
        &self,
        result: Acquisition,
    ) -> anyhow::Result<Option<AcquiredFrame>> {
        match result {
            Ok(frame) => Ok(Some(frame)),
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.config);
                Ok(None)
            }
            Err(wgpu::SurfaceError::Timeout) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    pub(crate) fn pre_present_notify(&self) {
        self.window.pre_present_notify();
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
    }

    pub fn set_vsync(&mut self, enabled: bool) {
        self.set_present_mode(if enabled {
            wgpu::PresentMode::Fifo
        } else {
            wgpu::PresentMode::AutoNoVsync
        });
    }

    pub fn set_present_mode(&mut self, mode: wgpu::PresentMode) {
        let latency = match mode {
            wgpu::PresentMode::AutoVsync
            | wgpu::PresentMode::Fifo
            | wgpu::PresentMode::FifoRelaxed => 1,
            _ => 2,
        };
        if self.config.present_mode == mode && self.config.desired_maximum_frame_latency == latency
        {
            return;
        }
        self.config.present_mode = mode;
        self.config.desired_maximum_frame_latency = latency;
        self.surface.configure(&self.device, &self.config);
    }
}
