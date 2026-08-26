use std::time::Instant;

use smithay_client_toolkit::{
    compositor::{CompositorState, FrameCallbackData, Region},
    reexports::protocols::ext::background_effect::v1::client::ext_background_effect_surface_v1,
    shell::{WaylandSurface, wlr_layer::LayerSurface},
    shm::slot::{Buffer, SlotPool},
};
use wayland_client::{
    QueueHandle,
    protocol::{wl_output, wl_shm, wl_surface},
};

use crate::app::App;
use crate::config::Config;
use crate::geometry::{Point, Rect};
use crate::render::{self, BYTES_PER_PIXEL};

type BlurSurface = ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1;

const FRAMES: usize = 2;

struct Frame {
    buffer: Buffer,
    painted: Option<Rect>,
}

pub struct Paint<'a> {
    pub config: &'a Config,
    pub compositor: &'a CompositorState,
    pub selection: Option<Rect>,
}

pub struct Monitor {
    output: wl_output::WlOutput,
    layer: LayerSurface,
    pool: SlotPool,
    frames: Vec<Frame>,
    origin: (i32, i32),
    width: u32,
    height: u32,
    configured: bool,
    dirty: bool,
    frame_pending: bool,
    painted: Option<Rect>,
    blur: Option<BlurSurface>,
    blur_region: Option<Rect>,
    last_draw: Option<Instant>,
}

impl Monitor {
    pub const fn new(
        output: wl_output::WlOutput,
        layer: LayerSurface,
        pool: SlotPool,
        origin: (i32, i32),
        blur: Option<BlurSurface>,
    ) -> Self {
        Self {
            output,
            layer,
            pool,
            frames: Vec::new(),
            origin,
            width: 0,
            height: 0,
            configured: false,
            dirty: false,
            frame_pending: false,
            painted: None,
            blur,
            blur_region: None,
            last_draw: None,
        }
    }

    pub const fn output(&self) -> &wl_output::WlOutput {
        &self.output
    }

    pub fn has_output(&self, output: &wl_output::WlOutput) -> bool {
        &self.output == output
    }

    pub fn has_layer(&self, layer: &LayerSurface) -> bool {
        &self.layer == layer
    }

    pub fn owns(&self, surface: &wl_surface::WlSurface) -> bool {
        self.layer.wl_surface() == surface
    }

    pub fn set_origin(&mut self, origin: (i32, i32)) -> bool {
        let moved = self.origin != origin;
        self.origin = origin;
        moved
    }

    pub fn configure(&mut self, width: u32, height: u32) {
        let resized = (self.width, self.height) != (width, height);

        self.width = width;
        self.height = height;
        self.configured = true;
        self.frame_pending = false;

        if resized || self.frames.is_empty() {
            self.build_frames();
        }
    }

    fn build_frames(&mut self) {
        self.frames.clear();
        self.painted = None;

        let (width, height) = (self.width, self.height);
        let stride = width as i32 * BYTES_PER_PIXEL as i32;
        let len = height as usize * stride as usize;

        if let Err(err) = self.pool.resize(len * FRAMES) {
            log::error!("failed to size the shm pool: {err}");
            return;
        }

        for _ in 0..FRAMES {
            let slot = match self.pool.new_slot(len) {
                Ok(slot) => slot,
                Err(err) => {
                    log::error!("failed to allocate a buffer slot: {err}");
                    return;
                }
            };

            match self.pool.create_buffer_in(
                &slot,
                width as i32,
                height as i32,
                stride,
                wl_shm::Format::Argb8888,
            ) {
                Ok(buffer) => self.frames.push(Frame {
                    painted: Some(Rect::new(0, 0, width, height)),
                    buffer,
                }),
                Err(err) => {
                    log::error!("failed to create a buffer: {err}");
                    return;
                }
            }
        }
    }

    fn free_frame(&mut self) -> Option<usize> {
        let Self { pool, frames, .. } = self;
        (0..frames.len()).find(|&i| pool.canvas(&frames[i].buffer).is_some())
    }

    pub fn local_rect(&self, selection: Option<Rect>) -> Option<Rect> {
        let rect = selection?.translate(-self.origin.0, -self.origin.1);
        rect.overlaps_surface(self.width, self.height)
            .then_some(rect)
    }

    pub fn covers(&self, rect: Rect) -> bool {
        self.local_rect(Some(rect)).is_some()
    }

