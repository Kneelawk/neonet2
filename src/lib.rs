#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;
#[macro_use]
extern crate thiserror;

mod buffer;
mod grid;
mod neonet;
mod timer;
mod util;

use crate::{
    buffer::BufferWrapper,
    grid::{Grid, Positioned},
    neonet::Model,
    timer::Timer,
    util::least_power_of_2_greater,
};
use std::{
    future::Future,
    process::Output,
    sync::{Arc, Mutex},
    time::SystemTime,
};
use wgpu::{
    Backends, DeviceDescriptor, Instance, Limits, PresentMode, RequestAdapterOptions,
    SurfaceConfiguration, TextureUsages,
};
use winit::{
    dpi::PhysicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::{Window, WindowBuilder},
};

lazy_static! {
    static ref APP_CONTROLLER: Arc<Mutex<AppController>> =
        Arc::new(Mutex::new(AppController::None));
    #[cfg(not(target_arch = "wasm32"))]
    static ref RUNTIME: tokio::runtime::Runtime = tokio::runtime::Runtime::new().unwrap();
}

#[derive(Debug)]
enum UserEvent {
    Shutdown,
}

enum AppController {
    #[cfg(target_arch = "wasm32")]
    Proxy(EventLoopProxy<UserEvent>),
    None,
}

// Force `EventLoopProxy` to be `Send` even in WASM contexts, because WASM is single-threaded.
unsafe impl Send for AppController {}

impl AppController {
    pub fn shutdown(&self) {
        match self {
            #[cfg(target_arch = "wasm32")]
            AppController::Proxy(proxy) => {
                proxy.send_event(UserEvent::Shutdown).unwrap();
            }
            AppController::None => {}
        }
    }
}

// When the `wee_alloc` feature is enabled, this uses `wee_alloc` as the global
// allocator.
//
// If you don't want to use `wee_alloc`, you can safely delete this.
#[cfg(feature = "wee_alloc")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

// This is like the `main` function, except for JavaScript.
#[cfg(target_arch = "wasm32")]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen(start))]
pub fn main_js() -> Result<(), JsValue> {
    wasm_logger::init(Default::default());

    // This provides better error messages in debug mode.
    // It's disabled in release mode so it doesn't bloat up the file size.
    #[cfg(all(debug_assertions, target_arch = "wasm32"))]
    console_error_panic_hook::set_once();

    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn block_on<F>(future: F)
where
    F: Future<Output = ()>,
{
    wasm_bindgen_futures::spawn_local(future);
}

#[cfg(not(target_arch = "wasm32"))]
fn block_on<F>(future: F)
where
    F: Future<Output = ()>,
{
    RUNTIME.block_on(future);
}

#[cfg(target_arch = "wasm32")]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub async fn start_neonet(canvas_container_id: String, canvas_id: String) {
    let window = web_sys::window().unwrap();
    let window_width = window.inner_width().unwrap().as_f64().unwrap() as f32;
    let window_height = window.inner_height().unwrap().as_f64().unwrap() as f32;

    // Setup window
    let event_loop = EventLoop::<UserEvent>::with_user_event();
    *APP_CONTROLLER.lock().unwrap() = AppController::Proxy(event_loop.create_proxy());
    let window = WindowBuilder::new()
        .with_inner_size(PhysicalSize::new(window_width, window_height))
        .build(&event_loop)
        .unwrap();
    let window = Arc::new(window);

    // Setup canvas stuff
    {
        use winit::platform::web::WindowExtWebSys;
        info!("Canvas Container ID: {}", &canvas_container_id);

        let canvas = window.canvas();
        let window = web_sys::window().unwrap();
        let document = window.document().unwrap();
        let canvas_container = document
            .get_element_by_id(&canvas_container_id)
            .expect("Unable to find Canvas Container Element!");
        let element = web_sys::Element::from(canvas);
        element.set_id(&canvas_id);
        canvas_container.append_child(&element).unwrap();
    }

    init_impl(event_loop, window, window_width, window_height).await;
}

#[cfg(not(target_arch = "wasm32"))]
pub fn start_neonet_bin() {
    let event_loop = EventLoop::<UserEvent>::with_user_event();
    let window = Arc::new(
        WindowBuilder::new()
            .with_inner_size(PhysicalSize::new(1280u32, 720))
            .with_title("NeoNet 2")
            .build(&event_loop)
            .unwrap(),
    );

    init_impl(event_loop, window, 1280.0, 720.0);
}

