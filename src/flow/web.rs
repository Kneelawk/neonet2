//! Web-Specific Flow implementation.

use crate::{
    controller::{AppController, APP_CONTROLLER},
    flow::{FlowModel, FlowModelInit, FlowSignal, FlowStartError},
};
use async_channel::Sender;
use futures::lock::Mutex;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use wasm_bindgen::{closure::Closure, JsCast};
use wasm_timer::Delay;
use wgpu::{
    Backends, DeviceDescriptor, Instance, Limits, Maintain, PresentMode, RequestAdapterOptions,
    SurfaceConfiguration, TextureFormat, TextureUsages,
};
use winit::{
    dpi::PhysicalSize,
    event::Event,
    event_loop::{ControlFlow, EventLoop},
    platform::web::WindowExtWebSys,
    window::WindowBuilder,
};

/// Used to manage a web application's control flow as well as integration with
/// the canvas and WGPU.
pub struct WebFlow {
    canvas_container_id: String,
    canvas_id: String,
}

impl WebFlow {
    pub fn new() -> WebFlow {
        WebFlow {
            canvas_container_id: "canvas-container".to_string(),
            canvas_id: "canvas".to_string(),
        }
    }

    pub fn canvas_container_id(mut self, id: impl Into<String>) -> Self {
        self.canvas_container_id = id.into();
        self
    }

    pub fn canvas_id(mut self, id: impl Into<String>) -> Self {
        self.canvas_id = id.into();
        self
    }

    pub async fn start<Model: FlowModel + 'static>(self) -> Result<!, FlowStartError> {
        info!("Getting window data...");
        let web_window = web_sys::window().unwrap();
        let window_width = web_window.inner_width().unwrap().as_f64().unwrap() as f32;
        let window_height = web_window.inner_height().unwrap().as_f64().unwrap() as f32;
        let window_size = PhysicalSize::new(window_width, window_height);

        info!("Creating event loop...");
        let event_loop = EventLoop::<FlowSignal>::with_user_event();
        *APP_CONTROLLER.lock().unwrap() = AppController::Proxy(event_loop.create_proxy());

        info!("Creating window...");
        let window = WindowBuilder::new()
            .with_inner_size(window_size)
            .build(&event_loop)?;
        let window = Arc::new(window);

        info!("Setting up canvas...");
        {
            info!("Canvas container id: {}", &self.canvas_container_id);
            info!("Canvas id: {}", &self.canvas_id);

            let canvas = window.canvas();
            let document = web_window.document().unwrap();
            let canvas_container = document
                .get_element_by_id(&self.canvas_container_id)
                .expect("Unable to find canvas container element");
            let canvas_element = web_sys::Element::from(canvas);
            canvas_element.set_id(&self.canvas_id);
            canvas_container.append_child(&canvas_element).unwrap();
        }

        info!("Creating instance...");
        let instance = Arc::new(Instance::new(Backends::all()));

        info!("Creating surface...");
        let surface = Arc::new(unsafe { instance.create_surface(window.as_ref()) });

        info!("Requesting adapter...");
        let adapter = instance
            .request_adapter(&RequestAdapterOptions {
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
                power_preference: Default::default(),
            })
            .await
            .ok_or(FlowStartError::AdapterRequestError)?;

        info!("Requesting device...");
        let (device, queue) = adapter
            .request_device(
                &DeviceDescriptor {
                    label: Some("Device Request"),
                    features: Default::default(),
                    limits: Limits::downlevel_webgl2_defaults(),
                },
                None,
            )
            .await?;
        let device = Arc::new(device);
        let queue = Arc::new(queue);

        info!("Creating device poll task...");
        let status = Arc::new(AtomicBool::new(true));
        let poll_device = device.clone();
        let poll_status = status.clone();
        wasm_bindgen_futures::spawn_local(async move {
            while poll_status.load(Ordering::Acquire) {
                poll_device.poll(Maintain::Poll);
                Delay::new(Duration::from_millis(50)).await.unwrap();
            }
            info!("Poll task completed.");
        });

