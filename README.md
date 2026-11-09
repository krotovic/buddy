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

- **Tray / menu bar icon:** what is pending and Clear pending; On, Calm, Off; Peek now; Preview (Idle, Done,
  Needs you); Peek every (30 s test
  interval, 2, 5 or 10 minutes); Size (S, M, L, XL); Edge; Frame colour;
  Claude Code hooks (install or remove); Open settings file; Quit.
- **Shortcut:** Ctrl+Alt+C (Windows) or Cmd+Option+C (macOS) toggles Off.

Settings are saved as JSON:

- Windows: `%APPDATA%\krotovic\Buddy\config\settings.json`
- macOS: `~/Library/Application Support/com.krotovic.Buddy/settings.json`

## Claude Code

Tick **Claude Code hooks** in the tray menu once. It adds HTTP hooks to
`~/.claude/settings.json` (keeping your other hooks, with a backup next to
it as `settings.json.buddy-backup`):

| Hook | Cube |
| --- | --- |
| Notification (permission prompts, questions) | Needs you: waves, then stays half visible |
| Stop | Done: peeks, waves, winks and hides |
| UserPromptSubmit, SessionEnd | Clears that session |

Items are tracked per session and merged: Needs you wins over Done, a new
Done while something needs you only changes the count, and Done items clear
themselves after `done_timeout_mins` (default 10, 0 = never). The hooks post
to `http://127.0.0.1:47821/buddy/hook` with a token from Buddy's settings
file; when Buddy is not running they fail fast and Claude Code carries on.

## Assets

Everything comes from the
[Figma design file](https://www.figma.com/design/FRcEU3FARqVPj2IXguv6wz) and
is embedded in the binary at build time:

- `assets/animations/` holds one timeline per animation, time of day and
  edge (Idle, Done, Needs you × morning, midday, evening × 4 edges), with
  the keyframes, easings and layer tree of the Figma motion frames.
- `assets/parts/` holds the SVG layers the timelines draw, named by content
  hash, so identical layers are stored once.
- `assets/figma/coral-idle.svg` is the still cube used for the tray icon.

The SVGs stay exactly as exported; the frame colour is swapped in code.

To export again after changing the animations in Figma, run
`tools/figma-export.js` through the Figma MCP `use_figma` tool (see the
comment at its top) and feed the results to `tools/import-figma.py`.

To look at every animation without a GPU, render contact sheets to
`target/frames/`:

```sh
cargo test contact_sheets -- --ignored
```

## Tests

```sh
cargo test
```

CI (`.github/workflows/ci.yml`) runs only when started by hand.
