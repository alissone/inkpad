//! M0 spike: proves a static Rust binary runs on the Kindle's kernel and
//! collects the hardware facts the HAL (M1) needs. Writes a report to stdout.

use std::collections::HashMap;
use std::fs;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

fn show(label: &str, path: &str) {
    match fs::read_to_string(path) {
        Ok(s) => println!("{label}: {}", s.trim()),
        Err(e) => println!("{label}: <{e}>"),
    }
}

fn main() {
    let t0 = Instant::now();
    println!("== inkpad hello (M0) ==");
    show("kernel", "/proc/version");
    show("cpu", "/proc/cpuinfo");
    show("meminfo", "/proc/meminfo");

    // std features that depend on newer syscalls: threads, random hash seeds, clocks.
    let handle = std::thread::spawn(|| (1..=1000u64).sum::<u64>());
    println!("thread: sum={}", handle.join().unwrap_or(0));
    let mut m = HashMap::new();
    m.insert("ok", 1);
    println!("hashmap: {}", m.len());
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    println!("unix_time: {now}");

    // Framebuffer facts.
    for f in ["name", "virtual_size", "bits_per_pixel", "stride", "rotate", "mode", "modes"] {
        show(&format!("fb0.{f}"), &format!("/sys/class/graphics/fb0/{f}"));
    }
    // Input devices (touch, buttons).
    show("input_devices", "/proc/bus/input/devices");
    match fs::read_dir("/dev/input") {
        Ok(rd) => {
            let mut names: Vec<String> = rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
            names.sort();
            println!("dev_input: {}", names.join(" "));
        }
        Err(e) => println!("dev_input: <{e}>"),
    }
    // Battery.
    for f in ["capacity", "status", "charging"] {
        show(&format!("battery.{f}"), &format!("/sys/class/power_supply/bd7181x_bat/{f}"));
    }
    println!("elapsed_ms: {}", t0.elapsed().as_millis());
    println!("RESULT: OK");
}
