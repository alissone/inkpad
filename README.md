# inkpad

A native UI framework for jailbroken e-ink Kindles, written in Rust. The goal: a modern black-and-white "PDA" with small, mostly read-only apps (tasks, calendar, reminders, an AI assistant) that launches like KOReader.

> Status: early spike (M0). Working name; may change before the first release.

## Target devices
First target: Kindle basic 8th gen (KT3), firmware 5.16.x, 600×800 mxcfb e-ink, soft-float ARMv7. Other mxcfb Kindles should follow.

## Building
Requirements: Rust via rustup. Nothing else: the Kindle target links with the bundled `rust-lld` against Rust's self-contained musl (see `.cargo/config.toml`). On Windows without the Windows SDK, use the `stable-x86_64-pc-windows-gnu` host toolchain.

```sh
rustup target add armv7-unknown-linux-musleabi
cargo kindle -p hello     # alias for: build --release --target armv7-unknown-linux-musleabi
```

The output is a single static binary in `target/armv7-unknown-linux-musleabi/release/`.

## Running the M0 test on a Kindle
1. Copy the `hello` binary to `inkpad/hello` on the Kindle's USB drive.
2. Copy `kindle/documents/InkpadHello.sh` to the Kindle's `documents/` folder.
3. Eject, open **Inkpad hello test** from the library, then read `inkpad/hello.log`.

## License
Dual-licensed under MIT or Apache-2.0, at your option. No GPL code is included.
"# inkpad" 
