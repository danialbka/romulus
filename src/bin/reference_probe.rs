use std::{fs, path::PathBuf};

use image::{imageops::{resize, FilterType}, DynamicImage, GenericImageView, ImageBuffer, Rgba, RgbaImage};

const DEFAULT_IMAGE: &str = "HEI1Ts9aIAETw1k.jpg";

#[derive(Clone, Copy)]
struct CropRect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

const PORTRAIT_CROP: CropRect = CropRect { x: 0.53, y: 0.17, w: 0.41, h: 0.67 };
const PRINTS_CROP: CropRect = CropRect { x: 0.54, y: 0.79, w: 0.41, h: 0.13 };
const HEADER_CROP: CropRect = CropRect { x: 0.02, y: 0.02, w: 0.96, h: 0.14 };
const LEFT_PANEL_CROP: CropRect = CropRect { x: 0.02, y: 0.17, w: 0.50, h: 0.75 };

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse()?;
    fs::create_dir_all(&cli.out_dir)?;

    let image = image::open(&cli.image_path)?;
    let mut sheet_items = Vec::new();

    for spec in [
        CropSpec { name: "portrait", crop: PORTRAIT_CROP, zoom: 3 },
        CropSpec { name: "fingerprints", crop: PRINTS_CROP, zoom: 6 },
        CropSpec { name: "header", crop: HEADER_CROP, zoom: 4 },
        CropSpec { name: "left_panel", crop: LEFT_PANEL_CROP, zoom: 4 },
    ] {
        let cropped = crop_region(&image, spec.crop);
        let raw_path = cli.out_dir.join(format!("{}.png", spec.name));
        cropped.save(&raw_path)?;

        let zoomed = enlarge(&cropped, spec.zoom);
        let zoom_path = cli.out_dir.join(format!("{}_zoom{}x.png", spec.name, spec.zoom));
        zoomed.save(&zoom_path)?;

        println!("wrote {}", raw_path.display());
        println!("wrote {}", zoom_path.display());
        sheet_items.push((format!("{}_zoom{}x", spec.name, spec.zoom), zoomed));
    }

    if let Some(sheet_path) = cli.sheet_path {
        let contact = contact_sheet(&sheet_items, 2, 18, 8, 520, 360, Rgba([10, 18, 15, 255]));
        contact.save(&sheet_path)?;
        println!("wrote {}", sheet_path.display());
    }

    Ok(())
}

struct CropSpec {
    name: &'static str,
    crop: CropRect,
    zoom: u32,
}

struct Cli {
    image_path: PathBuf,
    out_dir: PathBuf,
    sheet_path: Option<PathBuf>,
}

impl Cli {
    fn parse() -> Result<Self, Box<dyn std::error::Error>> {
        let mut image_path = PathBuf::from(DEFAULT_IMAGE);
        let mut out_dir = PathBuf::from("probe_out");
        let mut sheet_path = None;
        let mut args = std::env::args().skip(1);

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--image" => {
                    image_path = PathBuf::from(next_value(&mut args, "--image")?);
                }
                "--out-dir" => {
                    out_dir = PathBuf::from(next_value(&mut args, "--out-dir")?);
                }
                "--sheet" => {
                    sheet_path = Some(PathBuf::from(next_value(&mut args, "--sheet")?));
                }
                "-h" | "--help" => {
                    print_help();
                    std::process::exit(0);
                }
                other if image_path == PathBuf::from(DEFAULT_IMAGE) => {
                    image_path = PathBuf::from(other);
                }
                other => {
                    return Err(format!("unrecognized argument: {other}").into());
                }
            }
        }

        Ok(Self { image_path, out_dir, sheet_path })
    }
}

fn next_value(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, Box<dyn std::error::Error>> {
    args.next().ok_or_else(|| format!("missing value after {flag}").into())
}

fn print_help() {
    println!(
        "Usage:
  cargo run --bin reference_probe -- [--image IMG] [--out-dir DIR] [--sheet FILE]

Exports crops:
  portrait.png / portrait_zoom3x.png
  fingerprints.png / fingerprints_zoom6x.png
  header.png / header_zoom4x.png
  left_panel.png / left_panel_zoom4x.png
"
    );
}

fn crop_region(image: &DynamicImage, rect: CropRect) -> DynamicImage {
    let (w, h) = image.dimensions();
    let x = (w as f32 * rect.x).round() as u32;
    let y = (h as f32 * rect.y).round() as u32;
    let cw = (w as f32 * rect.w).round() as u32;
    let ch = (h as f32 * rect.h).round() as u32;

    let x = x.min(w.saturating_sub(1));
    let y = y.min(h.saturating_sub(1));
    let cw = cw.max(1).min(w - x);
    let ch = ch.max(1).min(h - y);
    image.crop_imm(x, y, cw, ch)
}

fn enlarge(image: &DynamicImage, scale: u32) -> DynamicImage {
    let rgba = image.to_rgba8();
    let (w, h) = rgba.dimensions();
    DynamicImage::ImageRgba8(resize(&rgba, w * scale, h * scale, FilterType::Nearest))
}

fn contact_sheet(
    items: &[(String, DynamicImage)],
    columns: usize,
    pad: u32,
    gutter: u32,
    tile_w: u32,
    tile_h: u32,
    bg: Rgba<u8>,
) -> RgbaImage {
    let rows = items.len().div_ceil(columns);
    let cell_w = tile_w + gutter * 2;
    let cell_h = tile_h + gutter * 2;

    let sheet_w = pad * 2 + columns as u32 * cell_w;
    let sheet_h = pad * 2 + rows as u32 * cell_h;
    let mut sheet = ImageBuffer::from_pixel(sheet_w, sheet_h, bg);

    for (index, (_name, img)) in items.iter().enumerate() {
        let col = (index % columns) as u32;
        let row = (index / columns) as u32;
        let x0 = pad + col * cell_w + gutter;
        let y0 = pad + row * cell_h + gutter;
        blit_centered(&mut sheet, img, x0, y0, tile_w, tile_h);
    }

    sheet
}

fn blit_centered(sheet: &mut RgbaImage, img: &DynamicImage, x0: u32, y0: u32, max_w: u32, max_h: u32) {
    let fitted = fit_to_box(img, max_w, max_h);
    let x_off = x0 + (max_w.saturating_sub(fitted.width())) / 2;
    let y_off = y0 + (max_h.saturating_sub(fitted.height())) / 2;

    for y in 0..fitted.height() {
        for x in 0..fitted.width() {
            let px = fitted.get_pixel(x, y);
            sheet.put_pixel(x_off + x, y_off + y, *px);
        }
    }
}

fn fit_to_box(img: &DynamicImage, max_w: u32, max_h: u32) -> RgbaImage {
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    let scale = f32::min(max_w as f32 / w as f32, max_h as f32 / h as f32).max(0.0001);
    let new_w = ((w as f32 * scale).round() as u32).max(1);
    let new_h = ((h as f32 * scale).round() as u32).max(1);
    resize(&rgba, new_w, new_h, FilterType::Nearest)
}
