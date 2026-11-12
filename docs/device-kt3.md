# Device notes: Kindle basic 8th gen (KT3)

Measured on the device with `tools/hello` (2026-11-12), firmware 5.16.2.1.1.

| | |
|---|---|
| Kernel | Linux 3.10.53-lab126, PREEMPT |
| SoC | Freescale i.MX6 SoloLite, ARMv7 (Cortex-A9, rev 10), NEON + VFPv3, ~1 GHz |
| RAM | 512 MB; ~85 MB free with the stock UI running |
| Framebuffer | `/dev/fb0`, `mxc_epdc_fb`, 8 bpp gray, mode `600x800`, stride 608 bytes, virtual 608×10752, `rotate`=3 |
| Touch | `/dev/input/event0`, `zforce2` (IR), multitouch protocol B (`ABS_MT_SLOT`, `ABS_MT_TRACKING_ID`, `ABS_MT_POSITION_X/Y`), `BTN_TOUCH` + `BTN_TOOL_FINGER/DOUBLETAP/TRIPLETAP` |
| Other input | none besides `mice`; the power button goes through `powerd` (lipc) |
| Battery | `/sys/class/power_supply/bd7181x_bat/{capacity,status,charging}` |

Static Rust (musl, soft-float `armv7-unknown-linux-musleabi`) runs fine: std threads, hashing, clocks OK. Process start to exit: 126 ms.
