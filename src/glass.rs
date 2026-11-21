//! What stands behind a glass window frame, and the frame's own highlights.
//!
//! A see-through frame is only glass with the desktop's help. Three kinds of
//! compositor are told apart once per window:
//!
//! - one that paints enzUI's material itself (`enzui-material-v1`, the enzimOS
//!   shell): the frame is the framework's glass, sampled and lit there;
//! - one that blurs what is behind a region the window names
//!   (`ext-background-effect-v1`: Hyprland, KWin; or KWin's own
//!   `org_kde_kwin_blur`): the browser asks for the frame's shape and paints
//!   the tint and the light on the glass itself;
//! - one that does neither: the frame is painted the same, over a sharp
//!   desktop.
//!
//! No compositor hands a window the pixels behind it, so the light a pane of
//! glass bends cannot be drawn here: the edge light and the sheen are a
//! picture of glass, laid over whatever the compositor shows.

use std::any::Any;
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::{Duration, Instant};

use iced::widget::canvas;
use iced::{Color, Point, Rectangle, Size, Task};
use wayland_client::backend::ObjectId;
use wayland_client::protocol::wl_callback::{self, WlCallback};
use wayland_client::protocol::wl_compositor::WlCompositor;
use wayland_client::protocol::wl_region::WlRegion;
use wayland_client::protocol::wl_registry::{self, WlRegistry};
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle, WEnum, delegate_noop};
use wayland_protocols::ext::background_effect::v1::client::ext_background_effect_manager_v1::{
    self, ExtBackgroundEffectManagerV1,
};
use wayland_protocols::ext::background_effect::v1::client::ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1;
use wayland_protocols_plasma::blur::client::org_kde_kwin_blur::OrgKdeKwinBlur;
use wayland_protocols_plasma::blur::client::org_kde_kwin_blur_manager::OrgKdeKwinBlurManager;

/// What the compositor does behind this window's glass frame.
#[derive(Clone, Default)]
pub(crate) enum Backdrop {
    /// Not asked yet: no frame of this window has been glass.
    #[default]
    Unasked,
    /// Asked; the frame is painted as on a desktop without blur meanwhile.
    Asked,
    /// The compositor paints the framework's material behind the window.
    Material(enzui_widgets::desktop_glass::WindowGlass),
    /// The compositor blurs what is behind the region the window names.
    Blur(BlurBehind),
    /// The compositor offers neither.
    Plain,
}
impl std::fmt::Debug for Backdrop {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Unasked => "Unasked",
            Self::Asked => "Asked",
            Self::Material(_) => "Material",
            Self::Blur(_) => "Blur",
            Self::Plain => "Plain",
        })
    }
}
impl Backdrop {
    /// Whether the compositor paints the glass, light and all.
    pub(crate) fn is_material(&self) -> bool {
        matches!(self, Self::Material(_))
    }
    /// The glass binding the window's view is hosted in: none unless the
    /// compositor paints the material.
    pub(crate) fn window_glass(&self) -> enzui_widgets::desktop_glass::WindowGlass {
        match self {
            Self::Material(glass) => glass.clone(),
            _ => enzui_widgets::desktop_glass::WindowGlass::unavailable(),
        }
    }
}

/// Finds out what the compositor offers `window`.
pub(crate) fn bind(window: iced::window::Id) -> Task<Backdrop> {
    enzui_widgets::desktop_glass::bind(window).then(move |glass| {
        if glass.is_bound() {
            return Task::done(Backdrop::Material(glass));
        }
        iced::window::wayland_parent_export(window).then(|payload| {
            let payload = Mutex::new(payload);
            Task::future(async move {
                let payload = payload.lock().ok().and_then(|mut slot| slot.take());
                let (sender, receiver) = iced::futures::channel::oneshot::channel();
                // The round trip waits on the compositor; the interface does not.
                let _ = std::thread::Builder::new()
                    .name("enzim-blur-bind".into())
                    .spawn(move || {
                        let _ = sender.send(BlurBehind::from_payload(payload));
                    });
                match receiver.await {
                    Ok(Some(blur)) => Backdrop::Blur(blur),
                    _ => Backdrop::Plain,
                }
            })
        })
    })
}