        info!("Configuring surface...");
        let preferred_format = surface.get_preferred_format(&adapter);
        info!("Preferred render frame format: {:?}", preferred_format);
        let mut config = SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT,
            format: preferred_format.unwrap_or(TextureFormat::Bgra8UnormSrgb),
            width: window_size.width as u32,
            height: window_size.height as u32,
            present_mode: PresentMode::Fifo,
        };

        surface.configure(&device, &config);

        // setup model
        info!("Creating model...");
        let init = FlowModelInit {
            device: device.clone(),
            queue: queue.clone(),
            window_size,
            frame_format: config.format,
        };
        let model: Arc<Mutex<Option<Model>>> = Arc::new(Mutex::new(Some(Model::init(init).await)));

        info!("Starting resize task...");
        let closure = {
            // Automatic resizing
            let model = model.clone();
            let window = window.clone();
            let surface = surface.clone();
            let device = device.clone();
            Closure::wrap(Box::new(move |_e: web_sys::Event| {
                let web_window = web_sys::window().unwrap();
                let size = PhysicalSize::new(
                    web_window.inner_width().unwrap().as_f64().unwrap() as f32,
                    web_window.inner_height().unwrap().as_f64().unwrap() as f32,
                );
                window.set_inner_size(size);

                config.width = size.width as u32;
                config.height = size.height as u32;
                surface.configure(&device, &config);

                let model = model.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    model.lock().await.as_mut().unwrap().resize(size).await;
                });
            }) as Box<dyn FnMut(_)>)
        };

        web_window
            .add_event_listener_with_callback("resize", closure.as_ref().unchecked_ref())
            .unwrap();

        let mut previous_update = now();
        let mut previous_render = now();

        let mut instance = Some(instance);
        let mut adapter = Some(adapter);
        let mut queue = Some(queue);

        let (tx, rx) = async_channel::unbounded();
        let mut tx = Some(tx);

        info!("Launching app task...");
        wasm_bindgen_futures::spawn_local(async move {
            while let Ok(event) = rx.recv().await {
                match event {
                    Event::MainEventsCleared => {
                        let now = now();
                        let delta = now.duration_since(previous_update).unwrap();
                        previous_update = now;

                        model.lock().await.as_mut().unwrap().update(delta).await;
                    },
                    Event::RedrawRequested(_) => {
                        let now = now();
                        let delta = now.duration_since(previous_render).unwrap();
                        previous_render = now;

                        match surface.get_current_texture() {
                            Ok(output) => {
                                let view = output.texture.create_view(&Default::default());

                                model
                                    .lock()
                                    .await
                                    .as_mut()
                                    .unwrap()
                                    .render(&view, delta)
                                    .await;
                            },
                            Err(err) => {
                                error!("Error getting texture: {:?}", err);
                            },
                        }
                    },
                    Event::LoopDestroyed => {
                        info!("Shutting down...");

                        model.lock().await.take().unwrap().shutdown().await;

                        status.store(false, Ordering::Release);

                        // shutdown WGPU
                        drop(queue.take());
                        drop(adapter.take());
                        drop(instance.take());

                        info!("Removing canvas: {}...", &self.canvas_id);
                        let web_window = web_sys::window().unwrap();
                        let document = web_window.document().unwrap();
                        let canvas = document.get_element_by_id(&self.canvas_id).expect("Unable to find canvas element when shutting down. The canvas element has either already been removed or will not be removed.");
                        canvas.remove();

                        info!("Done.");
                    },
                    _ => {},
                }
            }
        });

        info!("Starting event loop...");
        event_loop.run(move |event, _, control_flow| match event {
            Event::UserEvent(user_event) => {
                match user_event {
                    FlowSignal::Exit => {
                        *control_flow = ControlFlow::Exit;
                    },
                }
                send(&tx, event);
            },
            Event::MainEventsCleared => {
                send(&tx, event);
                window.request_redraw();
            },
            Event::LoopDestroyed => {
                drop(tx.take());
                send(&tx, event);
            },
            _ => {
                send(&tx, event);
            },
        });
    }
}

fn send(tx: &Option<Sender<Event<FlowSignal>>>, event: Event<FlowSignal>) {
    if let Some(tx) = tx.as_ref() {
        // The only event lost here is the re-scale event, but we handle those already.
        if let Some(event) = event.to_static() {
            // channel should never be full
            tx.try_send(event)
                .expect("Error sending event to app thread.");
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn now() -> SystemTime {
    let performance = web_sys::window().unwrap().performance().unwrap();
    let amt = performance.now();
    let secs = (amt as u64) / 1_000;
    let nanos = ((amt as u32) % 1_000) * 1_000_000;
    UNIX_EPOCH + Duration::new(secs, nanos)
}
