#![feature(never_type)]

#[macro_use]
extern crate async_trait;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;
#[macro_use]
extern crate thiserror;

mod buffer;
mod controller;
pub mod flow;
mod grid;
pub mod neonet;
mod timer;
mod util;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::{wasm_bindgen, JsValue};

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
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub async fn start_neonet(canvas_container_id: String, canvas_id: String) {
    flow::WebFlow::new()
        .canvas_container_id(canvas_container_id)
        .canvas_id(canvas_id)
        .start::<neonet::NeonetApp>()
        .await
        .unwrap();
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn stop_neonet() {
    controller::APP_CONTROLLER.lock().unwrap().shutdown();
}