/// Whose curve a window's corners are.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Corner {
    /// The frame's own arc: the blur reaches as far as the frame's paint.
    Own,
    /// The compositor's curve (see `crate::corners`), of this power in
    /// hundredths: 200 is a circle's arc. The compositor smooths the
    /// window's edge along it and draws blur under all of that edge, where
    /// the window's shadow should show through; the blur asked for
    /// therefore keeps a pixel inside the curve.
    Cut(i32),
}

/// The shape of a window's frame, in the surface's logical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Shape {
    pub(crate) width: i32,
    pub(crate) height: i32,
    pub(crate) radius: i32,
    pub(crate) corner: Corner,
}
impl Shape {
    /// A frame that rounds itself at `radius`.
    pub(crate) fn of(size: Size, radius: f32) -> Self {
        Self {
            width: size.width.round().max(0.0) as i32,
            height: size.height.round().max(0.0) as i32,
            radius: radius.round().max(0.0) as i32,
            corner: Corner::Own,
        }
    }
    /// A frame the compositor cuts at `rounding`.
    pub(crate) fn under(size: Size, rounding: crate::corners::Rounding) -> Self {
        Self {
            corner: Corner::Cut((rounding.power * 100.0).round() as i32),
            ..Self::of(size, rounding.radius)
        }
    }
    /// How far the shape's edge is in from the window's side on `row`,
    /// counted from the top, of a corner `radius` high.
    fn inset(self, radius: i32, row: i32) -> i32 {
        match self.corner {
            Corner::Own => {
                // How far the corner's arc has come in at the middle of this row.
                let rise = radius as f32 - row as f32 - 0.5;
                let reach = ((radius * radius) as f32 - rise * rise).max(0.0).sqrt();
                (radius as f32 - reach).ceil() as i32
            }
            Corner::Cut(power) => {
                // Only pixels that lie wholly inside the curve drawn a
                // pixel smaller: measured at the row's outer edge.
                let power = power as f32 / 100.0;
                let within = (radius - 1).max(0) as f32;
                let rise = (radius - row) as f32;
                if rise >= within {
                    return radius;
                }
                let reach = (within.powf(power) - rise.powf(power)).powf(power.recip());
                ((radius as f32 - reach).ceil() as i32).min(radius)
            }
        }
    }
    /// The rectangles (x, y, width, height) that cover the rounded shape:
    /// one row for each pixel of a corner's height, and the band between.
    /// A region has no curves of its own.
    pub(crate) fn rectangles(self) -> Vec<(i32, i32, i32, i32)> {
        let radius = self.radius.min(self.width / 2).min(self.height / 2).max(0);
        let mut rows = Vec::with_capacity(2 * radius as usize + 1);
        for row in 0..radius {
            let inset = self.inset(radius, row);
            let width = self.width - 2 * inset;
            if width > 0 {
                rows.push((inset, row, width, 1));
                rows.push((inset, self.height - 1 - row, width, 1));
            }
        }
        if self.height > 2 * radius && self.width > 0 {
            rows.push((0, radius, self.width, self.height - 2 * radius));
        }
        rows
    }
}

/// A window's request for blur behind its frame. Dropping the last clone
/// withdraws it.
#[derive(Clone)]
pub(crate) struct BlurBehind(Arc<Bound>);

struct Bound {
    connection: Connection,
    compositor: WlCompositor,
    effect: Effect,
    surface: WlSurface,
    handle: QueueHandle<Found>,
    /// Kept so the objects above have a queue for as long as they live.
    _queue: Mutex<EventQueue<Found>>,
    /// The shape last sent; `None` inside is "no blur".
    sent: Mutex<Option<Option<Shape>>>,
}
enum Effect {
    Standard(ExtBackgroundEffectSurfaceV1),
    Kwin {
        manager: OrgKdeKwinBlurManager,
        blur: OrgKdeKwinBlur,
    },
}

