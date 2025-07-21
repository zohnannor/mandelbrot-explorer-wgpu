use std::{sync::Arc, time::Instant};

use wgpu::{
    BackendOptions, Backends, BindGroup, BindGroupDescriptor, BindGroupEntry,
    BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingType, Buffer,
    BufferBindingType, BufferDescriptor, BufferUsages, ColorTargetState,
    ColorWrites, CommandEncoderDescriptor, CreateSurfaceError, Device,
    DeviceDescriptor, Features, FragmentState, Instance, InstanceDescriptor,
    InstanceFlags, MemoryBudgetThresholds, MultisampleState, Operations,
    PipelineCompilationOptions, PipelineLayoutDescriptor, PrimitiveState,
    Queue, RenderPassColorAttachment, RenderPassDescriptor, RenderPipeline,
    RenderPipelineDescriptor, RequestAdapterError, RequestAdapterOptions,
    RequestDeviceError, ShaderStages, Surface, SurfaceConfiguration,
    SurfaceError, TextureViewDescriptor, VertexState, include_wgsl,
};
use winit::{
    application::ApplicationHandler,
    dpi::{PhysicalPosition, PhysicalSize},
    event::{
        ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent,
    },
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::Window,
};

#[allow(clippy::enum_variant_names)]
#[derive(Debug, thiserror::Error)]
enum Error {
    #[error("Failed to create surface: {0}")]
    CreateSurfaceError(#[from] CreateSurfaceError),

    #[error("Failed to create texture: {0}")]
    SurfaceError(#[from] SurfaceError),

    #[error("Failed to request adapter: {0}")]
    RequestAdapterError(#[from] RequestAdapterError),

    #[error("Failed to request device from an adapter: {0}")]
    RequestDeviceError(#[from] RequestDeviceError),

    #[error("Surface is not supported by current adapter")]
    SurfaceIsNotSupportedByAdapter,
}

/// Represents the uniform buffer data. Matches the `struct Uniforms` in the
/// shader.
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
struct Uniforms {
    /// Window width and height.
    resolution: [f64; 2],
    /// Time in seconds since startup.
    time: f64,
    /// Number of zoom-ins. The lower the number, the deeper the zoom.
    zooms: f64,
    /// Translation of the center of the coordinate system from 0+0i.
    offset: [f64; 2],
    /// Current mouse position, normalized to the range [-1, 1].
    mouse_position: [f64; 2],
    /// Whether we are currently rendering the Mandelbrot set or the Julia set.
    is_mandelbrot: f32,
    /// Whether should we rotate the colors (creates a trippy rainbow effect).
    rotate_colors: f32,
    /// Maximum number of iterations to perform.
    max_iter: u32,
    _padding: u32,
}

const _: () = assert!(std::mem::size_of::<Uniforms>() % 16 == 0);

impl Default for Uniforms {
    fn default() -> Self {
        Self {
            resolution: Default::default(),
            time: Default::default(),
            zooms: 8.0,
            offset: [(0.25 - 2.0) / 2.0, 0.0],
            mouse_position: [0.0, 0.0],
            // offset: [-1.999_491_453_530_413, 0.0],
            is_mandelbrot: 1.0,
            rotate_colors: 1.0,
            max_iter: 1500,
            _padding: 0,
        }
    }
}

/// The data related to the current view in the window.
#[derive(Debug)]
struct View {
    /// Timer that starts when the window is created.
    time: Instant,
    /// Keyboard movement delta.
    movement_delta: (f64, f64),
    /// Whether the control key is pressed.
    ctrl_pressed: bool,
    /// Whether the mouse button is clicked.
    mouse_clicked: bool,
    /// Whether the window should be in fullscreen mode.
    fullscreen: bool,
    /// The current uniform buffer data, which is written to the GPU every
    /// [`AppState::update`].
    uniforms: Uniforms,
}

/// The state of the application with all the resources needed to render and
/// maintain the connection to the GPU.
#[derive(Debug)]
struct AppState {
    window: Arc<winit::window::Window>,
    surface: Surface<'static>,
    device: Device,
    queue: Queue,
    config: SurfaceConfiguration,
    render_pipeline: RenderPipeline,
    bind_group: BindGroup,
    buffer: Buffer,
    view: View,
}

impl AppState {
    /// Creates a new [`AppState`] using the given [`Window`] to initialize the
    /// [`Instance`].
    #[allow(clippy::too_many_lines, reason = "whatever")]
    async fn new(window: Arc<Window>) -> Result<Self, Error> {
        let window_size = window.inner_size();
        let instance = Instance::new(&InstanceDescriptor {
            backends: Backends::default(),
            flags: InstanceFlags::default(),
            memory_budget_thresholds: MemoryBudgetThresholds::default(),
            backend_options: BackendOptions::default(),
        });

        let surface = instance.create_surface(window.clone())?;

        let adapter = instance
            .request_adapter(&RequestAdapterOptions::default())
            .await?;
        let (device, queue) = adapter
            .request_device(&DeviceDescriptor {
                label: Some("Device"),
                required_features: Features::SHADER_F64
                    | Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES,
                ..Default::default()
            })
            .await?;

        let config = surface
            .get_default_config(&adapter, window_size.width, window_size.height)
            .ok_or(Error::SurfaceIsNotSupportedByAdapter)?;

        let shader = device.create_shader_module(include_wgsl!("shader.wgsl"));

        let buffer = device.create_buffer(&BufferDescriptor {
            label: Some("Uniforms Buffer"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout =
            device.create_bind_group_layout(&BindGroupLayoutDescriptor {
                label: Some("Bind Group Layout"),
                entries: &[BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::VERTEX_FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::default(),
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("Bind Group"),
            layout: &bind_group_layout,
            entries: &[BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });

        let render_pipeline_layout =
            device.create_pipeline_layout(&PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[&bind_group_layout],
                push_constant_ranges: &[],
            });

        let render_pipeline =
            device.create_render_pipeline(&RenderPipelineDescriptor {
                label: Some("Render Pipeline"),
                vertex: VertexState {
                    module: &shader,
                    entry_point: None,
                    compilation_options: PipelineCompilationOptions::default(),
                    buffers: &[],
                },
                fragment: Some(FragmentState {
                    module: &shader,
                    entry_point: None,
                    compilation_options: PipelineCompilationOptions::default(),
                    targets: &[Some(ColorTargetState {
                        format: config.format,
                        blend: None,
                        write_mask: ColorWrites::ALL,
                    })],
                }),
                layout: Some(&render_pipeline_layout),
                primitive: PrimitiveState::default(),
                depth_stencil: None,
                multisample: MultisampleState::default(),
                multiview: None,
                cache: None,
            });

        let mut state = Self {
            window,
            surface,
            device,
            queue,
            config,
            render_pipeline,
            bind_group,
            buffer,
            view: View {
                time: Instant::now(),
                uniforms: Uniforms::default(),
                movement_delta: (0.0, 0.0),
                ctrl_pressed: false,
                mouse_clicked: false,
                fullscreen: false,
            },
        };

        state.resize(state.window.inner_size());
        // state.zoom(8.0);
        // state.translate((-1.999_491_453_530_413, 0.0));
        state.update();

        Ok(state)
    }

    /// Reconfigures the [`Surface`] to the new [`Window`] size.
    fn resize(&mut self, window_size: PhysicalSize<u32>) {
        if window_size.width > 0 && window_size.height > 0 {
            self.config.width = window_size.width;
            self.config.height = window_size.height;
            self.surface.configure(&self.device, &self.config);
        }
    }

    /// Handles the [`WindowEvent`]s user inputs and updates the [`View`] and
    /// [`Uniforms`] data. Only expects [`KeyboardInput`], [`CursorMoved`],
    /// [`MouseWheel`], [`MouseInput`] and [`ModifiersChanged`] events.
    ///
    /// [`KeyboardInput`]: WindowEvent::KeyboardInput
    /// [`CursorMoved`]: WindowEvent::CursorMoved
    /// [`MouseWheel`]: WindowEvent::MouseWheel
    /// [`MouseInput`]: WindowEvent::MouseInput
    /// [`ModifiersChanged`]: WindowEvent::ModifiersChanged
    #[allow(clippy::needless_pass_by_value, reason = "clippy false positive")]
    fn input(&mut self, event: WindowEvent) {
        match event {
            WindowEvent::KeyboardInput {
                device_id: _,
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(key),
                        state,
                        repeat: false,
                        ..
                    },
                is_synthetic: false,
            } => {
                let step = (0.005 * self.compute_zoom()).max(f64::EPSILON);
                let sign = if state == ElementState::Pressed {
                    1.0
                } else {
                    -1.0
                };
                // Bool cannot be used in a `Uniforms` field :(
                let toggle_f32 = |prop: &mut f32| *prop = (*prop - 1.0).abs();
                let (dx, dy) = &mut self.view.movement_delta;
                match (key, state) {
                    // Update delta by a fraction depending on current zoom.
                    (KeyCode::KeyA, _) => *dx -= sign * step,
                    (KeyCode::KeyD, _) => *dx += sign * step,
                    (KeyCode::KeyW, _) => *dy += sign * step,
                    (KeyCode::KeyS, _) => *dy -= sign * step,
                    (KeyCode::Space, ElementState::Pressed) => {
                        toggle_f32(&mut self.uniforms_mut().is_mandelbrot);
                    }
                    (KeyCode::KeyQ, ElementState::Pressed) => {
                        toggle_f32(&mut self.uniforms_mut().rotate_colors);
                    }
                    (KeyCode::Comma, ElementState::Pressed)
                        if self.uniforms().max_iter > 100 =>
                    {
                        self.uniforms_mut().max_iter -= 100;
                        self.update();
                    }
                    (KeyCode::Period, ElementState::Pressed)
                        if self.uniforms().max_iter < u32::MAX / 10 =>
                    {
                        self.uniforms_mut().max_iter += 100;
                        self.update();
                    }
                    (KeyCode::KeyR, ElementState::Pressed) => {
                        self.view.uniforms = Uniforms::default();
                    }
                    (KeyCode::F11, ElementState::Pressed) => {
                        self.view.fullscreen = !self.view.fullscreen;
                        self.window.set_fullscreen(
                            self.view.fullscreen.then_some(
                                winit::window::Fullscreen::Borderless(None),
                            ),
                        );
                    }
                    _ => {}
                }
            }
            WindowEvent::KeyboardInput { .. } => {
                // ignore all others
            }
            WindowEvent::CursorMoved {
                device_id: _,
                position,
            } => {
                let (x0, y0) = self.mouse_coords();
                self.move_mouse(position);
                let (x1, y1) = self.mouse_coords();
                let delta = (x0 - x1, y0 - y1);

                if self.view.mouse_clicked {
                    self.translate(delta);
                }
            }
            WindowEvent::MouseWheel {
                delta: MouseScrollDelta::LineDelta(_, y),
                ..
            } => {
                if self.view.ctrl_pressed {
                    self.zoom((-y).into());
                } else {
                    self.mouse_zoom((-y).into());
                }
            }
            WindowEvent::MouseInput {
                device_id: _,
                state,
                button: MouseButton::Left,
            } => self.view.mouse_clicked = state.is_pressed(),
            WindowEvent::ModifiersChanged(modifiers) => {
                self.view.ctrl_pressed = modifiers.state().control_key();
            }
            _ => unreachable!("unexpected event"),
        }
    }

    /// Translates the center of the coordinate system by the given delta.
    fn translate(&mut self, delta: (f64, f64)) {
        let (x, y) = delta;
        self.uniforms_mut().offset[0] += x;
        self.uniforms_mut().offset[1] += y;
    }

    /// Zooms in or out by the given delta. Recalculates the zoom factor and
    /// updates [`Uniforms::zooms`].
    fn zoom(&mut self, delta: f64) {
        self.uniforms_mut().zooms += delta;
        // The bounds are chosen so that we don't zoom in too much and distort
        // the view because of floating point errors.
        self.uniforms_mut().zooms = self.uniforms().zooms.clamp(-314.0, 42.0);

        // Without epsilon, we wouln't be able to move on extreme zoom-ins.
        let step = (0.005 * self.compute_zoom()).max(f64::EPSILON);
        let (dx, dy) = &mut self.view.movement_delta;
        if *dx != 0.0 {
            *dx = dx.signum() * step;
        }
        if *dy != 0.0 {
            *dy = dy.signum() * step;
        }
    }

    /// Zooms in on mouse position.
    fn mouse_zoom(&mut self, delta: f64) {
        let (x, y) = self.mouse_coords();
        self.zoom(delta);
        let (new_x, new_y) = self.mouse_coords();
        self.translate((x - new_x, y - new_y));
    }

    /// Updates the [`Uniforms::mouse_position`] to the mouse position,
    /// normalized to the range [-1, 1] in the window space.
    fn move_mouse(&mut self, position: PhysicalPosition<f64>) {
        let (x, y): (f64, f64) = position.into();
        let (w, h): (f64, f64) = self.window.inner_size().into();
        let aspect = w / h;
        let nx = (x / w).mul_add(2.0, -1.0);
        let ny = (y / h).mul_add(2.0, -1.0) / aspect;
        self.uniforms_mut().mouse_position = [nx, ny];
    }

    /// Returns the current mouse coordinates in the complex plane.
    #[must_use]
    fn mouse_coords(&self) -> (f64, f64) {
        let (mx, my) = self.uniforms().mouse_position.into();
        let (ox, oy) = self.uniforms().offset.into();
        let zoom = self.compute_zoom();
        (mx.mul_add(zoom, ox), (-my).mul_add(zoom, oy))
    }

    /// Computes the exponential zoom factor.
    #[must_use]
    fn compute_zoom(&self) -> f64 {
        (self.uniforms().zooms / 10.0).exp()
    }

    /// Returns a reference to the [`Uniforms`].
    #[must_use]
    const fn uniforms(&self) -> &Uniforms {
        &self.view.uniforms
    }

    /// Returns a mutable reference to the [`Uniforms`].
    #[must_use]
    const fn uniforms_mut(&mut self) -> &mut Uniforms {
        &mut self.view.uniforms
    }

    /// Updates the [`Uniforms`] and writes them to the GPU. Also updates the
    /// window title to show the current zoom, center and mouse position.
    fn update(&mut self) {
        let window_size = self.window.inner_size();

        self.uniforms_mut().time = self.view.time.elapsed().as_secs_f64();
        self.uniforms_mut().resolution = window_size.into();
        self.translate(self.view.movement_delta);

        self.queue.write_buffer(
            &self.buffer,
            0,
            bytemuck::cast_slice(&[*self.uniforms()]),
        );

        let max_iter = self.uniforms().max_iter;
        let [center_x, center_y] = self.uniforms().offset;
        let (mouse_x, mouse_y) = self.mouse_coords();
        let prec = 20;
        let format = |x: f64, i: bool| {
            format!("{x:.prec$}{i}", i = if i { "i" } else { "" })
        };
        self.window.set_title(&format!(
            "Mandelbrot \
             | Zoom = x{zoom:prec$} \
             | Max Iter = {max_iter} \
             | Center = {re1:>prec$}{sign1}{im1:<prec$} \
             | Mouse = {re2:>prec$}{sign2}{im2:<prec$}",
            zoom = format(self.compute_zoom().recip(), false)
                .trim_end_matches('0'),
            re1 = format(center_x, false).trim_end_matches('0'),
            im1 = format(center_y, true).trim_end_matches('0'),
            sign1 = if center_y >= 0.0 { "+" } else { "" },
            re2 = format(mouse_x, false).trim_end_matches('0'),
            im2 = format(mouse_y, true).trim_end_matches('0'),
            sign2 = if mouse_y >= 0.0 { "+" } else { "" },
            prec = prec + 5
        ));
    }

    /// Renders the current frame to the window.
    fn render(&self) -> Result<(), SurfaceError> {
        let frame = self.surface.get_current_texture()?;
        let view = frame.texture.create_view(&TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor::default());

        let mut render_pass =
            encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: Operations::default(),
                    depth_slice: None,
                })],
                ..Default::default()
            });

        render_pass.set_pipeline(&self.render_pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.draw(0..3, 0..1);

        drop(render_pass);

        self.queue.submit([encoder.finish()]);
        frame.present();
        self.window.request_redraw();

        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct App {
    state: Option<AppState>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes().with_title("Mandelbrot"),
                )
                .expect("Failed to create window"),
        );

        let state = pollster::block_on(AppState::new(window))
            .expect("Failed to create state");
        self.state = Some(state);
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        let Some(state) = &mut self.state else { return };

        match event {
            WindowEvent::Resized(physical_size) => {
                state.resize(physical_size);
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(KeyCode::Escape),
                        state: ElementState::Pressed,
                        ..
                    },
                ..
            }
            | WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::KeyboardInput {
                device_id: _,
                event: _,
                is_synthetic: _,
            }
            | WindowEvent::CursorMoved {
                device_id: _,
                position: _,
            }
            | WindowEvent::MouseWheel {
                device_id: _,
                delta: _,
                phase: _,
            }
            | WindowEvent::MouseInput {
                device_id: _,
                state: _,
                button: _,
            }
            | WindowEvent::ModifiersChanged(_) => {
                state.input(event);
            }
            WindowEvent::RedrawRequested => {
                state.update();
                match state.render() {
                    Ok(()) => {}
                    Err(SurfaceError::Outdated | SurfaceError::Lost) => {
                        let window_size = state.window.inner_size();
                        state.resize(window_size);
                    }
                    Err(e) => eprintln!("Surface error: {e:?}"),
                }
            }
            _ => {}
        }
    }
}

fn main() {
    let event_loop = EventLoop::new().expect("Failed to create event loop");
    let _ = event_loop.run_app(&mut App::default());
}
