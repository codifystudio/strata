//! Strata window — окно приложения на winit + softbuffer.
//!
//! Открывает окно, поддерживает resize, и предоставляет функцию present()
//! для отображения RGBA-буфера на экране.

use std::num::NonZeroU32;
use std::sync::Arc;

use strata_core::{StrataError, StrataResult};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

/// Surface для рисования пикселей в окно.
pub struct Surface {
    context: softbuffer::Context<Arc<Window>>,
    surface: softbuffer::Surface<Arc<Window>, Arc<Window>>,
}

impl Surface {
    pub fn new(window: Arc<Window>) -> StrataResult<Self> {
        let context = softbuffer::Context::new(window.clone())
            .map_err(|e| StrataError::Window(e.to_string()))?;
        let surface = softbuffer::Surface::new(&context, window.clone())
            .map_err(|e| StrataError::Window(e.to_string()))?;
        Ok(Self { context, surface })
    }

    /// Презентация RGBA пикселей в окне.
    /// `rgba`: вектор длиной width*height*4 байт.
    pub fn present(&mut self, width: u32, height: u32, rgba: &[u8]) -> StrataResult<()> {
        if rgba.len() != (width as usize) * (height as usize) * 4 {
            return Err(StrataError::Window(format!(
                "buffer size mismatch: expected {}, got {}",
                width * height * 4,
                rgba.len()
            )));
        }

        let (w, h) = {
            let size = self
                .surface
                .window()
                .inner_size();
            (size.width, size.height)
        };

        self.surface
            .resize(
                NonZeroU32::new(w).ok_or_else(|| StrataError::Window("zero width".into()))?,
                NonZeroU32::new(h).ok_or_else(|| StrataError::Window("zero height".into()))?,
            )
            .map_err(|e| StrataError::Window(e.to_string()))?;

        let mut buffer = self
            .surface
            .buffer_mut()
            .map_err(|e| StrataError::Window(e.to_string()))?;

        // Скопировать RGBA → XRGB8888, масштабируя до размеров окна (nearest).
        let scale_x = width as f32 / w as f32;
        let scale_y = height as f32 / h as f32;

        for y in 0..h {
            let src_y = (y as f32 / scale_y) as usize;
            let src_y = src_y.min(height as usize - 1);
            for x in 0..w {
                let src_x = (x as f32 / scale_x) as usize;
                let src_x = src_x.min(width as usize - 1);
                let src_idx = (src_y * width as usize + src_x) * 4;
                let r = rgba[src_idx];
                let g = rgba[src_idx + 1];
                let b = rgba[src_idx + 2];
                let dst_idx = (y * w + x) as usize;
                buffer[dst_idx] = 0xFF00_0000u32 | ((b as u32) << 16) | ((g as u32) << 8) | (r as u32);
            }
        }

        buffer
            .present()
            .map_err(|e| StrataError::Window(e.to_string()))?;
        Ok(())
    }
}

/// Запрос, который ядро передаёт окну: перерисовать кадр.
pub type RenderRequest = Box<dyn FnMut(u32, u32) -> Vec<u8> + Send>;

/// Простой window handler. Запускает event loop, по Resumed/RedrawRequested
/// вызывает колбэк рендера и презентует пиксели.
pub struct WindowApp {
    title: String,
    width: u32,
    height: u32,
    window: Option<Arc<Window>>,
    surface: Option<Surface>,
    renderer: Option<RenderRequest>,
}

impl WindowApp {
    pub fn new(title: String, width: u32, height: u32) -> Self {
        Self {
            title,
            width,
            height,
            window: None,
            surface: None,
            renderer: None,
        }
    }

    pub fn set_renderer(&mut self, r: RenderRequest) {
        self.renderer = Some(r);
    }

    pub fn run(self) -> StrataResult<()> {
        let event_loop = EventLoop::new().map_err(|e| StrataError::Window(e.to_string()))?;
        let mut app = self;
        event_loop
            .run_app(&mut app)
            .map_err(|e| StrataError::Window(e.to_string()))?;
        Ok(())
    }
}

impl ApplicationHandler for WindowApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = event_loop
            .create_window(
                winit::window::WindowAttributes::new()
                    .with_title(&self.title)
                    .with_inner_size(LogicalSize::new(self.width, self.height))
                    .with_resizable(true),
            )
            .expect("create window");
        let window = Arc::new(window);
        let surface = Surface::new(window.clone()).expect("create surface");
        self.window = Some(window);
        self.surface = Some(surface);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(PhysicalSize { width, height }) => {
                self.width = width.max(1);
                self.height = height.max(1);
            }
            WindowEvent::RedrawRequested => {
                if let (Some(surface), Some(renderer)) = (&mut self.surface, &mut self.renderer) {
                    let w = self.width;
                    let h = self.height;
                    let rgba = renderer(w, h);
                    if let Err(e) = surface.present(w, h, &rgba) {
                        log::error!("present failed: {}", e);
                    }
                }
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        state: ElementState::Pressed,
                        logical_key: key,
                        ..
                    },
                ..
            } => {
                if key == Key::Named(NamedKey::Escape) {
                    event_loop.exit();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}