/// What the registry round trip gathers on its private queue.
#[derive(Default)]
struct Found {
    globals: Vec<(u32, String, u32)>,
    done: bool,
    blurs: bool,
}
impl Dispatch<WlRegistry, ()> for Found {
    fn event(
        state: &mut Self,
        _: &WlRegistry,
        event: wl_registry::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            state.globals.push((name, interface, version));
        }
    }
}
impl Dispatch<WlCallback, ()> for Found {
    fn event(
        state: &mut Self,
        _: &WlCallback,
        event: wl_callback::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_callback::Event::Done { .. } = event {
            state.done = true;
        }
    }
}
impl Dispatch<ExtBackgroundEffectManagerV1, ()> for Found {
    fn event(
        state: &mut Self,
        _: &ExtBackgroundEffectManagerV1,
        event: ext_background_effect_manager_v1::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let ext_background_effect_manager_v1::Event::Capabilities { flags } = event {
            state.blurs = matches!(
                flags,
                WEnum::Value(flags)
                    if flags.contains(ext_background_effect_manager_v1::Capability::Blur)
            );
        }
    }
}
delegate_noop!(Found: ignore WlCompositor);
delegate_noop!(Found: ignore WlRegion);
delegate_noop!(Found: ignore ExtBackgroundEffectSurfaceV1);
delegate_noop!(Found: ignore OrgKdeKwinBlurManager);
delegate_noop!(Found: ignore OrgKdeKwinBlur);

const ROUND_TRIP: Duration = Duration::from_secs(2);

/// Waits until the compositor has answered every request sent so far on
/// `queue`. The window's event loop reads the same socket on its own thread,
/// so this never blocks in a read: whichever thread reads fills this queue.
fn round_trip(connection: &Connection, queue: &mut EventQueue<Found>, found: &mut Found) -> bool {
    found.done = false;
    let _ = connection.display().sync(&queue.handle(), ());
    let deadline = Instant::now() + ROUND_TRIP;
    loop {
        if queue.dispatch_pending(found).is_err() {
            return false;
        }
        if found.done {
            return true;
        }
        if Instant::now() >= deadline || connection.flush().is_err() {
            return false;
        }
        if let Some(guard) = connection.prepare_read() {
            match guard.read() {
                Ok(_) => continue,
                Err(wayland_client::backend::WaylandError::Io(error))
                    if error.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(_) => return false,
            }
        }
        std::thread::sleep(Duration::from_millis(4));
    }
}

impl BlurBehind {
    /// The request for the window `iced::window::wayland_parent_export`
    /// resolved to; `None` off Wayland, or where the compositor blurs nothing
    /// on request.
    fn from_payload(payload: Option<Box<dyn Any + Send>>) -> Option<Self> {
        // The payload the runtime documents: the window's own connection, its
        // surface, and a count of the surface's incarnations.
        let (connection, surface, _) = *payload?
            .downcast::<(Connection, WlSurface, u64)>()
            .ok()?;
        Self::bind(&connection, &surface)
    }

