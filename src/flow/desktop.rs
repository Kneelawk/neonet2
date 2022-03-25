//! Desktop-Specific Flow implementation.

use crate::{
    controller::{AppController, APP_CONTROLLER},
    flow::{FlowModel, FlowModelInit, FlowSignal, FlowStartError},
};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, SystemTime},
};
use tokio::{runtime, time::sleep};
use wgpu::{
    Backends, DeviceDescriptor, Instance, Maintain, PresentMode, RequestAdapterOptions,
    SurfaceConfiguration, SurfaceError, TextureFormat, TextureUsages, TextureViewDescriptor,
};
use winit::{
    dpi::PhysicalSize,
    event::{Event, KeyboardInput, VirtualKeyCode, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::{Fullscreen, WindowBuilder},
};

/// Used to manage an application's control flow as well as integration with the
/// window manager. This version of `Flow` and `FlowModel` are designed to
/// support an asynchronous application.
pub struct DesktopFlow {
    /// The window's title.
    pub title: String,
    /// Whether the window should be fullscreen.
    pub fullscreen: bool,
    /// The window's width if not fullscreen.
    pub width: u32,
    /// The window's height if not fullscreen.
    pub height: u32,
}

impl DesktopFlow {
    /// Creates a new Flow designed to handle a specific kind of model.
    ///
    /// This model is instantiated when the Flow is started.
    pub fn new() -> DesktopFlow {
        DesktopFlow {
            title: "".to_string(),
            fullscreen: false,
            width: 1280,
            height: 720,
        }
    }

    /// Sets this Flow's window title.
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    /// Sets whether this Flow's window is fullscreen.
    pub fn fullscreen(mut self, fullscreen: bool) -> Self {
        self.fullscreen = fullscreen;
        self
    }

    /// Sets this Flow's window's width.
    pub fn width(mut self, width: u32) -> Self {
        self.width = width;
        self
    }

    /// Sets this Flow's window's height.
    pub fn height(mut self, height: u32) -> Self {
        self.height = height;
        self
    }

    /// Starts the Flow's event loop.
    pub fn start<Model: FlowModel + 'static>(self) -> Result<!, FlowStartError> {
        info!("Creating runtime...");
        let runtime = runtime::Builder::new_multi_thread().enable_all().build()?;

        info!("Creating event loop...");
        let event_loop = EventLoop::<FlowSignal>::with_user_event();
        *APP_CONTROLLER.lock().unwrap() = AppController::Proxy(event_loop.create_proxy());

        info!("Creating window...");
        let window = {
            let mut builder = WindowBuilder::new()
                .with_title(self.title.clone())
                .with_inner_size(PhysicalSize::new(self.width, self.height));

            builder = if self.fullscreen {
                builder.with_fullscreen(Some(Fullscreen::Borderless(None)))
            } else {
                builder
            };

            builder.build(&event_loop)?
        };

        let window = Arc::new(window);
        let window_size = window.inner_size();

        // setup wgpu
        info!("Creating instance...");
        let instance = Arc::new(Instance::new(Backends::PRIMARY));

        info!("Creating surface...");
        let surface = unsafe { instance.create_surface(window.as_ref()) };

        info!("Requesting adapter...");
        let adapter = runtime
            .block_on(instance.request_adapter(&RequestAdapterOptions {
                power_preference: Default::default(),
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
            }))
            .ok_or(FlowStartError::AdapterRequestError)?;

        info!("Requesting device...");
        let (device, queue) = runtime.block_on(adapter.request_device(
            &DeviceDescriptor {
                label: Some("Device"),
                limits: Default::default(),
                features: Default::default(),
            },
            None,
        ))?;

        let device = Arc::new(device);
        let queue = Arc::new(queue);

        info!("Creating device poll task");
        let poll_device = device.clone();
        let status = Arc::new(AtomicBool::new(true));
        let poll_status = status.clone();
        let mut poll_task = Some(runtime.spawn(async move {
            while poll_status.load(Ordering::Acquire) {
                poll_device.poll(Maintain::Poll);
                sleep(Duration::from_millis(5)).await;
            }
        }));

        info!("Configuring surface...");
        let preferred_format = surface.get_preferred_format(&adapter);
        info!("Preferred render frame format: {:?}", preferred_format);
        let mut config = SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT,
            format: preferred_format.unwrap_or(TextureFormat::Bgra8UnormSrgb),
            width: window_size.width,
            height: window_size.height,
            present_mode: PresentMode::Fifo,
        };

        surface.configure(&device, &config);

        // setup model
        info!("Creating model...");
        let init = FlowModelInit {
            device: device.clone(),
            queue: queue.clone(),
            window_size: window_size.into_f32_size(),
            frame_format: config.format,
        };
        let mut model: Option<Model> = Some(runtime.block_on(Model::init(init)));
        let mut previous_update = SystemTime::now();
        let mut previous_render = SystemTime::now();

        let mut runtime = Some(runtime);

        let mut instance = Some(instance);
        let mut adapter = Some(adapter);
        let mut queue = Some(queue);

        info!("Starting event loop...");
        event_loop.run(move |event, _, control| {
            match &event {
                Event::WindowEvent { event, window_id } if *window_id == window.id() => match event
                {
                    WindowEvent::Resized(size) => {
                        config.width = size.width;
                        config.height = size.height;
                        surface.configure(&device, &config);
                        runtime
                            .as_ref()
                            .unwrap()
                            .block_on(model.as_mut().unwrap().resize(size.into_f32_size()));
                    },
                    WindowEvent::ScaleFactorChanged { ref new_inner_size, .. } => {
                        config.width = new_inner_size.width;
                        config.height = new_inner_size.height;
                        surface.configure(&device, &config);
                        runtime.as_ref().unwrap().block_on(
                            model
                                .as_mut()
                                .unwrap()
                                .resize(new_inner_size.into_f32_size()),
                        );
                    },
                    WindowEvent::CloseRequested
                    | WindowEvent::KeyboardInput {
                        input:
                            KeyboardInput {
                                virtual_keycode: Some(VirtualKeyCode::Escape),
                                ..
                            },
                        ..
                    } => {
                        *control = ControlFlow::Exit;
                    },
                    _ => {},
                },
                Event::MainEventsCleared => {
                    let now = SystemTime::now();
                    let delta = now.duration_since(previous_update).unwrap();
                    previous_update = now;

                    runtime
                        .as_ref()
                        .unwrap()
                        .block_on(model.as_mut().unwrap().update(delta));
                    window.request_redraw();
                },
                Event::UserEvent(signal) => match signal {
                    FlowSignal::Exit => *control = ControlFlow::Exit,
                },
                Event::RedrawRequested(window_id) if *window_id == window.id() => {
                    let now = SystemTime::now();
                    let delta = now.duration_since(previous_render).unwrap();
                    previous_render = now;

                    let frame = match surface.get_current_texture() {
                        Ok(output) => Some(output),
                        Err(SurfaceError::OutOfMemory) => {
                            error!("Unable to obtain surface frame: OutOfMemory! Exiting...");
                            *control = ControlFlow::Exit;

                            None
                        },
                        Err(_) => None,
                    };

                    if let Some(frame) = frame {
                        let view = frame.texture.create_view(&TextureViewDescriptor::default());

                        model.as_mut().unwrap().render(&view, delta);

                        frame.present();
                    }
                },
                Event::LoopDestroyed => {
                    info!("Shutting down...");

                    let runtime = runtime.take().unwrap();

                    let mut model = model.take().unwrap();
                    model.shutdown();

                    status.store(false, Ordering::Release);
                    if let Err(e) = runtime.block_on(poll_task.take().unwrap()) {
                        error!("Error stopping device poll task: {:?}", e);
                    }

                    // shutdown WGPU
                    drop(queue.take());
                    drop(adapter.take());
                    drop(instance.take());

                    // shutdown the runtime
                    drop(runtime);

                    info!("Done.");
                },
                _ => {},
            }
        });
    }
}

trait IntoF32Size {
    fn into_f32_size(self) -> PhysicalSize<f32>;
}

impl IntoF32Size for PhysicalSize<u32> {
    fn into_f32_size(self) -> PhysicalSize<f32> {
        PhysicalSize::new(self.width as f32, self.height as f32)
    }
}
