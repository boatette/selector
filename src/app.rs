use anyhow::{Context, Result};
use smithay_client_toolkit::{
    background_effect::{BackgroundEffectHandler, BackgroundEffectState},
    compositor::{CompositorHandler, CompositorState, FrameCallbackData, Region},
    delegate_dispatch2, delegate_registry,
    output::{OutputHandler, OutputState},
    reexports::protocols::ext::background_effect::v1::client::{
        ext_background_effect_manager_v1, ext_background_effect_surface_v1,
    },
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        Capability, SeatHandler, SeatState,
        pointer::{PointerEvent, PointerEventKind, PointerHandler},
    },
    shell::{
        WaylandSurface,
        wlr_layer::{
            Anchor, KeyboardInteractivity, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
    },
    shm::{Shm, ShmHandler, slot::SlotPool},
};
use wayland_client::{
    Connection, QueueHandle,
    globals::{GlobalList, registry_queue_init},
    protocol::{wl_output, wl_pointer, wl_seat, wl_shm, wl_surface},
};

use crate::config::Config;
use crate::geometry::{Point, Rect};
use crate::render::{self, BYTES_PER_PIXEL};
use crate::selection::{BTN_LEFT, Selection};

const NAMESPACE: &str = "selector";

struct Monitor {
    output: wl_output::WlOutput,
    layer: LayerSurface,
    pool: SlotPool,
    selection: Selection,
    width: u32,
    height: u32,
    configured: bool,
    dirty: bool,
    frame_pending: bool,
    blur: Option<ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1>,
    blur_region: Option<Rect>,
}

impl Monitor {
    fn owns(&self, surface: &wl_surface::WlSurface) -> bool {
        self.layer.wl_surface() == surface
    }

