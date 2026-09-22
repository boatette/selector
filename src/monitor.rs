use std::time::Instant;

use smithay_client_toolkit::{
    compositor::{CompositorState, FrameCallbackData, Region},
    reexports::protocols::ext::background_effect::v1::client::ext_background_effect_surface_v1,
    shell::{WaylandSurface, wlr_layer::LayerSurface},
    shm::{
        Shm,
        slot::{Buffer, SlotPool},
    },
};
use wayland_client::{
    QueueHandle,
    protocol::{wl_output, wl_shm, wl_surface},
};

use crate::app::App;
use crate::config::Config;
use crate::fade::Look;
use crate::geometry::{Point, Rect};
use crate::render::{self, BYTES_PER_PIXEL, Painted};

type BlurSurface = ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1;

const FRAMES: usize = 2;

struct Frame {
    buffer: Buffer,
    painted: Option<Painted>,
}

pub struct Paint<'a> {
    pub config: &'a Config,
    pub compositor: &'a CompositorState,
    pub shm: &'a Shm,
    pub look: Look,
}

pub struct Monitor {
    output: wl_output::WlOutput,
    layer: LayerSurface,
    pool: Option<SlotPool>,
    frames: Vec<Frame>,
    origin: (i32, i32),
    width: u32,
    height: u32,
    configured: bool,
    dirty: bool,
    frame_pending: bool,
    painted: Option<Painted>,
    blur: Option<BlurSurface>,
    blur_region: Option<Rect>,
    last_draw: Option<Instant>,
}

impl Monitor {
    pub const fn new(
        output: wl_output::WlOutput,
        layer: LayerSurface,
        origin: (i32, i32),
        blur: Option<BlurSurface>,
    ) -> Self {
        Self {
            output,
            layer,
            pool: None,
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

    pub const fn is_configured(&self) -> bool {
        self.configured
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
        if (self.width, self.height) != (width, height) {
            self.release_frames();
            self.painted = None;
        }

        self.width = width;
        self.height = height;
        self.configured = true;
        self.frame_pending = false;
    }

    fn release_frames(&mut self) {
        self.frames.clear();
        self.pool = None;
    }

    fn free_frame(&mut self, shm: &Shm) -> Option<usize> {
        let Self { pool, frames, .. } = self;
        if let Some(pool) = pool
            && let Some(index) =
                (0..frames.len()).find(|&i| pool.canvas(&frames[i].buffer).is_some())
        {
            return Some(index);
        }

        if frames.len() < FRAMES {
            self.new_frame(shm)
        } else {
            None
        }
    }

    // A fresh memfd reads as zeroes, which is transparent, so a new frame starts out blank.
    // Pools are dropped whole rather than reused, which keeps that true.
    fn new_frame(&mut self, shm: &Shm) -> Option<usize> {
        let (width, height) = (self.width as i32, self.height as i32);
        let stride = width * BYTES_PER_PIXEL as i32;
        let len = height as usize * stride as usize;

        let pool = match &mut self.pool {
            Some(pool) => pool,
            // slots are 64-byte aligned, round up so the first frame fits without a resize
            None => match SlotPool::new(len.next_multiple_of(64), shm) {
                Ok(pool) => self.pool.insert(pool),
                Err(err) => {
                    log::error!("failed to create an shm pool: {err}");
                    return None;
                }
            },
        };

        match pool.create_buffer(width, height, stride, wl_shm::Format::Argb8888) {
            Ok((buffer, _)) => {
                self.frames.push(Frame {
                    buffer,
                    painted: None,
                });
                Some(self.frames.len() - 1)
            }
            Err(err) => {
                log::error!("failed to create a buffer: {err}");
                None
            }
        }
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
        if !self.needs_draw(paint) {
            return;
        }

        self.dirty = true;
        if !self.frame_pending {
            self.draw(qh, paint);
        }
    }

    pub fn frame_done(&mut self, qh: &QueueHandle<App>, paint: &Paint) {
        self.frame_pending = false;
        if self.dirty || self.needs_draw(paint) {
            self.draw(qh, paint);
        }
    }

    fn wanted(&self, look: &Look) -> Option<Painted> {
        self.local_rect(look.rect).map(|rect| Painted {
            rect,
            opacity: look.opacity,
        })
    }

    fn needs_draw(&self, paint: &Paint) -> bool {
        let wanted = self.wanted(&paint.look);
        wanted != self.painted || (wanted.is_some() && paint.look.leaving)
    }

    pub fn draw(&mut self, qh: &QueueHandle<App>, paint: &Paint) {
        if !self.configured || self.width == 0 || self.height == 0 {
            return;
        }

        self.dirty = false;

        let timing = log::log_enabled!(log::Level::Debug).then(Instant::now);

        let (width, height) = (self.width, self.height);
        let now = self.wanted(&paint.look);

        let Some(index) = self.free_frame(paint.shm) else {
            self.dirty = true;
            if !self.frame_pending {
                let surface = self.layer.wl_surface();
                surface.frame(qh, FrameCallbackData(surface.clone()));
                self.frame_pending = true;
                self.layer.commit();
            }
            return;
        };

        let Self {
            pool: Some(pool),
            frames,
            ..
        } = self
        else {
            return;
        };
        let frame = &mut frames[index];

        let Some(canvas) = pool.canvas(&frame.buffer) else {
            log::error!("buffer {index} became busy while drawing");
            return;
        };

        let stale = render::changes(frame.painted, now, paint.config, width, height);
        render::draw(canvas, width, height, now, &stale, paint.config);
        frame.painted = now;

        let surface = self.layer.wl_surface();

        if self.painted.is_none() && now.is_none() {
            surface.damage_buffer(0, 0, width as i32, height as i32);
        } else {
            for area in render::changes(self.painted, now, paint.config, width, height) {
                if area.is_empty() {
                    continue;
                }
                let area = area.grow(1).clamp_to_surface(width, height);
                surface.damage_buffer(area.x, area.y, area.width as i32, area.height as i32);
            }
        }

        surface.frame(qh, FrameCallbackData(surface.clone()));
        self.frame_pending = true;

        if let Err(err) = self.frames[index].buffer.attach_to(surface) {
            log::error!("failed to attach buffer: {err}");
            return;
        }

        let blur = now.map(|now| now.rect).filter(|_| !paint.look.leaving);
        self.set_blur_region(paint.compositor, blur, paint.config.corner_radius);
        self.painted = now;

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

impl Drop for Monitor {
    fn drop(&mut self) {
        if let Some(effect) = &self.blur {
            effect.destroy();
        }
    }
}
