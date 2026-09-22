use smithay_client_toolkit::{
    dispatch2::Dispatch2,
    reexports::protocols::wp::{
        single_pixel_buffer::v1::client::wp_single_pixel_buffer_manager_v1::{
            self, WpSinglePixelBufferManagerV1,
        },
        viewporter::client::{
            wp_viewport::{self, WpViewport},
            wp_viewporter::{self, WpViewporter},
        },
    },
    shm::{Shm, raw::RawPool},
};
use wayland_client::{
    Connection, QueueHandle,
    globals::GlobalList,
    protocol::{wl_buffer, wl_shm, wl_surface},
};

use crate::app::App;
use crate::render::BYTES_PER_PIXEL;

pub struct Idle {
    viewporter: WpViewporter,
    buffer: wl_buffer::WlBuffer,
}

impl Idle {
    pub fn bind(globals: &GlobalList, qh: &QueueHandle<App>, shm: &Shm) -> Option<Self> {
        let viewporter: WpViewporter = globals.bind(qh, 1..=1, IdleData).ok()?;

        let Some(buffer) = single_pixel(globals, qh).or_else(|| shm_pixel(qh, shm)) else {
            viewporter.destroy();
            return None;
        };

        Some(Self { viewporter, buffer })
    }

    pub fn viewport(&self, surface: &wl_surface::WlSurface, qh: &QueueHandle<App>) -> WpViewport {
        self.viewporter.get_viewport(surface, qh, IdleData)
    }

    pub const fn buffer(&self) -> &wl_buffer::WlBuffer {
        &self.buffer
    }
}

fn single_pixel(globals: &GlobalList, qh: &QueueHandle<App>) -> Option<wl_buffer::WlBuffer> {
    let pixels: WpSinglePixelBufferManagerV1 = globals.bind(qh, 1..=1, IdleData).ok()?;
    let buffer = pixels.create_u32_rgba_buffer(0, 0, 0, 0, qh, IdleData);
    pixels.destroy();
    Some(buffer)
}

// A fresh memfd reads as zeroes, so the pixel is already transparent. The compositor keeps its
// own mapping, so the pool can go as soon as the buffer exists.
fn shm_pixel(qh: &QueueHandle<App>, shm: &Shm) -> Option<wl_buffer::WlBuffer> {
    let mut pool = match RawPool::new(BYTES_PER_PIXEL, shm) {
        Ok(pool) => pool,
        Err(err) => {
            log::error!("failed to create the idle pixel: {err}");
            return None;
        }
    };
    Some(pool.create_buffer(
        0,
        1,
        1,
        BYTES_PER_PIXEL as i32,
        wl_shm::Format::Argb8888,
        IdleData,
        qh,
    ))
}

pub struct IdleData;

impl<D> Dispatch2<WpViewporter, D> for IdleData {
    fn event(
        &self,
        _: &mut D,
        _: &WpViewporter,
        _: wp_viewporter::Event,
        _: &Connection,
        _: &QueueHandle<D>,
    ) {
    }
}

impl<D> Dispatch2<WpViewport, D> for IdleData {
    fn event(
        &self,
        _: &mut D,
        _: &WpViewport,
        _: wp_viewport::Event,
        _: &Connection,
        _: &QueueHandle<D>,
    ) {
    }
}

impl<D> Dispatch2<WpSinglePixelBufferManagerV1, D> for IdleData {
    fn event(
        &self,
        _: &mut D,
        _: &WpSinglePixelBufferManagerV1,
        _: wp_single_pixel_buffer_manager_v1::Event,
        _: &Connection,
        _: &QueueHandle<D>,
    ) {
    }
}

// the buffer never changes, so there is nothing to do when the compositor lets go of it
impl<D> Dispatch2<wl_buffer::WlBuffer, D> for IdleData {
    fn event(
        &self,
        _: &mut D,
        _: &wl_buffer::WlBuffer,
        _: wl_buffer::Event,
        _: &Connection,
        _: &QueueHandle<D>,
    ) {
    }
}
