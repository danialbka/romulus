# romulus

Custom Rust renderer recreating the Weyland-Yutani colony dossier screen.

## What this project is now

The default app is a **custom native Rust renderer**, not a terminal-character UI.

We originally tried to match the reference as a Ratatui terminal UI, but the real terminal output was still too far from the source image because:

- terminal cells are too coarse for the line grid
- box-drawing glyphs do not match the source exactly
- the portrait and fingerprint strip degrade badly in real terminal rendering

So the final direction was:

1. use the original image as the visual reference
2. trace the layout against generated screenshots
3. move from cell rendering to a custom pixel renderer in Rust

The old Ratatui experiment is still in the repo as `cargo run --bin romulus`.

## How we replicated the whole thing

### 1. Start with a Ratatui layout pass

We first built the dossier as a Ratatui screen to approximate:

- the outer frame
- the top bars
- the left-side dossier fields
- the portrait area
- the fingerprint strip

That gave us the rough structure, colors, and proportions.

### 2. Build screenshot and comparison tooling

To stop eyeballing everything, we added tooling to compare our output against the source:

- `src/bin/reference_probe.rs`
  - zoom/crop helper for studying the reference image
- `src/bin/tracing_grid.rs`
  - overlays a tracing-paper style grid
  - aligns the generated shot with the source
  - exports blend, delta, edge, and cell-error views

Those tools let us iterate by coordinates instead of guessing.

### 3. Validate against actual output, not just synthetic screenshots

A big lesson was that synthetic screenshots were misleading.

The TUI looked decent in generated PNGs, but the **actual terminal pane** still looked rough because the terminal font/cell rasterization changed the result a lot.

That is what pushed the project away from a strict terminal-native implementation.

### 4. Switch to a custom Rust renderer

We then built `src/bin/custom_renderer.rs`:

- windowed renderer using `minifb`
- image composition using `image` + `imageproc`
- custom-drawn:
  - frame lines
  - panels
  - labels
  - status text
  - interactive highlights
  - popup menus
- image-assisted regions:
  - portrait crop
  - fingerprint strip crops
  - small logo crop

So the final renderer is not just showing the whole source image. It draws the interface itself, but uses cropped source regions where terminal/native text rendering would never match closely enough on its own.

### 5. Add interaction

After the renderer matched the still image closely enough, we added app behavior:

- clickable top tabs
- clickable badge
- clickable department selector
- clickable gender boxes
- clickable fingerprint cells
- popup menus
- hover/highlight states
- live `02:03:05` clock animation

### 6. Fix performance

The first custom renderer redraw was laggy because it:

- rebuilt the full scene on interaction
- re-cropped and re-resized image regions
- re-uploaded the full buffer too often

So we optimized it by:

- caching a static base frame
- redrawing only the dynamic overlays
- only presenting a new buffer when something changes

## Current project layout

- `src/bin/custom_renderer.rs`
  - main app
- `src/main.rs`
  - older Ratatui prototype
- `src/bin/tracing_grid.rs`
  - tracing-paper comparison harness
- `src/bin/reference_probe.rs`
  - zoom/crop inspection tool

## Run

```bash
cargo run
```

That launches the native windowed renderer.

## Headless / CI / WSL without GUI

```bash
cargo run -- --screenshot custom-renderer-shot.png
```

## Optional custom source image

```bash
cargo run -- path/to/other-image.jpg
```

If no image path is provided, the app uses the bundled `HEI1Ts9aIAETw1k.jpg` reference automatically.

## Controls

- Click top headers to open menus
- Click the badge, department field, gender boxes, and fingerprint cells
- `Tab` cycles the top tabs
- `Esc` closes a menu, then exits
- `Q` quits

## Tracing / verification

Generate a renderer screenshot:

```bash
cargo run -- --screenshot custom-renderer-shot.png
```

Compare it against the reference:

```bash
cargo run --bin tracing_grid -- --reference HEI1Ts9aIAETw1k.jpg --shot custom-renderer-shot.png --out-dir custom_trace_out
```

Useful outputs:

- `custom_trace_out/trace-grid-compare.png`
- `custom_trace_out/trace-grid-cells.png`
- `custom_trace_out/trace-grid-metrics.txt`

## Notes

- Fonts needed by the renderer are bundled in `assets/fonts/`
- Live window mode needs a desktop/GUI environment
- Screenshot mode works in headless environments