    fn bind(connection: &Connection, surface: &WlSurface) -> Option<Self> {
        if !surface.is_alive() {
            return None;
        }
        // A surface may have one effect object: a second request for the
        // same surface is a protocol error that ends the connection.
        /// Each surface with a request, and the request while it lives.
        type Requests = Vec<(ObjectId, Weak<Bound>)>;
        static BOUND: OnceLock<Mutex<Requests>> = OnceLock::new();
        let mut bound = BOUND.get_or_init(Mutex::default).lock().ok()?;
        bound.retain(|(_, known)| known.strong_count() > 0);
        if let Some(known) = bound
            .iter()
            .find(|(id, _)| *id == surface.id())
            .and_then(|(_, known)| known.upgrade())
        {
            return Some(Self(known));
        }

        let mut queue = connection.new_event_queue::<Found>();
        let handle = queue.handle();
        let registry = connection.display().get_registry(&handle, ());
        let mut found = Found::default();
        let answered = round_trip(connection, &mut queue, &mut found);
        let global = |interface: &str| {
            found
                .globals
                .iter()
                .find(|(_, name, _)| name == interface)
                .map(|(name, _, version)| (*name, *version))
        };
        let compositor = answered
            .then(|| global("wl_compositor"))
            .flatten()
            .map(|(name, version)| {
                registry.bind::<WlCompositor, _, _>(name, version.min(4), &handle, ())
            });
        let standard = global("ext_background_effect_manager_v1");
        let kwin = global("org_kde_kwin_blur_manager");
        let mut effect = None;
        if let (Some(_), Some((name, _))) = (&compositor, standard) {
            let manager = registry.bind::<ExtBackgroundEffectManagerV1, _, _>(name, 1, &handle, ());
            // The manager says what it can do as soon as it is bound.
            if round_trip(connection, &mut queue, &mut found) && found.blurs {
                effect = Some(Effect::Standard(manager.get_background_effect(
                    surface,
                    &handle,
                    (),
                )));
            }
            manager.destroy();
        }
        if let (None, Some(_), Some((name, _))) = (&effect, &compositor, kwin) {
            let manager = registry.bind::<OrgKdeKwinBlurManager, _, _>(name, 1, &handle, ());
            let blur = manager.create(surface, &handle, ());
            effect = Some(Effect::Kwin { manager, blur });
        }
        // wl_registry has no destructor request; drop its local route.
        let _ = connection.backend().destroy_object(&registry.id());
        let _ = connection.flush();
        let inner = Arc::new(Bound {
            connection: connection.clone(),
            compositor: compositor?,
            effect: effect?,
            surface: surface.clone(),
            handle,
            _queue: Mutex::new(queue),
            sent: Mutex::new(None),
        });
        bound.push((surface.id(), Arc::downgrade(&inner)));
        Some(Self(inner))
    }

    /// Asks for blur behind `shape`, or for none. Applied with the window's
    /// next frame; nothing is sent while the shape stays as it is.
    pub(crate) fn set(&self, shape: Option<Shape>) {
        let bound = &self.0;
        let Ok(mut sent) = bound.sent.lock() else {
            return;
        };
        if *sent == Some(shape) || !bound.surface.is_alive() {
            return;
        }
        let region = shape.map(|shape| {
            let region = bound.compositor.create_region(&bound.handle, ());
            for (x, y, width, height) in shape.rectangles() {
                region.add(x, y, width, height);
            }
            region
        });
        match &bound.effect {
            Effect::Standard(effect) => effect.set_blur_region(region.as_ref()),
            Effect::Kwin { manager, blur } => match &region {
                Some(region) => {
                    blur.set_region(Some(region));
                    blur.commit();
                }
                None => manager.unset(&bound.surface),
            },
        }
        if let Some(region) = region {
            region.destroy();
        }
        let _ = bound.connection.flush();
        *sent = Some(shape);
    }
}
impl Drop for Bound {
    fn drop(&mut self) {
        match &self.effect {
            Effect::Standard(effect) => effect.destroy(),
            Effect::Kwin { manager, blur } => {
                if self.surface.is_alive() {
                    manager.unset(&self.surface);
                }
                blur.release();
            }
        }
        let _ = self.connection.flush();
    }
}

/// The light on a painted glass frame: a rim that catches it along the
/// window's edge, brightest at the top left, and a sheen that falls off down
/// the frame. Drawn over the frame's tint and under the chrome.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Light {
    pub(crate) radius: f32,
    /// The power of the corners' curve: `CIRCLE` for an arc.
    pub(crate) power: f32,
    /// 0 to 1: how bright the rim is.
    pub(crate) edge: f32,
    /// 0 to 1: how bright the sheen is.
    pub(crate) shine: f32,
    pub(crate) dark: bool,
}
/// The power of a circle's arc, as a corner's curve.
pub(crate) const CIRCLE: f32 = 2.0;

