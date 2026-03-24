use std::{fs, path::PathBuf};

use ab_glyph::FontArc;
use anyhow::{Context, Result, bail};
use image::{DynamicImage, GenericImageView, ImageBuffer, Rgba, RgbaImage, imageops::FilterType};
use imageproc::drawing::{draw_filled_rect_mut, draw_hollow_rect_mut, draw_line_segment_mut, draw_text_mut};
use imageproc::rect::Rect;

const DEFAULT_REFERENCE: &str = "HEI1Ts9aIAETw1k.jpg";
const DEFAULT_SHOT: &str = "tui-shot.png";
const DEFAULT_FONT: &str = "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf";

fn main() -> Result<()> {
    let cli = Cli::parse()?;
    fs::create_dir_all(&cli.out_dir)?;

    let font = load_font()?;
    let reference = image::open(&cli.reference)?;
    let shot = image::open(&cli.shot)?;
    let scaled_shot = scale_like(&shot, reference.dimensions());
    let blend = blend_images(&reference.to_rgba8(), &scaled_shot, 0.5);

    let ref_grid = overlay_grid(reference.to_rgba8(), &font, cli.grid_x, cli.grid_y, "reference");
    let shot_grid = overlay_grid(scaled_shot.clone(), &font, cli.grid_x, cli.grid_y, "shot");
    let blend_grid = overlay_grid(blend, &font, cli.grid_x, cli.grid_y, "blend");

    let ref_path = cli.out_dir.join("trace-grid-reference.png");
    let shot_path = cli.out_dir.join("trace-grid-shot.png");
    let blend_path = cli.out_dir.join("trace-grid-blend.png");
    let compare_path = cli.out_dir.join("trace-grid-compare.png");

    ref_grid.save(&ref_path)?;
    shot_grid.save(&shot_path)?;
    blend_grid.save(&blend_path)?;

    let compare = triptych(&ref_grid, &shot_grid, &blend_grid, &font);
    compare.save(&compare_path)?;

    println!("wrote {}", ref_path.display());
    println!("wrote {}", shot_path.display());
    println!("wrote {}", blend_path.display());
    println!("wrote {}", compare_path.display());
    Ok(())
}

struct Cli {
    reference: PathBuf,
    shot: PathBuf,
    out_dir: PathBuf,
    grid_x: u32,
    grid_y: u32,
}

impl Cli {
    fn parse() -> Result<Self> {
        let mut reference = PathBuf::from(DEFAULT_REFERENCE);
        let mut shot = PathBuf::from(DEFAULT_SHOT);
        let mut out_dir = PathBuf::from("trace_grid_out");
        let mut grid_x = 12;
        let mut grid_y = 12;
        let mut args = std::env::args().skip(1);

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--reference" => reference = PathBuf::from(next_value(&mut args, "--reference")?),
                "--shot" => shot = PathBuf::from(next_value(&mut args, "--shot")?),
                "--out-dir" => out_dir = PathBuf::from(next_value(&mut args, "--out-dir")?),
                "--grid-x" => grid_x = next_value(&mut args, "--grid-x")?.parse()?,
                "--grid-y" => grid_y = next_value(&mut args, "--grid-y")?.parse()?,
                "-h" | "--help" => {
                    print_help();
                    std::process::exit(0);
                }
                other => bail!("unrecognized argument: {other}"),
            }
        }

        Ok(Self {
            reference,
            shot,
            out_dir,
            grid_x,
            grid_y,
        })
    }
}

