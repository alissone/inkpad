//! inkpad: the crate apps depend on. Re-exports the toolkit and picks the
//! right backend: the Kindle hardware when built for ARM Linux, the desktop
//! simulator everywhere else.
//!
//! ```ignore
//! use inkpad::prelude::*;
//! fn main() { inkpad::run(MyApp::default()).unwrap() }
//! ```

pub use inkpad_gfx as gfx;
pub use inkpad_hal as hal;
pub use inkpad_ui as ui;
/// HTTPS requests and Wi-Fi on demand.
pub use inkpad_net as net;

use inkpad_hal::{Display, Input};
use std::io;

pub mod prelude {
    pub use inkpad_ui::*;
}

/// Open the display and input for this platform.
pub fn platform() -> io::Result<(Box<dyn Display>, Box<dyn Input>)> {
    #[cfg(all(target_os = "linux", target_arch = "arm"))]
    {
        let d = inkpad_hal::kindle::KindleDisplay::open()?;
        let size = d.size();
        let dev = std::env::var("INKPAD_TOUCH_DEV").unwrap_or_else(|_| "/dev/input/event0".into());
        let t = inkpad_hal::kindle::KindleTouch::open(&dev, size)?;
        Ok((Box::new(d), Box::new(t)))
    }
    #[cfg(not(all(target_os = "linux", target_arch = "arm")))]
    {
        let (d, i) = inkpad_sim::open()?;
        Ok((Box::new(d), Box::new(i)))
    }
}

/// Run an app on this platform until it quits. The app can launch other
/// apps with `Ctx::launch`.
pub fn run<A: inkpad_ui::App>(app: A) -> io::Result<()> {
    let (d, i) = platform()?;
    inkpad_ui::run_with(app, d, i, Some(Box::new(platform)))
}