/// The outline of a frame `size` large at `origin`, its corners curved at
/// `radius` with `power`.
fn outline(origin: Point, size: Size, radius: f32, power: f32) -> canvas::Path {
    if (power - CIRCLE).abs() < 0.05 || radius < 0.5 {
        return canvas::Path::rounded_rectangle(origin, size, radius.into());
    }
    let quarter = quarter(radius, power);
    let (left, top) = (origin.x, origin.y);
    let (right, bottom) = (left + size.width, top + size.height);
    canvas::Path::new(|path| {
        // Clockwise from the top left corner's meeting with the left side.
        for (index, (x, y)) in quarter.iter().enumerate() {
            let point = Point::new(left + radius - x, top + radius - y);
            if index == 0 {
                path.move_to(point);
            } else {
                path.line_to(point);
            }
        }
        for (x, y) in quarter.iter().rev() {
            path.line_to(Point::new(right - radius + x, top + radius - y));
        }
        for (x, y) in &quarter {
            path.line_to(Point::new(right - radius + x, bottom - radius + y));
        }
        for (x, y) in quarter.iter().rev() {
            path.line_to(Point::new(left + radius - x, bottom - radius + y));
        }
        path.close();
    })
}

/// A quarter of a corner's curve, from where it leaves one side to where it
/// meets the next, as offsets from the corner's centre.
fn quarter(radius: f32, power: f32) -> Vec<(f32, f32)> {
    const STEPS: usize = 12;
    // A quarter turn's cosine comes out a hair under zero; a power of that
    // is no number.
    let along = |part: f32| radius * part.max(0.0).powf(CIRCLE / power);
    (0..=STEPS)
        .map(|step| {
            let angle = std::f32::consts::FRAC_PI_2 * step as f32 / STEPS as f32;
            (along(angle.cos()), along(angle.sin()))
        })
        .collect()
}

