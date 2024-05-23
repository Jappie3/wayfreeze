use clap::Parser;
use env_logger;
use log::{debug, error, info, warn};
use std::collections::HashMap;
use std::error::Error;
use std::hash::Hash;
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

fn vec_insert<T, V>(state_hm: &mut Option<HashMap<T, V>>, key: T, value: V)
where
    T: Eq + Hash,
    V: Clone,
{
    match state_hm {
        Some(hm) => {
            hm.insert(key, value.clone());
        }
        None => {
            let mut new_hm = HashMap::new();
            new_hm.insert(key, value.clone());
            *state_hm = Some(new_hm);
        }
    }
}

#[derive(Default)]
struct AppData {
    compositor: Option<(wl_compositor::WlCompositor, u32)>,
    // store all outputs in a vector
    outputs: Option<Vec<wl_output::WlOutput>>,
    // key is the position of the corresponding output in the above vector
    surfaces: Option<HashMap<i64, wl_surface::WlSurface>>,
    widths: Option<HashMap<i64, i32>>,
    heights: Option<HashMap<i64, i32>>,
    scales: Option<HashMap<i64, i32>>,
    viewports: Option<HashMap<i64, WpViewport>>,
    shm_pools: Option<HashMap<i64, wl_shm_pool::WlShmPool>>,
    buffers: Option<HashMap<i64, wl_buffer::WlBuffer>>,
    layer_surfaces: Option<HashMap<i64, zwlr_layer_surface_v1::ZwlrLayerSurfaceV1>>,
    screencopy_frames: Option<HashMap<i64, ZwlrScreencopyFrameV1>>,
    seat: Option<wl_seat::WlSeat>,
    pointer: Option<wl_pointer::WlPointer>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    context: Option<xkb::Context>,
    keymap: Option<xkb::Keymap>,
    kbstate: Option<xkb::State>,
    fs_manager: Option<(WpFractionalScaleManagerV1, u32)>,
    viewporter: Option<WpViewporter>,
    shm: Option<wl_shm::WlShm>,
    screencopy_manager: Option<(ZwlrScreencopyManagerV1, u32)>,
    layer_shell: Option<(zwlr_layer_shell_v1::ZwlrLayerShellV1, u32)>,
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
                    state.compositor = Some((proxy.bind(name, version, queue_handle, ()), name));
                } else if interface == wl_output::WlOutput::interface().name {
                    // wl_output
                    info!("> Bound: {interface} v{version}");
                    // TODO consider doing this in the Dispatch for wl_output after the Done event
                    match &mut state.outputs {
                        Some(vec) => {
                            // this is not the first monitor
                            vec.push(proxy.bind(name, version, queue_handle, vec.len()))
                        }
                        None => {
                            // vec doesn't exist -> first monitor, index is 0
                            let mut new_vec = Vec::new();
                            new_vec.push(proxy.bind(name, version, queue_handle, 0));
                            state.outputs = Some(new_vec);
                        }
                    }
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

impl Dispatch<wl_output::WlOutput, usize> for AppData {
    fn event(
        state: &mut Self,
        _proxy: &wl_output::WlOutput,
        event: <wl_output::WlOutput as Proxy>::Event,
        data: &usize,
        _connection: &wayland_client::Connection,
        queue_handle: &wayland_client::QueueHandle<Self>,
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

            // save the width & height of this output under the same key as this output's index in the vector
            vec_insert(&mut state.widths, *data as i64, width);
            vec_insert(&mut state.heights, *data as i64, height);

            // create a surface for this output & store it
            let Some((compositor, _)) = &state.compositor else {
                error!("No WlCompositor loaded");
                return;
            };
            vec_insert(
                &mut state.surfaces,
                *data as i64,
                compositor.create_surface(&queue_handle, ()),
            );
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
                info!("> Mouse button released - exiting...");
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
                debug!("| Key pressed: {}", key);
                let Some(kbstate) = &state.kbstate else {
                    error!("No xkb State loaded");
                    return;
                };
                if xkb::State::key_get_one_sym(kbstate, xkb::Keycode::new(key + 8))
                    == xkb::Keysym::Escape
                {
                    info!("> Escape pressed - exiting...");
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
        proxy: &wl_buffer::WlBuffer,
        event: <wl_buffer::WlBuffer as Proxy>::Event,
        _data: &(),
        _connection: &wayland_client::Connection,
        _queue_handle: &wayland_client::QueueHandle<Self>,
    ) {
        if let wl_buffer::Event::Release = event {
            debug!("| Received wl_buffer::Event::Release");
            proxy.destroy();
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

impl Dispatch<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, i64> for AppData {
    fn event(
        state: &mut Self,
        proxy: &zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
        event: <zwlr_layer_surface_v1::ZwlrLayerSurfaceV1 as Proxy>::Event,
        data: &i64,
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
                // acknowledge the Configure event
                proxy.ack_configure(serial);

                let Some(outputs) = &state.outputs else {
                    error!("No WlOutputs loaded");
                    return;
                };
                let Some((screencopy_manager, _)) = &state.screencopy_manager else {
                    error!("No ZwlrScreencopyManagerV1 loaded");
                    return;
                };
                let Some(shm) = &state.shm else {
                    error!("No WlShm loaded");
                    return;
                };
                let Some(widths) = &state.widths else {
                    error!("Could not load widths");
                    return;
                };
                let Some(heights) = &state.heights else {
                    error!("Could not load heights");
                    return;
                };

                // create pool
                let tmp = tempfile().ok().expect("Unable to create tempfile");
                let pool_size = heights[data] * widths[data] * 4; // height * width * 4 -> total size of the pool
                tmp.set_len(pool_size as u64).unwrap();
                let pool: wl_shm_pool::WlShmPool =
                    wl_shm::WlShm::create_pool(&shm, tmp.as_fd(), pool_size, &queue_handle, ());

                // create screencopyframe from output
                let screencopy_frame = screencopy_manager.capture_output(
                    !state.hide_cursor as i32,
                    &outputs[*data as usize],
                    &queue_handle,
                    *data,
                );
                vec_insert(&mut state.screencopy_frames, *data, screencopy_frame);
                vec_insert(&mut state.shm_pools, *data, pool);
            }
            zwlr_layer_surface_v1::Event::Closed => {
                proxy.destroy();
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

impl Dispatch<ZwlrScreencopyFrameV1, i64> for AppData {
    fn event(
        state: &mut Self,
        proxy: &ZwlrScreencopyFrameV1,
        event: <ZwlrScreencopyFrameV1 as Proxy>::Event,
        data: &i64,
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
                let Some(pools) = &state.shm_pools else {
                    error!("Could not load WlShmPools");
                    return;
                };

                // catch reported buffer type & create buffer
                let buffer: wl_buffer::WlBuffer = pools[data].create_buffer(
                    0, // buffer can take up the whole pool -> offset 0
                    width as i32,
                    height as i32,
                    stride as i32,
                    format.into_result().expect("Unsupported format"),
                    &queue_handle,
                    (),
                );
                vec_insert(&mut state.buffers, *data, buffer);
            }
            zwlr_screencopy_frame_v1::Event::BufferDone { .. } => {
                debug!("| Received zwlr_screencopy_frame_v1::Event::BufferDone");
                // all buffer types are reported, proceed to send copy request
                // after copy -> wait for Event::Ready
                let Some(buffer) = &state.buffers else {
                    error!("Could not load WlBuffers");
                    return;
                };
                // copy frame to buffer, sends Ready when successful
                proxy.copy(&buffer[data]);
            }
            zwlr_screencopy_frame_v1::Event::Ready { .. } => {
                debug!("| Received zwlr_screencopy_frame_v1::Event::Ready");
                // copy done, frame is available for reading
                let Some(surfaces) = &state.surfaces else {
                    error!("Could not load WlSurfaces");
                    return;
                };
                let Some(pools) = &state.shm_pools else {
                    error!("Could not load WlShmPools");
                    return;
                };
                let Some(buffers) = &state.buffers else {
                    error!("Could not load WlBuffers");
                    return;
                };

                // attach buffer to surface
                surfaces[data].attach(Some(&buffers[data]), 0, 0);
                surfaces[data].set_buffer_scale(1);
                //surface.damage(0, 0, width, height);
                surfaces[data].commit();

                // clean up screencopy_frame & pool
                proxy.destroy();
                pools[data].destroy();
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

impl Dispatch<WpFractionalScaleV1, i64> for AppData {
    fn event(
        state: &mut Self,
        _proxy: &WpFractionalScaleV1,
        event: <WpFractionalScaleV1 as Proxy>::Event,
        data: &i64,
        _connection: &wayland_client::Connection,
        _queue_handle: &wayland_client::QueueHandle<Self>,
    ) {
        match event {
            wp_fractional_scale_v1::Event::PreferredScale { scale } => {
                // notifies of a new preferred scale for this surface
                debug!("| Received wp_fractional_scale_v1::Event::PreferredScale");

                let Some(layer_surfaces) = &state.layer_surfaces else {
                    error!("No ZwlrLayerSurfaceV1 loaded");
                    return;
                };
                let Some(viewports) = &state.viewports else {
                    error!("No WpViewPortV1 loaded");
                    return;
                };
                let Some(widths) = &state.widths else {
                    error!("Could not load widths");
                    return;
                };
                let Some(heights) = &state.heights else {
                    error!("Could not load heights");
                    return;
                };

                // set source & destination rectangle
                viewports[data].set_source(0.0, 0.0, widths[data] as f64, heights[data] as f64);
                viewports[data].set_destination(
                    (widths[data] as f64 / (scale as f64 / 120.0)) as i32,
                    (heights[data] as f64 / (scale as f64 / 120.0)) as i32,
                );
                // update layer surface size every time the preferred scale changes
                layer_surfaces[data].set_size(
                    (widths[data] as f64 / (scale as f64 / 120.0)) as u32,
                    (heights[data] as f64 / (scale as f64 / 120.0)) as u32,
                );

                vec_insert(&mut state.scales, *data, scale as i32)
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
        info!("> Received all globals");

        state.context = Some(xkb::Context::new(xkb::CONTEXT_NO_FLAGS));

        Ok(Self {
            event_queue,
            queue_handle,
            state,
        })
    }
    pub fn freeze(&mut self) -> Result<(), Box<dyn Error>> {
        // check self.state.outputs
        match &self.state.outputs {
            // if the vector exists -> we're good, at least 1 output was found & bound to
            Some(vec) => {
                info!("> Bound to {} input(s)", vec.len())
            }
            None => {
                // no vector -> no outputs found
                error!("No outputs found - exiting...");
                self.state.exit = true;
            }
        }

        self.event_queue.blocking_dispatch(&mut self.state).unwrap();

        let Some(outputs) = &self.state.outputs else {
            return Ok(());
        };

        for i in 0..outputs.len() {
            {
                let i = i as i64;

                let Some(surfaces) = &self.state.surfaces else {
                    error!("No WlSurface loaded");
                    return Ok(());
                };

                let Some((layer_shell, _)) = &self.state.layer_shell else {
                    error!("No ZwlrLayerShellV1 loaded");
                    return Ok(());
                };
                let Some(outputs) = &self.state.outputs else {
                    return Ok(());
                };
                let output = &outputs[i as usize];

                // create a layer surface for the current output & its surface
                vec_insert(
                    &mut self.state.layer_surfaces,
                    i,
                    zwlr_layer_shell_v1::ZwlrLayerShellV1::get_layer_surface(
                        layer_shell,
                        &surfaces[&i],
                        Some(&output),
                        Layer::Overlay,
                        "wayfreeze".to_string(),
                        &self.queue_handle,
                        i,
                    ),
                );

                let Some(viewporter) = &self.state.viewporter else {
                    error!("No WpViewPorterV1 loaded");
                    return Ok(());
                };
                let Some((fs_manager, _)) = &self.state.fs_manager else {
                    error!("No WpFractionalScaleManagerV1 loaded");
                    return Ok(());
                };

                // instantiates an interface extension for the wl_surface to crop & scale its content
                vec_insert(
                    &mut self.state.viewports,
                    i,
                    viewporter.get_viewport(&surfaces[&i], &self.queue_handle, ()),
                );
                // create add-on object for the surface so that compositor can request fractional scales, will send preferred_scale event
                fs_manager.get_fractional_scale(&surfaces[&i], &self.queue_handle, i);

                // wait for the PreferredScale event
                self.event_queue.blocking_dispatch(&mut self.state).unwrap();

                let Some(layer_surfaces) = &self.state.layer_surfaces else {
                    error!("No ZwlrLayerSurfaceV1 loaded");
                    return Ok(());
                };
                let Some(widths) = &self.state.widths else {
                    error!("Could not load widths");
                    return Ok(());
                };
                let Some(heights) = &self.state.heights else {
                    error!("Could not load heights");
                    return Ok(());
                };
                let Some(scales) = &self.state.scales else {
                    error!("Could not load scales");
                    return Ok(());
                };

                // scale will always be 1 here, later PreferredScale event can update it
                layer_surfaces[&i].set_size(
                    (widths[&i] as f64 / (scales[&i] as f64 / 120.0)) as u32,
                    (heights[&i] as f64 / (scales[&i] as f64 / 120.0)) as u32,
                );
                layer_surfaces[&i]
                    .set_anchor(Anchor::Top | Anchor::Right | Anchor::Bottom | Anchor::Left);
                layer_surfaces[&i].set_exclusive_zone(-1); // extend surface to anchored edges
                layer_surfaces[&i].set_keyboard_interactivity(KeyboardInteractivity::Exclusive);

                let Some(surfaces) = &self.state.surfaces else {
                    error!("No WlSurface loaded");
                    return Ok(());
                };
                surfaces[&i].commit(); // commit before attaching any buffers
            }
        }

        info!("> Screen frozen");

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
