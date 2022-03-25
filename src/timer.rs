pub struct Timer {
    name: String,
}

impl Timer {
    pub fn new(name: String) -> Timer {
        #[cfg(target_arch = "wasm32")]
        web_sys::console::time_with_label(&name);
        Timer { name }
    }

    pub fn from_str(name: &str) -> Timer {
        #[cfg(target_arch = "wasm32")]
        web_sys::console::time_with_label(name);
        Timer { name: name.to_string() }
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        #[cfg(target_arch = "wasm32")]
        web_sys::console::time_end_with_label(&self.name);
    }
}
