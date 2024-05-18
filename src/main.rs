use clap::Parser;
use env_logger;
use log::{debug, error, info, warn};
use std::error::Error;
use std::os::unix::io::AsFd;
use tempfile::tempfile;
use wayland_client::{
    protocol::{
        wl_buffer, wl_compositor, wl_keyboard, wl_output, wl_pointer, wl_registry, wl_seat, wl_shm,
        wl_shm_pool, wl_surface,
    },
    Connection, Dispatch, EventQueue, Proxy, QueueHandle,
};
use wayland_protocols::wp::{
    fractional_scale::v1::client::{
        wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1,
        wp_fractional_scale_v1::{self, WpFractionalScaleV1},
    },
    viewporter::{client::wp_viewport::WpViewport, client::wp_viewporter::WpViewporter},
};
use wayland_protocols_wlr::{
    layer_shell::v1::client::{
        zwlr_layer_shell_v1::{self, Layer},
        zwlr_layer_surface_v1::{self, Anchor, KeyboardInteractivity},
    },
    screencopy::v1::client::{
        zwlr_screencopy_frame_v1::{self, ZwlrScreencopyFrameV1},
        zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1,
    },
};
use xkbcommon::xkb;

#[derive(Default)]
struct AppData {
    compositor: Option<(wl_compositor::WlCompositor, u32)>,
    surface: Option<wl_surface::WlSurface>,
    output: Option<wl_output::WlOutput>,
    seat: Option<wl_seat::WlSeat>,
    pointer: Option<wl_pointer::WlPointer>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    context: Option<xkb::Context>,
    keymap: Option<xkb::Keymap>,
    kbstate: Option<xkb::State>,
    width: i32,
    height: i32,
    scale: u32,
    shm: Option<wl_shm::WlShm>,
    buffer: Option<wl_buffer::WlBuffer>,
    pool: Option<wl_shm_pool::WlShmPool>,
    fs_manager: Option<(WpFractionalScaleManagerV1, u32)>,
    viewporter: Option<WpViewporter>,
    viewport: Option<WpViewport>,
    screencopy_manager: Option<(ZwlrScreencopyManagerV1, u32)>,
    screencopy_frame: Option<ZwlrScreencopyFrameV1>,
    layer_shell: Option<(zwlr_layer_shell_v1::ZwlrLayerShellV1, u32)>,
    layer_surface: Option<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1>,
    hide_cursor: bool,
    exit: bool,
}