    fn set_blur_region(&mut self, compositor: &CompositorState, wanted: Option<Rect>) {
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
                region.add(rect.x, rect.y, rect.width as i32, rect.height as i32);
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

pub struct App {
    registry_state: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,
    shm: Shm,
    compositor: CompositorState,
    layer_shell: LayerShell,
    background_effect: BackgroundEffectState,

    config: Config,
    blur: bool,
    monitors: Vec<Monitor>,
    pointer: Option<wl_pointer::WlPointer>,
    exit: bool,
}

impl App {
    pub fn new(globals: &GlobalList, qh: &QueueHandle<Self>, config: Config) -> Result<Self> {
        let compositor = CompositorState::bind(globals, qh).context("wl_compositor unavailable")?;
        let shm = Shm::bind(globals, qh).context("wl_shm unavailable")?;
        let layer_shell = LayerShell::bind(globals, qh)
            .context("wlr-layer-shell unavailable, this compositor cannot host selector")?;

        let background_effect = BackgroundEffectState::new(globals, qh);

        let blur = config.blur && background_effect.ext_background_effect_manager_v1().is_ok();
        if config.blur && !blur {
            log::warn!(
                "blur is on but this compositor does not implement ext-background-effect, drawing without it"
            );
        }

        Ok(Self {
            registry_state: RegistryState::new(globals),
            seat_state: SeatState::new(globals, qh),
            output_state: OutputState::new(globals, qh),
            shm,
            compositor,
            layer_shell,
            background_effect,
            config,
            blur,
            monitors: Vec::new(),
            pointer: None,
            exit: false,
        })
    }

    pub fn should_exit(&self) -> bool {
        self.exit
    }

    fn output_name(&self, output: &wl_output::WlOutput) -> String {
        self.output_state
            .info(output)
            .and_then(|info| info.name)
            .unwrap_or_else(|| "<unnamed>".to_owned())
    }

    fn add_monitor(&mut self, qh: &QueueHandle<Self>, output: wl_output::WlOutput) {
        if self.monitors.iter().any(|m| m.output == output) {
            return;
        }

        let name = self.output_name(&output);

        let surface = self.compositor.create_surface(qh);
        let layer = self.layer_shell.create_layer_surface(
            qh,
            surface,
            self.config.layer,
            Some(NAMESPACE),
            Some(&output),
        );

        layer.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
        layer.set_size(0, 0);
        layer.set_exclusive_zone(-1);
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);

        layer.commit();

        let pool = match SlotPool::new(BYTES_PER_PIXEL, &self.shm) {
            Ok(pool) => pool,
            Err(err) => {
                log::error!("failed to create shm pool for output {name}: {err}");
                return;
            }
        };

        let blur = self.blur.then(|| {
            self.background_effect
                .get_background_effect(layer.wl_surface(), qh)
        });
        let blur = match blur.transpose() {
            Ok(blur) => blur,
            Err(err) => {
                log::error!("failed to request blur for output {name}: {err}");
                None
            }
        };

        log::info!("tracking output {name}");
        self.monitors.push(Monitor {
            output,
            layer,
            pool,
            selection: Selection::new(self.config.drag_threshold),
            width: 0,
            height: 0,
            configured: false,
            dirty: false,
            frame_pending: false,
            blur,
            blur_region: None,
        });
    }

    fn remove_monitor(&mut self, output: &wl_output::WlOutput) {
        self.monitors.retain(|m| &m.output != output);
    }

    fn monitor_index(&self, surface: &wl_surface::WlSurface) -> Option<usize> {
        self.monitors.iter().position(|m| m.owns(surface))
    }

    fn request_redraw(&mut self, qh: &QueueHandle<Self>, index: usize) {
        let monitor = &mut self.monitors[index];
        monitor.dirty = true;
        if !monitor.frame_pending {
            self.draw(qh, index);
        }
    }

    fn draw(&mut self, qh: &QueueHandle<Self>, index: usize) {
        let config = &self.config;
        let compositor = &self.compositor;
        let monitor = &mut self.monitors[index];

        if !monitor.configured || monitor.width == 0 || monitor.height == 0 {
            return;
        }

        monitor.dirty = false;

        let width = monitor.width;
        let height = monitor.height;
        let stride = width as i32 * BYTES_PER_PIXEL as i32;
        let rect = monitor.selection.rect();

        let buffer = match monitor.pool.create_buffer(
            width as i32,
            height as i32,
            stride,
            wl_shm::Format::Argb8888,
        ) {
            Ok((buffer, canvas)) => {
                render::draw(canvas, width, height, rect, config);
                buffer
            }
            Err(err) => {
                log::error!("failed to acquire a buffer: {err}");
                return;
            }
        };

        let surface = monitor.layer.wl_surface();
        surface.damage_buffer(0, 0, width as i32, height as i32);
        surface.frame(qh, FrameCallbackData(surface.clone()));
        monitor.frame_pending = true;

        if let Err(err) = buffer.attach_to(surface) {
            log::error!("failed to attach buffer: {err}");
            return;
        }

        let blurred = rect
            .map(|rect| rect.clamp_to_surface(width, height))
            .filter(|rect| !rect.is_empty());
        monitor.set_blur_region(compositor, blurred);

        monitor.layer.commit();
    }

    fn selection_completed(&mut self, index: usize, rect: Rect) {
        let name = self.output_name(&self.monitors[index].output);

        log::info!(
            "selection on {name}: {}x{} at ({}, {})",
            rect.width,
            rect.height,
            rect.x,
            rect.y
        );
    }
}

impl CompositorHandler for App {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_factor: i32,
    ) {
        // buffers are allocated at the size the compositor configures, so scaling is handled by the configure path
    }

    fn transform_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_transform: wl_output::Transform,
    ) {
    }

    fn frame(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        _time: u32,
    ) {
        let Some(index) = self.monitor_index(surface) else {
            return;
        };

        self.monitors[index].frame_pending = false;
        if self.monitors[index].dirty {
            self.draw(qh, index);
        }
    }

    fn surface_enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for App {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        self.add_monitor(qh, output);
    }

    fn update_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
        // a resolution change arrives as a fresh configure on the layer surface
    }

    fn output_destroyed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        self.remove_monitor(&output);
    }
}