impl Light {
    /// The rim's opacity where it is brightest, and the sheen's at the top.
    /// A dark frame shows white light at a fraction of what a light one needs.
    pub(crate) fn strengths(self) -> (f32, f32) {
        let (rim, sheen) = if self.dark { (0.5, 0.11) } else { (0.95, 0.5) };
        (
            rim * self.edge.clamp(0.0, 1.0),
            sheen * self.shine.clamp(0.0, 1.0),
        )
    }
}
impl<Message> canvas::Program<Message> for Light {
    type State = ();
    fn draw(
        &self,
        (): &(),
        renderer: &iced::Renderer,
        _: &iced::Theme,
        bounds: Rectangle,
        _: iced::mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let (rim, sheen) = self.strengths();
        let size = bounds.size();
        if size.width < 4.0 || size.height < 4.0 || (rim <= 0.0 && sheen <= 0.0) {
            return Vec::new();
        }
        let white = |alpha: f32| Color {
            a: alpha,
            ..Color::WHITE
        };
        let mut frame = canvas::Frame::new(renderer, size);
        let radius = self.radius.min(size.width / 2.0).min(size.height / 2.0);
        if sheen > 0.0 {
            // Light from above: strongest under the top edge, gone a third
            // of the way down, with a breath of it left across the rest.
            let reach = (size.height * 0.34).clamp(48.0, 260.0);
            let shape = outline(Point::ORIGIN, size, radius, self.power);
            let fall = canvas::gradient::Linear::new(Point::ORIGIN, Point::new(0.0, reach))
                .add_stop(0.0, white(sheen))
                .add_stop(0.35, white(sheen * 0.45))
                .add_stop(1.0, white(sheen * 0.08));
            frame.fill(
                &shape,
                canvas::Fill {
                    style: canvas::Style::Gradient(fall.into()),
                    ..canvas::Fill::default()
                },
            );
        }
        if rim > 0.0 {
            // Two hairlines, the inner one fainter: an edge with thickness.
            for (inset, strength) in [(0.5, 1.0), (1.5, 0.32)] {
                let inner = Size::new(size.width - 2.0 * inset, size.height - 2.0 * inset);
                let edge = outline(
                    Point::new(inset, inset),
                    inner,
                    (radius - inset).max(0.0),
                    self.power,
                );
                // Lit from the top left; the far corner catches a little of
                // it again, as a bevel does.
                let across = canvas::gradient::Linear::new(
                    Point::ORIGIN,
                    Point::new(size.width.min(size.height * 1.6), size.height),
                )
                .add_stop(0.0, white(rim * strength))
                .add_stop(0.3, white(rim * strength * 0.3))
                .add_stop(0.62, white(rim * strength * 0.1))
                .add_stop(1.0, white(rim * strength * 0.42));
                frame.stroke(
                    &edge,
                    canvas::Stroke {
                        style: canvas::Style::Gradient(across.into()),
                        width: 1.0,
                        ..canvas::Stroke::default()
                    },
                );
            }
        }
        vec![frame.into_geometry()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn covered(shape: Shape, x: i32, y: i32) -> bool {
        shape
            .rectangles()
            .into_iter()
            .any(|(left, top, width, height)| {
                (left..left + width).contains(&x) && (top..top + height).contains(&y)
            })
    }

    #[test]
    fn the_blur_region_is_the_rounded_frame_and_nothing_outside_it() {
        let shape = Shape::of(Size::new(400.4, 300.0), 21.6);
        assert_eq!(
            shape,
            Shape {
                width: 400,
                height: 300,
                radius: 22,
                corner: Corner::Own,
            }
        );
        let rows = shape.rectangles();
        // Inside the window, and no row laid over another.
        let mut area = 0;
        for (index, (x, y, width, height)) in rows.iter().copied().enumerate() {
            assert!(x >= 0 && y >= 0 && x + width <= 400 && y + height <= 300);
            area += width * height;
            for (_, other_y, _, other_height) in &rows[index + 1..] {
                assert!(
                    y + height <= *other_y || other_y + other_height <= y,
                    "rows overlap"
                );
            }
        }
        // The corner's own pixel is outside the arc; the pixel on the arc's
        // diagonal just inside it is covered, at all four corners.
        for (x, y) in [(0, 0), (399, 0), (0, 299), (399, 299)] {
            assert!(!covered(shape, x, y), "corner {x},{y}");
        }
        for (x, y) in [(7, 7), (392, 7), (7, 292), (392, 292)] {
            assert!(covered(shape, x, y), "inside the arc at {x},{y}");
        }
        for (x, y) in [(5, 5), (394, 5), (5, 294), (394, 294)] {
            assert!(!covered(shape, x, y), "outside the arc at {x},{y}");
        }
        // The edges' middles and the centre are covered.
        for (x, y) in [(200, 0), (0, 150), (399, 150), (200, 299), (200, 150)] {
            assert!(covered(shape, x, y), "{x},{y}");
        }
        // The area is the rectangle less what four corners cut away, and
        // each row errs towards the inside by less than a pixel a side.
        let cut = (4.0 - std::f32::consts::PI) * 22.0 * 22.0;
        let away = (400 * 300 - area) as f32;
        assert!(away >= cut && away < cut + 4.0 * 22.0, "{away} of {cut}");

        // A square window is one rectangle; a radius larger than the window
        // can hold is brought down to what it can.
        assert_eq!(
            Shape::of(Size::new(40.0, 30.0), 0.0).rectangles(),
            vec![(0, 0, 40, 30)]
        );
        let small = Shape::of(Size::new(20.0, 20.0), 64.0);
        assert!(covered(small, 10, 10) && !covered(small, 0, 0));
        assert!(
            Shape::of(Size::new(0.0, 0.0), 8.0).rectangles().is_empty(),
            "no window, no region"
        );
    }

    #[test]
    fn blur_under_a_corner_the_compositor_cuts_keeps_inside_its_curve() {
        let size = Size::new(400.0, 300.0);
        let rounding = |power| crate::corners::Rounding {
            radius: 21.0,
            power,
        };
        let circle = Shape::under(size, rounding(2.0));
        // Every blurred pixel of a corner lies, with its outermost point,
        // inside the arc drawn a pixel smaller: nothing under the edge the
        // compositor smooths.
        let mut blurred = 0;
        for y in 0..21 {
            for x in 0..21 {
                if covered(circle, x, y) {
                    blurred += 1;
                    let (dx, dy) = ((21 - x) as f32, (21 - y) as f32);
                    assert!(dx * dx + dy * dy <= 20.0 * 20.0, "{x},{y} is on the edge");
                    for (x, y) in [(399 - x, y), (x, 299 - y), (399 - x, 299 - y)] {
                        assert!(covered(circle, x, y), "the corners differ at {x},{y}");
                    }
                }
            }
        }
        // And it is the corner less that edge, not less than that: a square
        // of 21 holds 346 pixels of the arc, its edge about one pixel deep
        // along 33.
        assert!((270..330).contains(&blurred), "{blurred} pixels blurred");
        // The straight edges are blurred to the window's side.
        for (x, y) in [(200, 0), (0, 150), (399, 150), (200, 299), (21, 0), (0, 21)] {
            assert!(covered(circle, x, y), "{x},{y}");
        }
        // A frame that rounds itself is blurred as far as its own paint,
        // which reaches further than the cut one's.
        let own = Shape::of(size, 21.0);
        assert!(covered(own, 4, 9) && !covered(circle, 4, 9));
        // A squarer curve leaves more of the corner inside it.
        let square = Shape::under(size, rounding(4.0));
        assert!(covered(square, 5, 5) && !covered(circle, 5, 5));
        for y in 0..21 {
            for x in 0..21 {
                assert!(!covered(circle, x, y) || covered(square, x, y), "{x},{y}");
            }
        }
    }

    #[test]
    fn the_rim_follows_a_squarer_corner_from_side_to_side() {
        for power in [1.0, 3.0, 4.0, 10.0] {
            let curve = quarter(20.0, power);
            assert!(
                curve.iter().all(|(x, y)| x.is_finite() && y.is_finite()),
                "power {power}: {curve:?}"
            );
            // It leaves one side where the corner starts and meets the next
            // where it ends.
            let (first, last) = (curve[0], curve[curve.len() - 1]);
            assert!((first.0 - 20.0).abs() < 0.01 && first.1.abs() < 0.01, "{first:?}");
            assert!(last.0.abs() < 0.01 && (last.1 - 20.0).abs() < 0.01, "{last:?}");
            // Every point is on the curve of that power.
            for (x, y) in &curve {
                let on = (x / 20.0).powf(power) + (y / 20.0).powf(power);
                assert!((on - 1.0).abs() < 0.01, "power {power}: {x},{y} gives {on}");
            }
        }
    }

    #[test]
    fn a_dark_frame_takes_less_light_than_a_light_one() {
        let light = |dark| Light {
            radius: 20.0,
            power: CIRCLE,
            edge: 1.0,
            shine: 1.0,
            dark,
        };
        let (dark_rim, dark_sheen) = light(true).strengths();
        let (light_rim, light_sheen) = light(false).strengths();
        assert!(dark_rim < light_rim && dark_sheen < light_sheen);
        assert!(light_rim <= 1.0 && light_sheen <= 0.5);
        let off = Light {
            edge: 0.0,
            shine: -1.0,
            ..light(true)
        };
        assert_eq!(off.strengths(), (0.0, 0.0));
    }
}