fn next_value(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String> {
    args.next().with_context(|| format!("missing value after {flag}"))
}

fn print_help() {
    println!(
        "Usage:\n  cargo run --bin tracing_grid -- [--reference IMG] [--shot IMG] [--out-dir DIR] [--grid-x N] [--grid-y N]"
    );
}

fn load_font() -> Result<FontArc> {
    let bytes = fs::read(DEFAULT_FONT).context("failed to load tracing font")?;
    FontArc::try_from_vec(bytes).map_err(|_| anyhow::anyhow!("invalid font data"))
}

fn scale_like(image: &DynamicImage, dims: (u32, u32)) -> RgbaImage {
    image
        .resize_exact(dims.0, dims.1, FilterType::Triangle)
        .to_rgba8()
}

fn blend_images(a: &RgbaImage, b: &RgbaImage, alpha: f32) -> RgbaImage {
    let (w, h) = a.dimensions();
    let mut out = ImageBuffer::from_pixel(w, h, Rgba([0, 0, 0, 255]));
    for y in 0..h {
        for x in 0..w {
            let pa = a.get_pixel(x, y).0;
            let pb = b.get_pixel(x, y).0;
            let mix = |va: u8, vb: u8| ((va as f32 * (1.0 - alpha)) + (vb as f32 * alpha)) as u8;
            out.put_pixel(x, y, Rgba([mix(pa[0], pb[0]), mix(pa[1], pb[1]), mix(pa[2], pb[2]), 255]));
        }
    }
    out
}

fn overlay_grid(mut image: RgbaImage, font: &FontArc, grid_x: u32, grid_y: u32, label: &str) -> RgbaImage {
    let (w, h) = image.dimensions();
    let major = Rgba([0, 255, 220, 255]);
    let minor = Rgba([0, 180, 160, 180]);
    let accent = Rgba([255, 0, 220, 180]);
    let panel = Rgba([0, 0, 0, 180]);

    let step_x = w as f32 / grid_x.max(1) as f32;
    let step_y = h as f32 / grid_y.max(1) as f32;

    for i in 0..=grid_x {
        let x = (i as f32 * step_x).round();
        draw_line_segment_mut(&mut image, (x, 0.0), (x, h as f32), if i % 3 == 0 { major } else { minor });
        if i < grid_x {
            let tx = (x + 4.0) as i32;
            draw_filled_rect_mut(&mut image, Rect::at(tx - 2, 4).of_size(48, 16), panel);
            draw_text_mut(&mut image, Rgba([255,255,255,255]), tx, 4, 12.0, font, &format!("{:.3}", i as f32 / grid_x as f32));
        }
    }

    for i in 0..=grid_y {
        let y = (i as f32 * step_y).round();
        draw_line_segment_mut(&mut image, (0.0, y), (w as f32, y), if i % 3 == 0 { accent } else { Rgba([150, 0, 120, 150]) });
        if i < grid_y {
            let ty = (y + 4.0) as i32;
            draw_filled_rect_mut(&mut image, Rect::at(4, ty - 2).of_size(52, 16), panel);
            draw_text_mut(&mut image, Rgba([255,255,255,255]), 6, ty, 12.0, font, &format!("{:.3}", i as f32 / grid_y as f32));
        }
    }

    draw_hollow_rect_mut(&mut image, Rect::at(0, 0).of_size(w.saturating_sub(1), h.saturating_sub(1)), major);
    draw_filled_rect_mut(&mut image, Rect::at(12, 12).of_size(120, 22), panel);
    draw_text_mut(&mut image, Rgba([255,255,255,255]), 18, 16, 16.0, font, label);
    image
}

fn triptych(a: &RgbaImage, b: &RgbaImage, c: &RgbaImage, font: &FontArc) -> RgbaImage {
    let gap = 24;
    let (w, h) = a.dimensions();
    let mut canvas = ImageBuffer::from_pixel(w * 3 + gap * 4, h + 48, Rgba([10, 12, 12, 255]));
    for (index, (label, image)) in [("reference", a), ("shot", b), ("blend", c)].into_iter().enumerate() {
        let x = gap + index as u32 * (w + gap);
        image::imageops::overlay(&mut canvas, image, x.into(), 40);
        draw_text_mut(&mut canvas, Rgba([255,255,255,255]), x as i32, 12, 18.0, font, label);
    }
    canvas
}
