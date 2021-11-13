use crate::timer::Timer;

pub struct Grid<P: Positioned + Clone> {
    position_offset: f32,
    chunk_size: f32,
    chunks: Vec<Vec<Vec<P>>>,
    tmp: Vec<P>,
}

pub trait Positioned {
    fn x(&self) -> f32;
    fn y(&self) -> f32;

    fn x_mut(&mut self) -> &mut f32;
    fn y_mut(&mut self) -> &mut f32;
}

impl<P: Positioned + Clone> Grid<P> {
    pub fn new(position_offset: f32, chunk_size: f32, width: f32, height: f32) -> Grid<P> {
        let x_chunks = (width / chunk_size).ceil() as usize;
        let y_chunks = (height / chunk_size).ceil() as usize;

        let mut chunks = vec![];
        for _y in 0..y_chunks {
            let mut strip = vec![];
            for _x in 0..x_chunks {
                strip.push(vec![]);
            }
            chunks.push(strip);
        }

        Grid {
            position_offset,
            chunk_size,
            chunks,
            tmp: vec![],
        }
    }

    pub fn set_size(&mut self, width: f32, height: f32) {
        let x_chunks = (width / self.chunk_size).ceil() as usize;
        let y_chunks = (height / self.chunk_size).ceil() as usize;

        for strip in self.chunks.iter_mut() {
            if x_chunks > strip.len() {
                for _ in strip.len()..x_chunks {
                    strip.push(vec![]);
                }
            } else if x_chunks < strip.len() {
                for x in (x_chunks..strip.len()).rev() {
                    let chunk = strip.remove(x);
                    let new_x = x % x_chunks;
                    let new_chunk = &mut strip[new_x];
                    let dif_x = (x - new_x) as f32 * self.chunk_size;
                    for mut p in chunk {
                        *p.x_mut() -= dif_x;
                        new_chunk.push(p);
                    }
                }
            }
        }

        if y_chunks > self.chunks.len() {
            for _ in self.chunks.len()..y_chunks {
                let mut strip = vec![];
                for _ in 0..x_chunks {
                    strip.push(vec![]);
                }
                self.chunks.push(strip);
            }
        } else if y_chunks < self.chunks.len() {
            for y in (y_chunks..self.chunks.len()).rev() {
                let strip = self.chunks.remove(y);
                let new_y = y % y_chunks;
                let new_strip = &mut self.chunks[new_y];
                let dif_y = (y - new_y) as f32 * self.chunk_size;
                for (x, chunk) in strip.into_iter().enumerate() {
                    let new_chunk = &mut new_strip[x];
                    for mut p in chunk {
                        *p.y_mut() -= dif_y;
                        new_chunk.push(p);
                    }
                }
            }
        }
    }

    pub fn insert(&mut self, p: P) {
        let x = (((p.x() + self.position_offset) / self.chunk_size)
            .floor()
            .max(0.0) as usize)
            .min(self.chunks[0].len() - 1);
        let y = (((p.y() + self.position_offset) / self.chunk_size)
            .floor()
            .max(0.0) as usize)
            .min(self.chunks.len() - 1);
        self.chunks[y][x].push(p);
    }

    pub fn clear(&mut self) {
        for strip in self.chunks.iter_mut() {
            for chunk in strip.iter_mut() {
                chunk.clear();
            }
        }
    }

    pub fn all_mut<F: FnMut(&mut P)>(&mut self, mut f: F) {
        for strip in self.chunks.iter_mut() {
            for chunk in strip.iter_mut() {
                self.tmp.append(chunk);
            }
        }

        for mut p in self.tmp.drain(..) {
            f(&mut p);
            let x = (((p.x() + self.position_offset) / self.chunk_size)
                .floor()
                .max(0.0) as usize)
                .min(self.chunks[0].len() - 1);
            let y = (((p.y() + self.position_offset) / self.chunk_size)
                .floor()
                .max(0.0) as usize)
                .min(self.chunks.len() - 1);
            self.chunks[y][x].push(p);
        }
    }

