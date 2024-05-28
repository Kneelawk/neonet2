//! Web-Specific Flow implementation.

use crate::flow::{FlowModel, FlowModelInit, FlowStartError, WindowSize};
use futures::lock::Mutex;
use js_sys::Promise;
use raw_window_handle::{
    HasRawDisplayHandle, HasRawWindowHandle, RawDisplayHandle, RawWindowHandle, WebDisplayHandle,
    WebWindowHandle,
};
use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use wasm_bindgen::{prelude::wasm_bindgen, JsCast, JsValue};
use wasm_bindgen_futures::future_to_promise;
use web_sys::{Element, HtmlCanvasElement};
use wgpu::{
    Backends, CompositeAlphaMode, Device, DeviceDescriptor, Instance, Limits, PresentMode, Queue,
    RequestAdapterOptions, Surface, SurfaceConfiguration, TextureFormat, TextureUsages,
};

/// Used to manage a web application's control flow as well as integration with
/// the canvas and WGPU.
pub struct WebFlowBuilder {
    canvas_container_id: String,
    canvas_id: String,
}

impl WebFlowBuilder {
    pub fn new() -> WebFlowBuilder {
        WebFlowBuilder {
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

    pub async fn start<Model: FlowModel + 'static>(self) -> Result<WebFlow, FlowStartError> {
        let Self { canvas_container_id, canvas_id } = self;

        info!("Getting window data...");
        let web_window = web_sys::window().unwrap();
        let window_width = web_window.inner_width().unwrap().as_f64().unwrap() as f32;
        let window_height = web_window.inner_height().unwrap().as_f64().unwrap() as f32;
        let window_size = WindowSize {
            width: window_width,
            height: window_height,
        };

        let window_id = 1;
        let window_handle = CanvasHandleWrapper(window_id);

        info!("Setting up canvas...");
        let canvas = {
            info!("Canvas container id: {}", &canvas_container_id);
            info!("Canvas id: {}", &canvas_id);

            let document = web_window.document().unwrap();
            let canvas_container = document
                .get_element_by_id(&canvas_container_id)
                .expect("Unable to find canvas container element");

            let canvas_element = document.create_element("canvas").unwrap();
            canvas_element.set_id(&canvas_id);
            // canvas_element.set_attribute("tabindex", "0").unwrap();

            // get WGPU to recognize the canvas
            canvas_element
                .set_attribute("data-raw-handle", &window_id.to_string())
                .unwrap();

            // set size
            set_canvas_size(&canvas_element, &window_size);

            canvas_container.append_child(&canvas_element).unwrap();

            canvas_element.unchecked_into()
        };

        info!("Creating instance...");
        let instance = Arc::new(Instance::new(Backends::all()));

        info!("Creating surface...");
        let surface = Arc::new(unsafe { instance.create_surface(&window_handle) });

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

        info!("Configuring surface...");
        let preferred_format = surface.get_supported_formats(&adapter).into_iter().next();
        info!("Preferred render frame format: {:?}", preferred_format);
        let config = SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT,
            format: preferred_format.unwrap_or(TextureFormat::Bgra8UnormSrgb),
            width: window_size.width as u32,
            height: window_size.height as u32,
            present_mode: PresentMode::Fifo,
            alpha_mode: CompositeAlphaMode::Auto,
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
        let model: Arc<Mutex<dyn FlowModel>> = Arc::new(Mutex::new(Model::init(init).await));

        let previous_render = now();

        Ok(WebFlow {
            canvas,
            _instance: instance,
            surface,
            device,
            _queue: queue,
            config,
            model,
            previous_render,
        })
    }
}

#[wasm_bindgen]
pub struct WebFlow {
    canvas: HtmlCanvasElement,
    _instance: Arc<Instance>,
    surface: Arc<Surface>,
    device: Arc<Device>,
    _queue: Arc<Queue>,
    config: SurfaceConfiguration,
    model: Arc<Mutex<dyn FlowModel>>,
    previous_render: SystemTime,
}

#[wasm_bindgen]
impl WebFlow {
    pub fn resize(&self, width: f32, height: f32) -> Promise {
        let canvas = self.canvas.clone();
        let model = self.model.clone();
        let surface = self.surface.clone();
        let device = self.device.clone();
        let mut config = self.config.clone();

        future_to_promise(async move {
            info!("Resizing: {}x{}", width, height);
            let window_size = WindowSize { width, height };

            set_canvas_size(&canvas, &window_size);
            config.width = width as u32;
            config.height = height as u32;
            surface.configure(&device, &config);

            model.lock().await.resize(window_size).await;

            Ok(JsValue::undefined())
        })
    }

    pub fn render(&mut self) -> Promise {
        let model = self.model.clone();
        let surface = self.surface.clone();

        let now = now();
        let delta = now.duration_since(self.previous_render).unwrap();
        self.previous_render = now;

        future_to_promise(async move {
            info!("Rendering...");

            let mut model = model.lock().await;
            model.update(delta).await;

            match surface.get_current_texture() {
                Ok(output) => {
                    let view = output.texture.create_view(&Default::default());

                    model.render(&view, delta);

                    output.present();
                },
                Err(err) => {
                    error!("Error getting texture: {:?}", err);
                },
            }

            Ok(JsValue::undefined())
        })
    }
}

impl Drop for WebFlow {
    fn drop(&mut self) {
        info!("Removing canvas...");
        self.canvas.remove();
    }
}

struct CanvasHandleWrapper(u32);

unsafe impl HasRawWindowHandle for CanvasHandleWrapper {
    fn raw_window_handle(&self) -> RawWindowHandle {
        let mut web_handle = WebWindowHandle::empty();
        web_handle.id = self.0;
        RawWindowHandle::Web(web_handle)
    }
}

unsafe impl HasRawDisplayHandle for CanvasHandleWrapper {
    fn raw_display_handle(&self) -> RawDisplayHandle {
        RawDisplayHandle::Web(WebDisplayHandle::empty())
    }
}

fn set_canvas_size(canvas_element: &Element, window_size: &WindowSize) {
    canvas_element
        .set_attribute("width", &window_size.width.to_string())
        .unwrap();
    canvas_element
        .set_attribute("height", &window_size.height.to_string())
        .unwrap();
    canvas_element
        .set_attribute(
            "style",
            &format!(
                "width: {}px; height: {}px;",
                window_size.width, window_size.height
            ),
        )
        .unwrap();
}

#[cfg(target_arch = "wasm32")]
fn now() -> SystemTime {
    let performance = web_sys::window().unwrap().performance().unwrap();
    let amt = performance.now();
    let secs = (amt as u64) / 1_000;
    let nanos = ((amt as u32) % 1_000) * 1_000_000;
    UNIX_EPOCH + Duration::new(secs, nanos)
}
