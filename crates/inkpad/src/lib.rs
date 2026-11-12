//! inkpad: the crate apps depend on. Re-exports the drawing and platform
//! layers and picks the right backend: the Kindle hardware when built for
//! ARM Linux, the desktop simulator everywhere else.

pub use inkpad_gfx as gfx;
pub use inkpad_hal as hal;

use inkpad_hal::{Display, Input};
use std::io;

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
