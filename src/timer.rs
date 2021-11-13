use web_sys::console;

pub struct Timer {
    name: String,
}

impl Timer {
    pub fn new(name: String) -> Timer {
        console::time_with_label(&name);
        Timer { name }
    }

    pub fn from_str(name: &str) -> Timer {
        console::time_with_label(name);
        Timer {
            name: name.to_string(),
        }
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        console::time_end_with_label(&self.name);
    }
}
