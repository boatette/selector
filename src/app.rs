use anyhow::{Context, Result};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState, FrameCallbackData},
    delegate_dispatch2, delegate_registry,
    output::{OutputHandler, OutputState},
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

// some compositors key window rules off this
const NAMESPACE: &str = "selector";

// one output's layer surface plus everything needed to paint it
// each owns its own drag state, a selection belongs to the monitor it began on
struct Monitor {
    output: wl_output::WlOutput,
    layer: LayerSurface,
    pool: SlotPool,
    selection: Selection,
    width: u32,
    height: u32,
    // set once the compositor has sent its first configure
    configured: bool,
    // a redraw is wanted but we are waiting on a frame callback
    dirty: bool,
    frame_pending: bool,
}

impl Monitor {
    fn owns(&self, surface: &wl_surface::WlSurface) -> bool {
        self.layer.wl_surface() == surface
    }
}

pub struct App {
    registry_state: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,
    shm: Shm,
    compositor: CompositorState,
    layer_shell: LayerShell,

    config: Config,
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

        Ok(Self {
            registry_state: RegistryState::new(globals),
            seat_state: SeatState::new(globals, qh),
            output_state: OutputState::new(globals, qh),
            shm,
            compositor,
            layer_shell,
            config,
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

        // anchoring to all four edges with a zero size asks the compositor for the full output,
        // the negative exclusive zone keeps panels from shrinking us out of the space they reserve
        layer.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
        layer.set_size(0, 0);
        layer.set_exclusive_zone(-1);
        // set explicitly rather than relying on the protocol default, selector never wants focus and must not steal it from the window underneath
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);

        // a layer surface is only mapped after an initial commit with no buffer, the compositor answers with the configure that sizes it
        layer.commit();

        // the pool grows on demand, so the initial size only needs to be non-zero
        let pool = match SlotPool::new(BYTES_PER_PIXEL, &self.shm) {
            Ok(pool) => pool,
            Err(err) => {
                log::error!("failed to create shm pool for output {name}: {err}");
                return;
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
        });
    }

    fn remove_monitor(&mut self, output: &wl_output::WlOutput) {
        self.monitors.retain(|m| &m.output != output);
    }

    fn monitor_index(&self, surface: &wl_surface::WlSurface) -> Option<usize> {
        self.monitors.iter().position(|m| m.owns(surface))
    }

    // draws immediately unless the compositor still owes us a frame callback
    fn request_redraw(&mut self, qh: &QueueHandle<Self>, index: usize) {
        let monitor = &mut self.monitors[index];
        monitor.dirty = true;
        if !monitor.frame_pending {
            self.draw(qh, index);
        }
    }

    fn draw(&mut self, qh: &QueueHandle<Self>, index: usize) {
        let config = &self.config;
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
        monitor.layer.commit();
    }

    // nothing consumes the rectangle yet, this is the seam where selection results will be reported
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

        // losing every surface leaves nothing to drag on
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

        // the first buffer maps the surface even though it is fully transparent
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
                    // track the release position first, compositors may fold the final motion into the release event
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
                // a press of any other button abandons the drag rather than leaving a stuck rectangle behind
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

    // announces outputs, which is what creates our surfaces
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