fn init_impl(
    event_loop: EventLoop<UserEvent>,
    window: Arc<Window>,
    window_width: f32,
    window_height: f32,
) {
    info!("Initializing NeoNet...");

    #[cfg(not(target_arch = "wasm32"))]
    let handle = tokio::runtime::Handle::current();

    // Setup wgpu stuff
    let instance = Instance::new(Backends::all());
    let surface = Arc::new(unsafe { instance.create_surface(window.as_ref()) });

    let adapter = block_on(instance
        .request_adapter(&RequestAdapterOptions {
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
            power_preference: Default::default(),
        }))
        .unwrap();

    let (device, queue) = adapter
        .request_device(
            &DeviceDescriptor {
                limits: Limits::downlevel_webgl2_defaults(),
                features: Default::default(),
                label: Some("Device"),
            },
            None,
        )
        .await
        .unwrap();
    let device = Arc::new(device);
    let queue = Arc::new(queue);

    let preferred_format = surface.get_preferred_format(&adapter).unwrap();
    let mut surface_config = SurfaceConfiguration {
        width: window_width as u32,
        height: window_height as u32,
        format: preferred_format,
        usage: TextureUsages::RENDER_ATTACHMENT,
        present_mode: PresentMode::Mailbox,
    };
    surface.configure(&device, &surface_config);

    let model = Arc::new(Mutex::new(Model::new(
        window_width,
        window_height,
        device.clone(),
        &queue,
        preferred_format,
    )));

    #[cfg(target_arch = "wasm32")]
    let closure = {
        // Automatic resizing
        let model = model.clone();
        let window_ref = window.clone();
        let surface = surface.clone();
        let device = device.clone();
        Closure::wrap(Box::new(move |_e: web_sys::Event| {
            let window = web_sys::window().unwrap();
            let width = window.inner_width().unwrap().as_f64().unwrap() as f32;
            let height = window.inner_height().unwrap().as_f64().unwrap() as f32;
            window_ref.set_inner_size(PhysicalSize::new(width, height));

            surface_config.width = width as u32;
            surface_config.height = height as u32;
            surface.configure(&device, &surface_config);

            let model = model.clone();
            wasm_bindgen_futures::spawn_local(async move {
                model.borrow_mut().resize(width, height).await;
            });
        }) as Box<dyn FnMut(_)>)
    };

    #[cfg(target_arch = "wasm32")]
    {
        web_sys::window()
            .unwrap()
            .add_event_listener_with_callback("resize", closure.as_ref().unchecked_ref())
            .unwrap();
    }

    #[cfg(target_arch = "wasm32")]
    let mut last_update = now();
    #[cfg(not(target_arch = "wasm32"))]
    let mut last_update = SystemTime::now();
    event_loop.run(move |event, _, control_flow| {
        #[cfg(target_arch = "wasm32")]
        let _ = &closure;
        match event {
            #[cfg(not(target_arch = "wasm32"))]
            Event::WindowEvent {
                event:
                    WindowEvent::Resized(size)
                    | WindowEvent::ScaleFactorChanged {
                        new_inner_size: &mut size,
                        ..
                    },
                ..
            } => {
                surface_config.width = size.width;
                surface_config.height = size.height;
                surface.configure(&device, &surface_config);

                let model = model.clone();
                handle.block_on(async move {
                    model.lock().unwrap().resize(size.width as f32, size.height as f32).await;
                });
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } | Event::UserEvent(UserEvent::Shutdown) => {
                *control_flow = ControlFlow::Exit;
            }
            Event::RedrawRequested(_) => {
                #[cfg(target_arch = "wasm32")]
                let now = now();
                #[cfg(not(target_arch = "wasm32"))]
                let now = SystemTime::now();
                let delta = now.duration_since(last_update).unwrap();
                last_update = now;

                match surface.get_current_texture() {
                    Ok(output) => {
                        let view = output.texture.create_view(&Default::default());

                        let model = model.clone();
                        let queue = queue.clone();
                        #[cfg(target_arch = "wasm32")]
                        {
                            wasm_bindgen_futures::spawn_local(async move {
                                model.lock().unwrap().update(delta).await;
                                model.lock().unwrap().render(&queue, &view);
                            });
                        }
                        #[cfg(not(target_arch = "wasm32"))]
                        {
                            handle.block_on(async move {
                                model.lock().unwrap().update(delta).await;
                                model.lock().unwrap().render(&queue, &view);
                            });
                        }
                    }
                    Err(err) => {
                        error!("Error getting texture: {:?}", err);
                    }
                }
            }
            Event::RedrawEventsCleared => {
                window.request_redraw();
            }
            Event::LoopDestroyed => {
                // Remove the canvas element we appended earlier
                #[cfg(target_arch = "wasm32")]
                {
                    let window = web_sys::window().unwrap();
                    let document = window.document().unwrap();
                    let canvas = document.get_element_by_id(&canvas_id).expect("Unable to find canvas element when shutting down. The canvas element has either already been removed or will not be removed.");
                    canvas.remove();
                }
            }
            _ => {}
        }
    });
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn stop_neonet() {
    APP_CONTROLLER.lock().unwrap().shutdown();
}

#[cfg(target_arch = "wasm32")]
fn now() -> SystemTime {
    let performance = web_sys::window().unwrap().performance().unwrap();
    let amt = performance.now();
    let secs = (amt as u64) / 1_000;
    let nanos = ((amt as u32) % 1_000) * 1_000_000;
    UNIX_EPOCH + Duration::new(secs, nanos)
}