impl Dispatch<wl_registry::WlRegistry, ()> for AppData {
    fn event(
        state: &mut Self,
        proxy: &wl_registry::WlRegistry,
        event: <wl_registry::WlRegistry as wayland_client::Proxy>::Event,
        _data: &(),
        _connection: &wayland_client::Connection,
        queue_handle: &wayland_client::QueueHandle<Self>,
    ) {
        match event {
            wl_registry::Event::Global {
                name,
                interface,
                version,
            } => {
                debug!("| Received wl_registry::Event::Global: {interface} v{version}");
                if interface == wl_compositor::WlCompositor::interface().name
                    && state.compositor.is_none()
                {
                    // wl_compositor
                    info!("> Bound: {interface} v{version}");
                    let compositor: wl_compositor::WlCompositor =
                        proxy.bind(name, version, queue_handle, ());
                    state.surface = Some(compositor.create_surface(&queue_handle, ()));
                    state.compositor = Some((compositor, name));
                } else if interface == wl_output::WlOutput::interface().name
                    && state.output.is_none()
                {
                    // wl_output
                    info!("> Bound: {interface} v{version}");
                    state.output = Some(proxy.bind(name, version, queue_handle, ()));
                } else if interface == wl_seat::WlSeat::interface().name && state.seat.is_none() {
                    // wl_seat
                    info!("> Bound: {interface} v{version}");
                    let seat: wl_seat::WlSeat = proxy.bind(name, version, queue_handle, ());
                    state.pointer = Some(seat.get_pointer(queue_handle, ()));
                    state.keyboard = Some(seat.get_keyboard(queue_handle, ()));
                    state.seat = Some(seat);
                } else if interface == wl_shm::WlShm::interface().name && state.shm.is_none() {
                    // wl_shm
                    info!("> Bound: {interface} v{version}");
                    state.shm = Some(proxy.bind(name, version, queue_handle, ()));
                } else if interface == WpFractionalScaleManagerV1::interface().name
                    && state.fs_manager.is_none()
                {
                    // wp_fractional_scale_manager_v1
                    info!("> Bound: {interface} v{version}");
                    state.fs_manager = Some((proxy.bind(name, version, queue_handle, ()), name));
                } else if interface == WpViewporter::interface().name && state.viewporter.is_none()
                {
                    // wp_viewporter
                    state.viewporter = Some(proxy.bind(name, version, queue_handle, ()));
                } else if interface == ZwlrScreencopyManagerV1::interface().name
                    && state.screencopy_manager.is_none()
                {
                    // zwlr_screencopy_manager_v1
                    info!("> Bound: {interface} v{version}");
                    state.screencopy_manager =
                        Some((proxy.bind(name, version, queue_handle, ()), name));
                } else if interface == zwlr_layer_shell_v1::ZwlrLayerShellV1::interface().name
                    && state.layer_shell.is_none()
                {
                    // zwlr_layer_shell_v1
                    info!("> Bound: {interface} v{version}");
                    state.layer_shell = Some((proxy.bind(name, version, queue_handle, ()), name));
                };
            }
            wl_registry::Event::GlobalRemove { name } => {
                debug!("| Received wl_registry::Event::GlobalRemove");
                if let Some((_, compositor_name)) = &state.compositor {
                    if name == *compositor_name {
                        warn!("Compositor was removed");
                        state.compositor = None;
                        state.surface = None;
                    }
                } else if let Some((_, screencopymanager_name)) = &state.screencopy_manager {
                    if name == *screencopymanager_name {
                        warn!("ZwlrScreencopyManagerV1 was removed");
                        state.screencopy_manager = None;
                    }
                } else if let Some((_, layer_shell_name)) = &state.layer_shell {
                    if name == *layer_shell_name {
                        warn!("ZwlrLayerShellV1 was removed");
                        state.layer_shell = None;
                    }
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_output::WlOutput, ()> for AppData {
    fn event(
        state: &mut Self,
        _proxy: &wl_output::WlOutput,
        event: <wl_output::WlOutput as Proxy>::Event,
        _data: &(),
        _connection: &wayland_client::Connection,
        _queue_handle: &wayland_client::QueueHandle<Self>,
    ) {
        if let wl_output::Event::Mode {
            flags: _,
            width,
            height,
            refresh: _,
        } = event
        {
            debug!("| Received wl_output::Event::Mode");
            // describes an available output mode for the output
            state.width = width;
            state.height = height;
        };
    }
}

impl Dispatch<wl_surface::WlSurface, ()> for AppData {
    fn event(
        _state: &mut Self,
        _proxy: &wl_surface::WlSurface,
        _event: <wl_surface::WlSurface as Proxy>::Event,
        _data: &(),
        _connection: &wayland_client::Connection,
        _queue_handle: &wayland_client::QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for AppData {
    fn event(
        _state: &mut Self,
        _proxy: &wl_seat::WlSeat,
        _event: <wl_seat::WlSeat as Proxy>::Event,
        _data: &(),
        _connection: &wayland_client::Connection,
        _queue_handle: &wayland_client::QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_pointer::WlPointer, ()> for AppData {
    fn event(
        state: &mut Self,
        _proxy: &wl_pointer::WlPointer,
        event: <wl_pointer::WlPointer as Proxy>::Event,
        _data: &(),
        _connection: &wayland_client::Connection,
        _queue_handle: &wayland_client::QueueHandle<Self>,
    ) {
        match event {
            wl_pointer::Event::Button {
                state: button_state,
                ..
            } => {
                debug!("| Received wl_pointer::Event::Button");
                // pointer button event
                if button_state != wayland_client::WEnum::Value(wl_pointer::ButtonState::Released) {
                    return;
                }
                info!("Mouse button released - exiting...");
                state.exit = true;
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, ()> for AppData {
    fn event(
        state: &mut Self,
        _proxy: &wl_keyboard::WlKeyboard,
        event: <wl_keyboard::WlKeyboard as Proxy>::Event,
        _data: &(),
        _connection: &wayland_client::Connection,
        _queue_handle: &wayland_client::QueueHandle<Self>,
    ) {
        match event {
            wl_keyboard::Event::Keymap { format, fd, size } => {
                debug!("| Received wl_keyboard::Event::Keymap");
                // provides a file descriptor to the client which can be memory-mapped in read-only mode to provide a keyboard mapping description
                if format != wayland_client::WEnum::Value(wl_keyboard::KeymapFormat::XkbV1) {
                    error!("Could not recognize keyboard format");
                    return;
                }
                let Some(context) = &state.context else {
                    error!("No xkb Context loaded");
                    return;
                };

                let size = size as usize;
                let keymap = unsafe {
                    xkb::Keymap::new_from_fd(
                        context,
                        fd,
                        size - 1,
                        xkb::FORMAT_TEXT_V1,
                        xkb::COMPILE_NO_FLAGS,
                    )
                    .expect("Could not create xkb keymap")
                    .unwrap()
                };
                state.kbstate = Some(xkb::State::new(&keymap));
                state.keymap = Some(keymap);
            }
            wl_keyboard::Event::Key {
                key,
                state: key_state,
                ..
            } => {
                debug!("| Received wl_keyboard::Event::Key");
                // a 'key' is a platform-specific key code that can be interpreted by feeding it to the keyboard mapping
                if key_state != wayland_client::WEnum::Value(wl_keyboard::KeyState::Pressed) {
                    return;
                }
                debug!("Key pressed: {}", key);
                let Some(kbstate) = &state.kbstate else {
                    error!("No xkb State loaded");
                    return;
                };
                if xkb::State::key_get_one_sym(kbstate, xkb::Keycode::new(key + 8))
                    == xkb::Keysym::Escape
                {
                    info!("Escape pressed - exiting...");
                    state.exit = true;
                };
            }
            _ => (),
        }
    }
}

// has no events
impl Dispatch<wl_compositor::WlCompositor, ()> for AppData {
    fn event(
        _state: &mut Self,
        _proxy: &wl_compositor::WlCompositor,
        _event: <wl_compositor::WlCompositor as Proxy>::Event,
        _data: &(),
        _connection: &wayland_client::Connection,
        _queue_handle: &wayland_client::QueueHandle<Self>,
    ) {
    }
}

// has no events
impl Dispatch<wl_shm_pool::WlShmPool, ()> for AppData {
    fn event(
        _state: &mut Self,
        _proxy: &wl_shm_pool::WlShmPool,
        _event: <wl_shm_pool::WlShmPool as Proxy>::Event,
        _data: &(),
        _connection: &wayland_client::Connection,
        _queue_handle: &wayland_client::QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_shm::WlShm, ()> for AppData {
    fn event(
        _state: &mut Self,
        _proxy: &wl_shm::WlShm,
        _event: <wl_shm::WlShm as Proxy>::Event,
        _data: &(),
        _connection: &wayland_client::Connection,
        _queue_handle: &wayland_client::QueueHandle<Self>,
    ) {
        // if let wl_shm::Event::Format {format} = event {
        //     debug!("| Received wl_shm::Event::Format");
        //     // informs client about a valid pixel format that can be used for buffers
        //     state.format = Some(format.into_result().expect("Unexpected format"));
        // };
    }
}

impl Dispatch<wl_buffer::WlBuffer, ()> for AppData {
    fn event(
        state: &mut Self,
        _proxy: &wl_buffer::WlBuffer,
        event: <wl_buffer::WlBuffer as Proxy>::Event,
        _data: &(),
        _connection: &wayland_client::Connection,
        _queue_handle: &wayland_client::QueueHandle<Self>,
    ) {
        if let wl_buffer::Event::Release = event {
            debug!("| Received wl_buffer::Event::Release");
            let Some(buffer) = &state.buffer else {
                error!("No WlBuffer loaded");
                return;
            };
            buffer.destroy();
        }
    }
}

// has no events
impl Dispatch<zwlr_layer_shell_v1::ZwlrLayerShellV1, ()> for AppData {
    fn event(
        _state: &mut Self,
        _proxy: &zwlr_layer_shell_v1::ZwlrLayerShellV1,
        _event: <zwlr_layer_shell_v1::ZwlrLayerShellV1 as Proxy>::Event,
        _data: &(),
        _connection: &wayland_client::Connection,
        _queue_handle: &wayland_client::QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, ()> for AppData {
    fn event(
        state: &mut Self,
        _proxy: &zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
        event: <zwlr_layer_surface_v1::ZwlrLayerSurfaceV1 as Proxy>::Event,
        _data: &(),
        _connection: &wayland_client::Connection,
        queue_handle: &wayland_client::QueueHandle<Self>,
    ) {
        match event {
            zwlr_layer_surface_v1::Event::Configure {
                serial,
                width: _,
                height: _,
            } => {
                debug!("| Received zwlr_layer_surface_v1::Event::Configure");
                let Some(layer_surface) = &state.layer_surface else {
                    error!("No ZwlrLayerSurfaceV1 loaded");
                    return;
                };
                // acknowledge the Configure event
                layer_surface.ack_configure(serial);

                let Some(output) = &state.output else {
                    error!("No WlOutput loaded");
                    return;
                };
                let Some(shm) = &state.shm else {
                    error!("No WlShm loaded");
                    return;
                };
                let Some((screencopy_manager, _)) = &state.screencopy_manager else {
                    error!("No ZwlrScreencopyFrameV1 loaded");
                    return;
                };

                // create pool
                let tmp = tempfile().ok().expect("Unable to create tempfile");
                let pool_size = state.height * state.width * 4; // height * width * 4 -> total size of the pool
                tmp.set_len(pool_size as u64).unwrap();
                let pool: wl_shm_pool::WlShmPool =
                    wl_shm::WlShm::create_pool(&shm, tmp.as_fd(), pool_size, &queue_handle, ());

                // create screencopyframe from output
                let screencopy_frame = screencopy_manager.capture_output(
                    !state.hide_cursor as i32,
                    &output,
                    &queue_handle,
                    (),
                );
                state.screencopy_frame = Some(screencopy_frame);
                state.pool = Some(pool);
            }
            zwlr_layer_surface_v1::Event::Closed => {
                let Some(layer_surface) = &state.layer_surface else {
                    error!("No ZwlrLayerSurfaceV1 loaded");
                    return;
                };
                layer_surface.destroy();
            }
            _ => (),
        }
    }
}

// has no events
impl Dispatch<ZwlrScreencopyManagerV1, ()> for AppData {
    fn event(
        _state: &mut Self,
        _proxy: &ZwlrScreencopyManagerV1,
        _event: <ZwlrScreencopyManagerV1 as Proxy>::Event,
        _data: &(),
        _connection: &wayland_client::Connection,
        _queue_handle: &wayland_client::QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrScreencopyFrameV1, ()> for AppData {
    fn event(
        state: &mut Self,
        _proxy: &ZwlrScreencopyFrameV1,
        event: <ZwlrScreencopyFrameV1 as Proxy>::Event,
        _data: &(),
        _connection: &wayland_client::Connection,
        queue_handle: &wayland_client::QueueHandle<Self>,
    ) {
        match event {
            zwlr_screencopy_frame_v1::Event::Buffer {
                format,
                width,
                height,
                stride,
            } => {
                debug!("| Received zwlr_screencopy_frame_v1::Event::Buffer");
                // provides information about wl_shm buffer parameters that need to be used for this frame
                // sent once after the frame is created if wl_shm buffers are supported
                let Some(pool) = &state.pool else {
                    error!("No WlShmPool loaded");
                    return;
                };

                // catch reported buffer type & create buffer
                let buffer: wl_buffer::WlBuffer = pool.create_buffer(
                    0, // buffer can take up the whole pool -> offset 0
                    width as i32,
                    height as i32,
                    stride as i32,
                    format.into_result().expect("Unsupported format"),
                    &queue_handle,
                    (),
                );
                state.buffer = Some(buffer);
            }
            zwlr_screencopy_frame_v1::Event::BufferDone { .. } => {
                debug!("| Received zwlr_screencopy_frame_v1::Event::BufferDone");
                // all buffer types are reported, proceed to send copy request
                // after copy -> wait for Event::Ready
                let Some(screencopy_frame) = &state.screencopy_frame else {
                    error!("No WlScreencopyFrameV1 loaded");
                    return;
                };
                let Some(buffer) = &state.buffer else {
                    error!("No WlBuffer loaded");
                    return;
                };
                // copy frame to buffer, sends Ready when successful
                screencopy_frame.copy(&buffer);
            }
            zwlr_screencopy_frame_v1::Event::Ready { .. } => {
                debug!("| Received zwlr_screencopy_frame_v1::Event::Ready");
                // copy done, frame is available for reading
                let Some(screencopy_frame) = &state.screencopy_frame else {
                    error!("No WlScreencopyFrameV1 loaded");
                    return;
                };
                let Some(surface) = &state.surface else {
                    error!("No WlSurface loaded");
                    return;
                };
                let Some(pool) = &state.pool else {
                    error!("No WlShmPool loaded");
                    return;
                };
                let Some(buffer) = &state.buffer else {
                    error!("No WlBuffer loaded");
                    return;
                };
                // attach buffer to surface
                surface.attach(Some(&buffer), 0, 0);
                surface.set_buffer_scale(1);
                //surface.damage(0, 0, width, height);
                surface.commit();

                // clean up screencopy_frame & pool
                screencopy_frame.destroy();
                pool.destroy();
            }
            zwlr_screencopy_frame_v1::Event::Failed => {
                debug!("| Received zwlr_screencopy_frame_v1::Event::Failed");
                error!("Failed to get a screencopyframe");
                // TODO exit here
            }
            _ => (),
        }
    }
}

// has no events
impl Dispatch<WpFractionalScaleManagerV1, ()> for AppData {
    fn event(
        _state: &mut Self,
        _proxy: &WpFractionalScaleManagerV1,
        _event: <WpFractionalScaleManagerV1 as Proxy>::Event,
        _data: &(),
        _connection: &wayland_client::Connection,
        _queue_handle: &wayland_client::QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WpFractionalScaleV1, ()> for AppData {
    fn event(
        state: &mut Self,
        _proxy: &WpFractionalScaleV1,
        event: <WpFractionalScaleV1 as Proxy>::Event,
        _data: &(),
        _connection: &wayland_client::Connection,
        _queue_handle: &wayland_client::QueueHandle<Self>,
    ) {
        match event {
            wp_fractional_scale_v1::Event::PreferredScale { scale, .. } => {
                // notifies of a new preferred scale for this surface
                debug!("| Received wp_fractional_scale_v1::Event::PreferredScale");
                debug!("SCALE: {}", scale);
                state.scale = scale;

                let Some(layer_surface) = &state.layer_surface else {
                    error!("No ZwlrLayerSurfaceV1 loaded");
                    return;
                };
                let Some(viewport) = &state.viewport else {
                    error!("No WpViewPortV1 loaded");
                    return;
                };

                // set source & destination rectangle
                viewport.set_source(0.0, 0.0, state.width as f64, state.height as f64);
                viewport.set_destination(
                    (state.width as f64 / (state.scale as f64 / 120.0)) as i32,
                    (state.height as f64 / (state.scale as f64 / 120.0)) as i32,
                );

                // update layer surface size every time the preferred scale changes
                layer_surface.set_size(
                    (state.width as f64 / (state.scale as f64 / 120.0)) as u32,
                    (state.height as f64 / (state.scale as f64 / 120.0)) as u32,
                );
            }
            _ => {}
        }
    }
}

// has no events
impl Dispatch<WpViewporter, ()> for AppData {
    fn event(
        _state: &mut Self,
        _proxy: &WpViewporter,
        _event: <WpViewporter as Proxy>::Event,
        _data: &(),
        _connection: &wayland_client::Connection,
        _queue_handle: &wayland_client::QueueHandle<Self>,
    ) {
    }
}

// has no events
impl Dispatch<WpViewport, ()> for AppData {
    fn event(
        _state: &mut Self,
        _proxy: &WpViewport,
        _event: <WpViewport as Proxy>::Event,
        _data: &(),
        _connection: &wayland_client::Connection,
        _queue_handle: &wayland_client::QueueHandle<Self>,
    ) {
    }
}

struct ScreenFreezer {
    event_queue: EventQueue<AppData>,
    queue_handle: QueueHandle<AppData>,
    state: AppData,
}

impl ScreenFreezer {
    fn new(hide_cursor: bool) -> Result<Self, Box<dyn Error>> {
        let connection = Connection::connect_to_env().unwrap();
        let mut event_queue = connection.new_event_queue();
        let queue_handle = event_queue.handle();
        let display = connection.display();
        let _registry = display.get_registry(&queue_handle, ());
        let mut state = AppData::default();
        state.hide_cursor = hide_cursor;

        event_queue.roundtrip(&mut state).unwrap();

        state.context = Some(xkb::Context::new(xkb::CONTEXT_NO_FLAGS));

        // block to receive wl_keyboard::Event::Keymap & wl_output::Event::Mode
        event_queue.blocking_dispatch(&mut state).unwrap();

        Ok(Self {
            event_queue,
            queue_handle,
            state,
        })
    }
    pub fn freeze(&mut self) -> Result<(), Box<dyn Error>> {
        {
            let Some(output) = &self.state.output else {
                error!("No WlOutput loaded");
                return Ok(());
            };
            let Some(surface) = &self.state.surface else {
                error!("No WlSurface loaded");
                return Ok(());
            };
            let Some(viewporter) = &self.state.viewporter else {
                error!("No WpViewPorterV1 loaded");
                return Ok(());
            };
            let Some((fs_manager, _)) = &self.state.fs_manager else {
                error!("No WpFractionalScaleManagerV1 loaded");
                return Ok(());
            };
            let Some((layer_shell, _)) = &self.state.layer_shell else {
                error!("No ZwlrLayerShellV1 loaded");
                return Ok(());
            };

            // create a layer surface & store it for later
            self.state.layer_surface = Some(zwlr_layer_shell_v1::ZwlrLayerShellV1::get_layer_surface(
                layer_shell,
                &surface,
                Some(&output),
                Layer::Overlay,
                "wayfreeze".to_string(),
                &self.queue_handle,
                (),
            ));

            // instantiates an interface extension for the wl_surface to crop & scale its content
            self.state.viewport = Some(viewporter.get_viewport(&surface, &self.queue_handle, ()));

            // create add-on object for the surface so that compositor can request fractional scales, will send preferred_scale event
            fs_manager.get_fractional_scale(&surface, &self.queue_handle, ());
            // wait for the PreferredScale event
            self.event_queue.blocking_dispatch(&mut self.state).unwrap();
            
        }
        
        let Some(surface) = &self.state.surface else {
            error!("No WlSurface loaded");
            return Ok(());
        };
        let Some(layer_surface) = &self.state.layer_surface else {
            error!("No ZwlrLayerSurfaceV1 loaded");
            return Ok(());
        };

        // commit viewport set_source & set_destination in response to PreferredScale event
        surface.commit();

        debug!("SETTING SCALE: {}",self.state.scale);
        // scale will always be 1 here, later PreferredScale event can update it
        layer_surface.set_size(
            (self.state.width as f64 / (self.state.scale as f64 / 120.0)) as u32,
            (self.state.height as f64 / (self.state.scale as f64 / 120.0)) as u32,
        );
        layer_surface.set_anchor(Anchor::Top | Anchor::Right | Anchor::Bottom | Anchor::Left);
        layer_surface.set_exclusive_zone(-1); // extend surface to anchored edges
        layer_surface.set_keyboard_interactivity(KeyboardInteractivity::Exclusive);
        surface.commit(); // commit before attaching any buffers

        info!("Screen frozen");

        loop {
            self.event_queue.blocking_dispatch(&mut self.state).unwrap();
            if self.state.exit {
                std::process::exit(0);
            }
        }
    }
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Hide cursor when freezing the screen.
    #[arg(long, default_value_t = false)]
    hide_cursor: bool,
}

fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();
    let args = Args::parse();
    info!(target: "main", "Parsed arguments");

    match ScreenFreezer::new(args.hide_cursor) {
        Ok(mut sf) => sf.freeze().unwrap(),
        Err(e) => panic!("Could not create ScreenFreezer: {}", e),
    };

    Ok(())
}
