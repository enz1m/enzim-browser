//! Who rounds this window's corners.
//!
//! Hyprland clips every window to a rounded shape of its own and draws the
//! window's shadow outside that shape. A window that rounds itself as well
//! leaves, wherever its own curve lies inside the compositor's, pixels that
//! are neither window nor shadow, and the bare desktop shows in them: a
//! dotted line along the corner where the two radii agree (the two curves
//! are smoothed differently), a sliver where they do not. Where the
//! compositor rounds, the frame is therefore painted to its square corners
//! and the compositor's curve is the window's. That holds for a Hyprland
//! told to round nothing as well: its shadow follows the window's square
//! box, and a corner the window rounded would show bare inside it.
//!
//! The radius is the compositor's option, asked over its command socket. A
//! window rule that rounds this one window differently is not reported
//! there. Other compositors leave a window's corners to the window.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

/// How long the compositor has to answer. It answers from its event loop,
/// within a frame; a longer wait means there is nobody there.
const REPLY: Duration = Duration::from_millis(500);

/// The curve a compositor cuts a window's corners along.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Rounding {
    /// In logical pixels; none where the compositor keeps windows square.
    pub(crate) radius: f32,
    /// 2 for a circle's arc; more is squarer.
    pub(crate) power: f32,
}

/// The curve the compositor cuts this window's corners along; `None` where
/// the corners are the window's own.
pub(crate) fn ask() -> iced::Task<Option<Rounding>> {
    iced::Task::future(async {
        let (sender, receiver) = iced::futures::channel::oneshot::channel();
        // The reply waits on the compositor; the interface does not.
        let _ = std::thread::Builder::new()
            .name("enzim-corners".into())
            .spawn(move || {
                let _ = sender.send(hyprland());
            });
        receiver.await.ok().flatten()
    })
}

fn hyprland() -> Option<Rounding> {
    let instance = Path::new(&std::env::var_os("XDG_RUNTIME_DIR")?)
        .join("hypr")
        .join(std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE")?);
    let lock = std::fs::read_to_string(instance.join("hyprland.lock")).ok()?;
    let display = std::env::var("WAYLAND_DISPLAY").ok();
    if !shows_on(&lock, display.as_deref()) {
        return None;
    }
    let socket = instance.join(".socket.sock");
    let rounding = number(&request(&socket, "j/getoption decoration:rounding")?)?;
    let power = request(&socket, "j/getoption decoration:rounding_power")
        .and_then(|reply| number(&reply))
        .unwrap_or(CIRCLE);
    rounded(rounding, power)
}

/// Whether the Hyprland that wrote `lock` is the compositor this process
/// draws on. The variable that names a Hyprland is inherited by everything
/// started under it, a compositor nested in it included.
fn shows_on(lock: &str, display: Option<&str>) -> bool {
    // The lock holds the compositor's process and, below it, its socket.
    let Some(socket) = lock.lines().nth(1).map(str::trim) else {
        return false;
    };
    let display = display.unwrap_or("wayland-0");
    !socket.is_empty() && Path::new(display).file_name() == Path::new(socket).file_name()
}

fn request(socket: &Path, command: &str) -> Option<String> {
    let mut stream = UnixStream::connect(socket).ok()?;
    stream.set_read_timeout(Some(REPLY)).ok()?;
    stream.set_write_timeout(Some(REPLY)).ok()?;
    stream.write_all(command.as_bytes()).ok()?;
    let mut reply = String::new();
    stream.read_to_string(&mut reply).ok()?;
    Some(reply)
}

/// The value of an option, from the compositor's reply to `getoption`.
fn number(reply: &str) -> Option<f32> {
    let reply: serde_json::Value = serde_json::from_str(reply).ok()?;
    let value = reply.get("int").or_else(|| reply.get("float"))?.as_f64()?;
    Some(value as f32)
}

/// The power of the curve Hyprland draws for a circle's arc.
const CIRCLE: f32 = 2.0;

/// The curve Hyprland cuts for its `rounding` and `rounding_power` options:
/// it scales the first by the second so that a squarer curve looks as
/// round.
fn rounded(rounding: f32, power: f32) -> Option<Rounding> {
    let power = power.clamp(1.0, 10.0);
    let radius = rounding * (power / CIRCLE);
    (radius.is_finite() && radius >= 0.0).then_some(Rounding { radius, power })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_compositors_reply_gives_the_radius_it_rounds_at() {
        // The replies of Hyprland 0.56.0, as read from its socket.
        let rounding = r#"{"option": "decoration:rounding", "int": 21, "set": true }"#;
        let power = r#"{"option": "decoration:rounding_power", "float": 2.000000, "set": false }"#;
        assert_eq!(number(rounding), Some(21.0));
        assert_eq!(number(power), Some(2.0));
        assert_eq!(number("no such option"), None);
        let curve = |radius, power| Some(Rounding { radius, power });
        assert_eq!(rounded(21.0, 2.0), curve(21.0, 2.0));
        // A squarer curve is drawn at a larger radius.
        assert_eq!(rounded(10.0, 4.0), curve(20.0, 4.0));
        // No rounding is an answer too: the window is kept square.
        assert_eq!(rounded(0.0, 2.0), curve(0.0, 2.0));
        assert_eq!(rounded(f32::NAN, 2.0), None);
    }

    #[test]
    fn only_the_compositor_the_window_is_shown_on_is_asked() {
        let lock = "42\nwayland-24\n";
        assert!(shows_on(lock, Some("wayland-24")));
        assert!(shows_on(lock, Some("/run/user/example/wayland-24")));
        // Started under that Hyprland, shown on a compositor nested in it.
        assert!(!shows_on(lock, Some("wayland-25")));
        assert!(!shows_on(lock, None));
        assert!(shows_on("1\nwayland-0\n", None));
        assert!(!shows_on("42\n", Some("wayland-24")));
    }
}