impl LayerShellHandler for App {
    fn closed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, layer: &LayerSurface) {
        self.monitors.retain(|m| &m.layer != layer);

        if self.monitors.is_empty() {
            log::info!("last layer surface closed, exiting");
            self.exit = true;
        }
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        let Some(index) = self.monitors.iter().position(|m| &m.layer == layer) else {
            return;
        };

        let (width, height) = configure.new_size;
        if width == 0 || height == 0 {
            log::warn!("compositor configured a zero-sized surface, ignoring");
            return;
        }

        let monitor = &mut self.monitors[index];
        monitor.width = width;
        monitor.height = height;
        monitor.configured = true;
        monitor.frame_pending = false;

        self.draw(qh, index);
    }
}

impl SeatHandler for App {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}

    fn new_capability(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        match capability {
            Capability::Pointer if self.pointer.is_none() => {
                match self.seat_state.get_pointer(qh, &seat) {
                    Ok(pointer) => self.pointer = Some(pointer),
                    Err(err) => log::error!("failed to acquire pointer: {err}"),
                }
            }
            _ => {}
        }
    }

    fn remove_capability(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Pointer
            && let Some(pointer) = self.pointer.take()
        {
            pointer.release();
        }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl PointerHandler for App {
    fn pointer_frame(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        _pointer: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        for event in events {
            let Some(index) = self.monitor_index(&event.surface) else {
                continue;
            };
            let at = Point::from(event.position);

            let redraw = match event.kind {
                PointerEventKind::Press {
                    button: BTN_LEFT, ..
                } => self.monitors[index].selection.press(at),
                PointerEventKind::Motion { .. } => self.monitors[index].selection.motion(at),
                PointerEventKind::Release {
                    button: BTN_LEFT, ..
                } => {
                    let motion_redraw = self.monitors[index].selection.motion(at);
                    let (rect, release_redraw) = self.monitors[index].selection.release();

                    if let Some(rect) = rect {
                        let clamped = rect.clamp_to_surface(
                            self.monitors[index].width,
                            self.monitors[index].height,
                        );
                        self.selection_completed(index, clamped);
                    }

                    motion_redraw.or(release_redraw)
                }
                PointerEventKind::Press { .. } => self.monitors[index].selection.cancel(),
                PointerEventKind::Leave { .. } => self.monitors[index].selection.cancel(),
                PointerEventKind::Enter { .. } | PointerEventKind::Axis { .. } => continue,
                PointerEventKind::Release { .. } => continue,
            };

            if redraw.is_needed() {
                self.request_redraw(qh, index);
            }
        }
    }
}

impl BackgroundEffectHandler for App {
    fn background_effect_state(&mut self) -> &mut BackgroundEffectState {
        &mut self.background_effect
    }

    fn update_capabilities(&mut self) {
        if !self.blur {
            return;
        }

        let blurs = self
            .background_effect
            .capabilities()
            .is_some_and(|caps| caps.contains(ext_background_effect_manager_v1::Capability::Blur));

        if !blurs {
            log::warn!("the compositor is not advertising blur, the selection will not be blurred");
        }
    }
}

impl ShmHandler for App {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

delegate_registry!(App);
delegate_dispatch2!(App);

impl ProvidesRegistryState for App {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }

    registry_handlers![OutputState, SeatState];
}

pub fn run(config: Config) -> Result<()> {
    let conn = Connection::connect_to_env()
        .context("could not connect to a wayland compositor, is WAYLAND_DISPLAY set?")?;

    let (globals, mut event_queue) =
        registry_queue_init(&conn).context("failed to enumerate wayland globals")?;
    let qh = event_queue.handle();

    let mut app = App::new(&globals, &qh, config)?;

    event_queue
        .roundtrip(&mut app)
        .context("initial roundtrip failed")?;

    if app.monitors.is_empty() {
        anyhow::bail!("no outputs available");
    }

    while !app.should_exit() {
        event_queue
            .blocking_dispatch(&mut app)
            .context("wayland dispatch failed")?;
    }

    Ok(())
}
