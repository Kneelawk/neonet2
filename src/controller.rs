use crate::flow::FlowSignal;
use std::sync::{Arc, Mutex};
use winit::event_loop::EventLoopProxy;

lazy_static! {
    pub static ref APP_CONTROLLER: Arc<Mutex<AppController>> =
        Arc::new(Mutex::new(AppController::None));
}

pub enum AppController {
    Proxy(EventLoopProxy<FlowSignal>),
    None,
}

// Force `EventLoopProxy` to be `Send` even in WASM contexts, because WASM is
// single-threaded.
#[cfg(target_arch = "wasm32")]
unsafe impl Send for AppController {}

impl AppController {
    pub fn shutdown(&self) {
        match self {
            AppController::Proxy(proxy) => {
                proxy.send_event(FlowSignal::Exit).unwrap();
            },
            AppController::None => {},
        }
    }
}
