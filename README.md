# Buddy

A small framed cube that lives behind the edges of your screen. It peeks out
now and then, so an OLED monitor (Samsung Odyssey G8) never sees a static
image long enough to dim, and it will tell you when local Claude Code work is
done or needs you.

Pure Rust, no webview: `winit` for the transparent, always-on-top,
click-through window, `vello` (GPU) for drawing the SVG layers exported from
Figma, `tray-icon` and `global-hotkey` for the menu bar / tray and shortcut.
Targets Windows and macOS.

## Run

```sh
cargo run --release
```

Logs go to the terminal; set `RUST_LOG=buddy=debug` for more.

## Controls

- **Tray / menu bar icon:** On, Calm, Off; Peek now; Peek every (30 s test
  interval, 2, 5 or 10 minutes); Size (S, M, L, XL); Edge; Frame colour;
  Open settings file; Quit.
- **Shortcut:** Ctrl+Alt+C (Windows) or Cmd+Option+C (macOS) toggles Off.

Settings are saved as JSON:

- Windows: `%APPDATA%\krotovic\Buddy\config\settings.json`
- macOS: `~/Library/Application Support/com.krotovic.Buddy/settings.json`

## Assets

`assets/figma/` holds SVGs exported from the
[Figma design file](https://www.figma.com/design/FRcEU3FARqVPj2IXguv6wz).
They stay exactly as exported; the frame colour is swapped in code.

## Tests

```sh
cargo test
```

CI (`.github/workflows/ci.yml`) runs only when started by hand.
