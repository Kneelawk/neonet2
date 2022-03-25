//! This has the common Flow API stuff.

#[cfg(not(target_arch = "wasm32"))]
mod desktop;
#[cfg(target_arch = "wasm32")]
mod web;

use std::{io, sync::Arc, time::Duration};
use wgpu::{Device, Queue, RequestDeviceError, TextureFormat, TextureView};

#[cfg(not(target_arch = "wasm32"))]
pub use desktop::DesktopFlow;
#[cfg(target_arch = "wasm32")]
pub use web::WebFlow;
#[cfg(target_arch = "wasm32")]
pub use web::WebFlowBuilder;

/// Signal sent by the application to the Flow to control the application flow.
#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq)]
pub enum FlowSignal {
    Exit,
}

/// Contains data to be used when initializing the FlowModel.
pub struct FlowModelInit {
    pub device: Arc<Device>,
    pub queue: Arc<Queue>,
    pub window_size: WindowSize,
    pub frame_format: TextureFormat,
}

/// Represents an application's data, allowing the application to receive
/// lifecycle events. This version of `Flow` and `FlowModel` are designed to
/// support an asynchronous application.
#[async_trait]
pub trait FlowModel {
    async fn init(init: FlowModelInit) -> Self
    where
        Self: Sized;

    /// Specifically handles resize events.
    async fn resize(&mut self, size: WindowSize);

    async fn update(&mut self, update_delta: Duration);

    fn render(&mut self, frame_view: &TextureView, render_delta: Duration);

    fn shutdown(&mut self);
}

#[derive(Error, Debug)]
pub enum FlowStartError {
    #[error("IO error")]
    IOError(#[from] io::Error),
    #[error("Window Builder error")]
    WindowBuilderError,
    #[error("Error requesting adapter")]
    AdapterRequestError,
    #[error("Error requesting device")]
    RequestDeviceError(#[from] RequestDeviceError),
}

/// Describes a window size.
#[derive(Debug, Copy, Clone, PartialOrd, PartialEq)]
pub struct WindowSize {
    pub width: f32,
    pub height: f32,
}
