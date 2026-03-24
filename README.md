# romulus

Custom Rust renderer recreating the Weyland-Yutani colony dossier screen.

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

## Notes

- Fonts needed by the renderer are bundled in `assets/fonts/`
- Live window mode needs a desktop/GUI environment
- Screenshot mode works in headless environments