    pub fn point_to_global(&self, position: (f64, f64)) -> Point {
        Point::new(
            position.0 + self.origin.0 as f64,
            position.1 + self.origin.1 as f64,
        )
    }

    pub fn sync(&mut self, qh: &QueueHandle<App>, paint: &Paint) {
        if self.painted == self.local_rect(paint.selection) {
            return;
        }

        self.dirty = true;
        if !self.frame_pending {
            self.draw(qh, paint);
        }
    }

    pub fn frame_done(&mut self, qh: &QueueHandle<App>, paint: &Paint) {
        self.frame_pending = false;
        if self.dirty {
            self.draw(qh, paint);
        }
    }

    pub fn draw(&mut self, qh: &QueueHandle<App>, paint: &Paint) {
        if !self.configured || self.width == 0 || self.height == 0 {
            return;
        }

        self.dirty = false;

        let timing = log::log_enabled!(log::Level::Debug).then(Instant::now);

        let (width, height) = (self.width, self.height);
        let rect = self.local_rect(paint.selection);

        let Some(index) = self.free_frame() else {
            self.dirty = true;
            if !self.frame_pending {
                let surface = self.layer.wl_surface();
                surface.frame(qh, FrameCallbackData(surface.clone()));
                self.frame_pending = true;
                self.layer.commit();
            }
            return;
        };

        let Self { pool, frames, .. } = self;
        let frame = &mut frames[index];
        let stale = frame.painted;

        let Some(canvas) = pool.canvas(&frame.buffer) else {
            log::error!("buffer {index} became busy while drawing");
            return;
        };

        render::draw(canvas, width, height, rect, stale, paint.config);
        frame.painted = rect;

        let surface = self.layer.wl_surface();

        match damage(self.painted, rect, width, height) {
            Some(area) => {
                surface.damage_buffer(area.x, area.y, area.width as i32, area.height as i32)
            }
            None => surface.damage_buffer(0, 0, width as i32, height as i32),
        }

        surface.frame(qh, FrameCallbackData(surface.clone()));
        self.frame_pending = true;

        if let Err(err) = self.frames[index].buffer.attach_to(surface) {
            log::error!("failed to attach buffer: {err}");
            return;
        }

        self.set_blur_region(paint.compositor, rect, paint.config.corner_radius);
        self.painted = rect;

        self.layer.commit();

        if let Some(started) = timing {
            self.log_frame(started);
        }
    }

    fn log_frame(&mut self, started: Instant) {
        let elapsed = started.elapsed();
        let gap = self.last_draw.replace(started).map(|last| started - last);
        let ms = |d: std::time::Duration| d.as_secs_f64() * 1e3;

        log::debug!(
            "drew {}x{} in {:.3}ms{}",
            self.width,
            self.height,
            ms(elapsed),
            match gap {
                Some(gap) => format!(", {:.3}ms since the last frame", ms(gap)),
                None => String::new(),
            }
        );
    }

    fn set_blur_region(&mut self, compositor: &CompositorState, wanted: Option<Rect>, radius: u32) {
        let Some(effect) = &self.blur else { return };
        if self.blur_region == wanted {
            return;
        }

        match wanted {
            Some(rect) => {
                let region = match Region::new(compositor) {
                    Ok(region) => region,
                    Err(err) => {
                        log::error!("failed to create a blur region: {err}");
                        return;
                    }
                };
                for span in rect.rounded_spans(radius) {
                    let span = span.clamp_to_surface(self.width, self.height);
                    if span.is_empty() {
                        continue;
                    }
                    region.add(span.x, span.y, span.width as i32, span.height as i32);
                }
                effect.set_blur_region(Some(region.wl_region()));
            }
            None => effect.set_blur_region(None),
        }

        self.blur_region = wanted;
    }
}

fn damage(was: Option<Rect>, now: Option<Rect>, width: u32, height: u32) -> Option<Rect> {
    let area = match (was, now) {
        (Some(was), Some(now)) => was.union(now),
        (Some(only), None) | (None, Some(only)) => only,
        (None, None) => return None,
    };

    Some(area.grow(1).clamp_to_surface(width, height))
}

impl Drop for Monitor {
    fn drop(&mut self) {
        if let Some(effect) = &self.blur {
            effect.destroy();
        }
    }
}