    pub fn all_within<F: FnMut(&P, f32)>(&self, x: f32, y: f32, distance: f32, mut f: F) {
        let x = x + self.position_offset;
        let y = y + self.position_offset;
        let max_distance_sqr = distance * distance;
        let min_x = ((x - distance - 1.0) / self.chunk_size).floor().max(0.0) as usize;
        let min_y = ((y - distance - 1.0) / self.chunk_size).floor().max(0.0) as usize;
        let max_x = (((x + distance + 1.0) / self.chunk_size).floor() as usize)
            .min(self.chunks[0].len() - 1);
        let max_y =
            (((y + distance + 1.0) / self.chunk_size).floor() as usize).min(self.chunks.len() - 1);
        for strip in &self.chunks[min_y..=max_y] {
            for chunk in &strip[min_x..=max_x] {
                for p in chunk.iter() {
                    let x = (p.x() + self.position_offset) - x;
                    let y = (p.y() + self.position_offset) - y;
                    let distance_sqr = x * x + y * y;
                    if distance_sqr < max_distance_sqr {
                        f(p, distance_sqr);
                    }
                }
            }
        }
    }

    pub fn pairs<F: FnMut(&P, &P, f32)>(&mut self, mut f: F) {
        #[cfg(debug_assertions)]
        let _timer = Timer::from_str("Grid::pairs");

        let max_distance_sqr = self.chunk_size * self.chunk_size;
        let mut try_call = |p: &P, op: &P| {
            let x = p.x() - op.x();
            let y = p.y() - op.y();
            let distance_sqr = x * x + y * y;
            if distance_sqr < max_distance_sqr {
                f(p, op, distance_sqr);
            }
        };

        let grid_len = self.chunks.len();
        for y in 0..grid_len {
            #[cfg(debug_assertions)]
            let _timer = Timer::new(format!("Grid::paris y={}", y));

            let strip = &self.chunks[y];
            let next_strip = if y < grid_len - 1 {
                Some(&self.chunks[y + 1])
            } else {
                None
            };

            let strip_len = strip.len();
            for x in 0..strip_len {
                #[cfg(debug_assertions)]
                let _timer = Timer::new(format!("Grid::pairs y={} x={}", y, x));

                let chunk = &strip[x];
                let over = if x < strip_len - 1 {
                    Some(&strip[x + 1])
                } else {
                    None
                };
                let below = next_strip.map(|next_strip| &next_strip[x]);
                let over_below = next_strip.and_then(|next_strip| {
                    if x < strip_len - 1 {
                        Some(&next_strip[x + 1])
                    } else {
                        None
                    }
                });
                let back_below = next_strip.and_then(|next_strip| {
                    if x > 0 {
                        Some(&next_strip[x - 1])
                    } else {
                        None
                    }
                });

                for p in chunk.iter() {
                    {
                        #[cfg(debug_assertions)]
                        let _timer = Timer::new(format!("Grid::pairs self-chunk y={} x={}", y, x));
                        for op in self.tmp.iter() {
                            try_call(p, op);
                        }
                        self.tmp.push(p.clone());
                    }

                    {
                        #[cfg(debug_assertions)]
                        let _timer = Timer::new(format!("Grid::pairs over y={} x={}", y, x));
                        if let Some(over) = over {
                            for op in over.iter() {
                                try_call(p, op);
                            }
                        }
                    }

                    {
                        #[cfg(debug_assertions)]
                        let _timer = Timer::new(format!("Grid::pairs below y={} x={}", y, x));
                        if let Some(below) = below {
                            for op in below.iter() {
                                try_call(p, op);
                            }
                        }
                    }

                    {
                        #[cfg(debug_assertions)]
                        let _timer = Timer::new(format!("Grid::pairs over-below y={} x={}", y, x));
                        if let Some(over_below) = over_below {
                            for op in over_below.iter() {
                                try_call(p, op);
                            }
                        }
                    }

                    {
                        #[cfg(debug_assertions)]
                        let _timer = Timer::new(format!("Grid::pairs back-below y={} x={}", y, x));
                        if let Some(back_below) = back_below {
                            for op in back_below.iter() {
                                try_call(p, op);
                            }
                        }
                    }
                }
                self.tmp.clear();
            }
        }
    }
}
