mod grid;
mod timer;

use crate::{
    grid::{Grid, Positioned},
    timer::Timer,
};
use rand::{thread_rng, Rng};
use std::{cell::RefCell, rc::Rc, time::UNIX_EPOCH};
use wasm_bindgen::{
    prelude::*,
    JsCast,
    __rt::{
        core::{f32::consts::PI, time::Duration},
        std::time::SystemTime,
    },
};
use web_sys::{console, CanvasRenderingContext2d, HtmlCanvasElement};

const LINE_LENGTH: f32 = 200f32;
const POINT_COUNT: u32 = 200;
const BACKGROUND_COLOR: &str = "#000c18";
const LINE_COLOR: &str = "#006688";

// When the `wee_alloc` feature is enabled, this uses `wee_alloc` as the global
// allocator.
//
// If you don't want to use `wee_alloc`, you can safely delete this.
#[cfg(feature = "wee_alloc")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

// This is like the `main` function, except for JavaScript.
#[wasm_bindgen(start)]
pub fn main_js() -> Result<(), JsValue> {
    // This provides better error messages in debug mode.
    // It's disabled in release mode so it doesn't bloat up the file size.
    #[cfg(debug_assertions)]
        console_error_panic_hook::set_once();

    Ok(())
}

#[wasm_bindgen]
pub fn start_neo_net(canvas_id: &str) {
    let f: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
    let g = f.clone();

    let mut model = Model::new(canvas_id);
    let mut last_update = now();
    *g.borrow_mut() = Some(Closure::wrap(Box::new(move || {
        let now = now();
        let delta = now.duration_since(last_update).unwrap();
        last_update = now;

        model.update(delta);
        model.render();

        web_sys::window()
            .unwrap()
            .request_animation_frame(f.borrow().as_ref().unwrap().as_ref().unchecked_ref())
            .unwrap();
    })));

    web_sys::window()
        .unwrap()
        .request_animation_frame(g.borrow().as_ref().unwrap().as_ref().unchecked_ref())
        .unwrap();
}

fn now() -> SystemTime {
    let performance = web_sys::window().unwrap().performance().unwrap();
    let amt = performance.now();
    let secs = (amt as u64) / 1_000;
    let nanos = ((amt as u32) % 1_000) * 1_000_000;
    UNIX_EPOCH + Duration::new(secs, nanos)
}

struct Model {
    canvas: HtmlCanvasElement,
    context: CanvasRenderingContext2d,
    points: Grid<Point>,
}

#[derive(Debug, Copy, Clone)]
struct Point {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
}

impl Positioned for Point {
    fn x(&self) -> f32 {
        self.x
    }

    fn y(&self) -> f32 {
        self.y
    }

    fn x_mut(&mut self) -> &mut f32 {
        &mut self.x
    }

    fn y_mut(&mut self) -> &mut f32 {
        &mut self.y
    }
}

impl Model {
    pub fn new(canvas_id: &str) -> Model {
        let document = web_sys::window().unwrap().document().unwrap();
        let canvas = document
            .get_element_by_id(canvas_id)
            .unwrap()
            .dyn_into::<HtmlCanvasElement>()
            .unwrap();

        console::log_1(&JsValue::from_str("Initializing NeoNet..."));
        console::log_1(&JsValue::from_str(&format!("Canvas ID: {}", canvas_id)));

        let context = canvas
            .get_context("2d")
            .unwrap()
            .unwrap()
            .dyn_into::<web_sys::CanvasRenderingContext2d>()
            .unwrap();

        let width = web_sys::window()
            .unwrap()
            .inner_width()
            .unwrap()
            .as_f64()
            .unwrap() as f32;
        let height = web_sys::window()
            .unwrap()
            .inner_height()
            .unwrap()
            .as_f64()
            .unwrap() as f32;
        canvas.set_width(width as u32);
        canvas.set_height(height as u32);

        context
            .set_global_composite_operation("destination-atop")
            .unwrap();

        let mut points = Grid::new(
            LINE_LENGTH,
            LINE_LENGTH,
            width + LINE_LENGTH * 2.0,
            height + LINE_LENGTH * 2.0,
        );
        let mut rng = thread_rng();
        for _i in 0..POINT_COUNT {
            let angle = rng.gen_range(0.0..(PI * 2.0));
            let speed = rng.gen_range(20.0..100.0f32);
            points.insert(Point {
                x: rng.gen_range(-LINE_LENGTH..width + LINE_LENGTH),
                y: rng.gen_range(-LINE_LENGTH..height + LINE_LENGTH),
                vx: angle.cos() * speed,
                vy: angle.sin() * speed,
            })
        }

        Model {
            canvas,
            context,
            points,
        }
    }

    pub fn update(&mut self, delta: Duration) {
        #[cfg(debug_assertions)]
        let _timer = Timer::from_str("Model::update");

        let width = web_sys::window()
            .unwrap()
            .inner_width()
            .unwrap()
            .as_f64()
            .unwrap() as f32;
        let height = web_sys::window()
            .unwrap()
            .inner_height()
            .unwrap()
            .as_f64()
            .unwrap() as f32;
        self.canvas.set_width(width as u32);
        self.canvas.set_height(height as u32);

        self.points
            .set_size(width + LINE_LENGTH * 2.0, height + LINE_LENGTH * 2.0);

        self.points.all_mut(|point| {
            point.x += point.vx * delta.as_secs_f32();
            point.y += point.vy * delta.as_secs_f32();

            if point.x < -LINE_LENGTH {
                point.x += width + LINE_LENGTH * 2.0;
            } else if point.x > width + LINE_LENGTH {
                point.x -= width + LINE_LENGTH * 2.0;
            }

            if point.y < -LINE_LENGTH {
                point.y += height + LINE_LENGTH * 2.0;
            } else if point.y > height + LINE_LENGTH {
                point.y -= height + LINE_LENGTH * 2.0;
            }
        });
    }

    pub fn render(&mut self) {
        #[cfg(debug_assertions)]
        let _timer = Timer::from_str("Model::render");

        let width = web_sys::window()
            .unwrap()
            .inner_width()
            .unwrap()
            .as_f64()
            .unwrap() as f32;
        let height = web_sys::window()
            .unwrap()
            .inner_height()
            .unwrap()
            .as_f64()
            .unwrap() as f32;

        self.context
            .set_fill_style(&JsValue::from_str(BACKGROUND_COLOR));
        self.context
            .fill_rect(0.0, 0.0, width.into(), height.into());

        let context = &self.context;

        self.points.pairs(|point, other, distance_sqr| {
            #[cfg(debug_assertions)]
            let _timer1 = Timer::new(format!("Model::render point={:?} other={:?}", point, other));

            let alpha = ((1.0 - distance_sqr.sqrt() / LINE_LENGTH) * 255.0) as u8;

            context
                .set_stroke_style(&JsValue::from_str(&format!("{}{:02x}", LINE_COLOR, alpha)));

            context.begin_path();

            context.move_to(point.x.into(), point.y.into());
            context.line_to(other.x.into(), other.y.into());

            context.stroke();
        });
    }
}
