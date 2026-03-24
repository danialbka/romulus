use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use ab_glyph::FontArc;
use anyhow::{Context, Result, bail};
use image::{DynamicImage, GenericImageView, Rgba, RgbaImage, imageops::FilterType};
use imageproc::{
    drawing::{draw_filled_rect_mut, draw_hollow_rect_mut, draw_text_mut},
    rect::Rect,
};
use minifb::{Key, Window, WindowOptions};

const DEFAULT_IMAGE: &str = "HEI1Ts9aIAETw1k.jpg";
const BASE_W: u32 = 1197;
const BASE_H: u32 = 907;

const BG: Rgba<u8> = Rgba([6, 9, 8, 255]);
const PANEL_BG: Rgba<u8> = Rgba([6, 11, 10, 255]);
const BORDER: Rgba<u8> = Rgba([13, 115, 84, 255]);
const HEADER_BG: Rgba<u8> = Rgba([30, 99, 75, 255]);
const LABEL: Rgba<u8> = Rgba([62, 155, 117, 255]);
const MUTED: Rgba<u8> = Rgba([18, 60, 49, 255]);
const VALUE: Rgba<u8> = Rgba([219, 177, 91, 255]);
const BADGE: Rgba<u8> = Rgba([212, 165, 78, 255]);

const MONO_BOLD: &str = "/usr/share/fonts/truetype/dejavu/DejaVuSansMono-Bold.ttf";
const NARROW_BOLD: &str = "/usr/share/fonts/truetype/liberation/LiberationSansNarrow-Bold.ttf";

const PORTRAIT_CROP: CropRect = CropRect {
    x: 0.503,
    y: 0.172,
    w: 0.485,
    h: 0.648,
};
const PRINTS_SOURCE_CROP: CropRect = CropRect {
    x: 0.509,
    y: 0.872,
    w: 0.466,
    h: 0.076,
};
const LOGO_CROP: CropRect = CropRect {
    x: 0.014,
    y: 0.023,
    w: 0.062,
    h: 0.033,
};

fn main() -> Result<()> {
    let cli = Cli::parse()?;
    let assets = Assets::load(&cli.image_path)?;
    let mut frame = render_scene(&assets)?;

    if let Some(path) = cli.screenshot_path {
        frame.save(&path)?;
        println!("saved screenshot to {}", path.display());
        return Ok(());
    }

    if (cli.scale - 1.0).abs() > f32::EPSILON {
        frame = resize_image(&frame, cli.scale);
    }

    show_window(&frame)
}

struct Cli {
    image_path: PathBuf,
    screenshot_path: Option<PathBuf>,
    scale: f32,
}

impl Cli {
    fn parse() -> Result<Self> {
        let mut image_path = PathBuf::from(DEFAULT_IMAGE);
        let mut screenshot_path = None;
        let mut scale = 1.0f32;
        let mut args = std::env::args().skip(1);

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--screenshot" => {
                    screenshot_path = Some(PathBuf::from(
                        args.next().context("missing output path after --screenshot")?,
                    ));
                }
                "--scale" => {
                    scale = args
                        .next()
                        .context("missing value after --scale")?
                        .parse()
                        .context("invalid --scale value")?;
                }
                "-h" | "--help" => {
                    println!(
                        "Usage:\n  cargo run --bin custom_renderer -- [image_path]\n  cargo run --bin custom_renderer -- --screenshot out.png [image_path]\n  cargo run --bin custom_renderer -- --scale 0.75"
                    );
                    std::process::exit(0);
                }
                other if other.starts_with('-') => bail!("unrecognized argument: {other}"),
                other => image_path = PathBuf::from(other),
            }
        }

        Ok(Self {
            image_path,
            screenshot_path,
            scale,
        })
    }
}

struct Assets {
    source: DynamicImage,
    mono: FontArc,
    narrow: FontArc,
}

impl Assets {
    fn load(image_path: &Path) -> Result<Self> {
        Ok(Self {
            source: image::open(image_path)
                .with_context(|| format!("failed to open {}", image_path.display()))?,
            mono: load_font(MONO_BOLD)?,
            narrow: load_font(NARROW_BOLD)?,
        })
    }
}

#[derive(Clone, Copy)]
struct CropRect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

fn load_font(path: &str) -> Result<FontArc> {
    FontArc::try_from_vec(fs::read(path).with_context(|| format!("failed to read font {path}"))?)
        .map_err(|_| anyhow::anyhow!("invalid font data at {path}"))
}

fn show_window(frame: &RgbaImage) -> Result<()> {
    let (w, h) = frame.dimensions();
    let mut window = Window::new(
        "Romulus Custom Renderer",
        w as usize,
        h as usize,
        WindowOptions {
            resize: true,
            ..WindowOptions::default()
        },
    )?;

    let mut buffer = rgba_to_u32(frame);
    while window.is_open() && !window.is_key_down(Key::Escape) && !window.is_key_down(Key::Q) {
        let (ww, hh) = window.get_size();
        if ww as u32 != w || hh as u32 != h {
            let scaled = image::imageops::resize(frame, ww as u32, hh as u32, FilterType::Nearest);
            buffer = rgba_to_u32(&scaled);
            window.update_with_buffer(&buffer, ww, hh)?;
        } else {
            window.update_with_buffer(&buffer, w as usize, h as usize)?;
        }
        std::thread::sleep(Duration::from_millis(16));
    }

    Ok(())
}

fn render_scene(assets: &Assets) -> Result<RgbaImage> {
    let mut img = RgbaImage::from_pixel(BASE_W, BASE_H, BG);

    let outer = PxRect::new(12, 22, 1179, 880);
    let header_left = PxRect::new(31, 37, 402, 35);
    let header_center = PxRect::new(438, 37, 696, 35);
    let status_left = PxRect::new(31, 74, 522, 75);
    let badge = PxRect::new(555, 74, 58, 75);
    let status_right = PxRect::new(618, 74, 540, 75);
    let left_panel = PxRect::new(31, 154, 572, 748);
    let portrait = PxRect::new(605, 154, 553, 604);
    let finger = PxRect::new(605, 758, 553, 144);

    stroke_rect(&mut img, outer, BORDER, 3);
    fill_stroke_rect(&mut img, header_left, HEADER_BG, BORDER, 3);
    fill_stroke_rect(&mut img, header_center, HEADER_BG, BORDER, 3);
    fill_stroke_rect(&mut img, status_left, PANEL_BG, BORDER, 3);
    fill_stroke_rect(&mut img, badge, PANEL_BG, BORDER, 3);
    fill_stroke_rect(&mut img, status_right, PANEL_BG, BORDER, 3);
    fill_stroke_rect(&mut img, left_panel, PANEL_BG, BORDER, 3);
    fill_stroke_rect(&mut img, portrait, PANEL_BG, BORDER, 3);
    fill_stroke_rect(&mut img, finger, PANEL_BG, BORDER, 3);

    draw_header(&mut img, assets, header_left, header_center)?;
    draw_status(&mut img, assets, status_left, badge, status_right);
    draw_left_panel(&mut img, assets, left_panel);
    draw_portrait_panel(&mut img, assets, portrait)?;
    draw_fingerprint_panel(&mut img, assets, finger)?;

    Ok(img)
}

fn draw_header(img: &mut RgbaImage, assets: &Assets, left: PxRect, center: PxRect) -> Result<()> {
    let logo_area = PxRect::new(left.x + 10, left.y + 6, 54, 20);
    let logo = crop_norm(&assets.source, LOGO_CROP)?;
    overlay_fit(img, &logo, logo_area.inner(0, 0));
    draw_text(
        img,
        &assets.narrow,
        22.0,
        VALUE,
        left.x + 74,
        left.y + 7,
        "WEYLAND-YUTANI CORP",
    );
    draw_centered_text(
        img,
        &assets.narrow,
        22.0,
        VALUE,
        center,
        center.y + 7,
        "COLONY AFFAIRS DATABASE",
        0.58,
    );
    Ok(())
}

fn draw_status(
    img: &mut RgbaImage,
    assets: &Assets,
    left: PxRect,
    badge: PxRect,
    right: PxRect,
) {
    draw_text(img, &assets.narrow, 18.0, LABEL, left.x + 18, left.y + 14, "USER:");
    draw_text(
        img,
        &assets.narrow,
        20.0,
        VALUE,
        left.x + 139,
        left.y + 11,
        "OFFICER AD",
    );
    draw_text(
        img,
        &assets.narrow,
        18.0,
        LABEL,
        left.x + 18,
        left.y + 38,
        "SETTLEMENT:",
    );
    draw_text(
        img,
        &assets.narrow,
        20.0,
        VALUE,
        left.x + 139,
        left.y + 35,
        "JACKSON'S STAR COLONY",
    );

    let inner = badge.inner(11, 10);
    draw_filled_rect_mut(img, Rect::at(inner.x as i32, inner.y as i32).of_size(inner.w, inner.h), BADGE);
    draw_centered_text(img, &assets.narrow, 24.0, PANEL_BG, inner, inner.y + 5, "A", 0.62);

    draw_text(img, &assets.narrow, 18.0, LABEL, right.x + 22, right.y + 14, "02:03:05");
    draw_text(
        img,
        &assets.narrow,
        22.0,
        VALUE,
        right.x + 188,
        right.y + 11,
        "SYSTEM ONLINE",
    );
    draw_text(img, &assets.narrow, 18.0, LABEL, right.x + 22, right.y + 38, "LOG_ID");
}

fn draw_left_panel(img: &mut RgbaImage, assets: &Assets, area: PxRect) {
    draw_text(img, &assets.narrow, 18.0, LABEL, area.x + 20, area.y + 34, "CITIZEN ID:");
    draw_right_text(
        img,
        &assets.mono,
        28.0,
        VALUE,
        area.right() - 20,
        area.y + 18,
        "FWC25583",
        0.58,
    );

    let dept = PxRect::new(area.x + 14, area.y + 115, 188, 38);
    fill_stroke_rect(img, dept, PANEL_BG, BORDER, 3);
    let dept_label = PxRect::new(dept.x, dept.y, 74, dept.h);
    draw_filled_rect_mut(
        img,
        Rect::at(dept_label.x as i32, dept_label.y as i32).of_size(dept_label.w, dept_label.h),
        LABEL,
    );
    draw_text(img, &assets.narrow, 18.0, PANEL_BG, dept_label.x + 12, dept_label.y + 8, "DEPT.");
    draw_text(img, &assets.narrow, 20.0, VALUE, dept.x + 92, dept.y + 7, "FARMING");

    let gender_x = area.x + 228;
    for (i, (label, active)) in [("M", false), ("F", true), ("O", false)]
        .into_iter()
        .enumerate()
    {
        let box_rect = PxRect::new(gender_x + i as u32 * 47, area.y + 115, 37, 38);
        fill_stroke_rect(img, box_rect, if active { BADGE } else { PANEL_BG }, BORDER, 3);
        draw_centered_text(
            img,
            &assets.narrow,
            22.0,
            if active { PANEL_BG } else { VALUE },
            box_rect,
            box_rect.y + 7,
            label,
            0.55,
        );
    }

    draw_text(img, &assets.mono, 28.0, MUTED, area.x + 8, area.y + 168, ">");
    draw_text(img, &assets.mono, 28.0, MUTED, area.x + 8, area.y + 196, ">");

    let fields = [
        ("Name", "MARIE RAINES CARRADINE", area.y + 248),
        ("Resident", "JACKSON'S STAR", area.y + 301),
        ("Date of Birth", "18 FEBRUARY 2121", area.y + 394),
        ("Birth Place", "EARTH, 21 YRS - AQUARIUS", area.y + 447),
        ("Height", "150 CM", area.y + 500),
        ("Weight", "45 KG", area.y + 553),
    ];
    for (idx, (label, value, y)) in fields.into_iter().enumerate() {
        draw_text(img, &assets.narrow, 18.0, LABEL, area.x + 20, y, &format!("{label}:"));
        draw_right_text(
            img,
            &assets.narrow,
            20.0,
            VALUE,
            area.right() - 18,
            y - 2,
            value,
            0.52,
        );
        let line_y = y + 27;
        draw_dashed_hline(img, area.x + 16, area.right() - 18, line_y, MUTED, 2, 10, 5);
        if idx == 1 {
            draw_dashed_hline(img, area.x + 16, area.right() - 18, area.y + 374, MUTED, 2, 10, 5);
        }
    }

    draw_text(
        img,
        &assets.narrow,
        18.0,
        LABEL,
        area.x + 20,
        area.bottom() - 118,
        "Education Records:",
    );
    draw_right_text(
        img,
        &assets.narrow,
        20.0,
        VALUE,
        area.right() - 18,
        area.bottom() - 120,
        "N/A",
        0.52,
    );
    draw_dashed_hline(
        img,
        area.x + 16,
        area.right() - 18,
        area.bottom() - 92,
        MUTED,
        2,
        10,
        5,
    );

    draw_text(
        img,
        &assets.narrow,
        18.0,
        LABEL,
        area.x + 20,
        area.bottom() - 62,
        "Known Relations:",
    );
    draw_right_text(
        img,
        &assets.narrow,
        20.0,
        VALUE,
        area.right() - 18,
        area.bottom() - 63,
        "CARRADINE, E (DECEASED)",
        0.52,
    );
    draw_right_text(
        img,
        &assets.narrow,
        20.0,
        VALUE,
        area.right() - 18,
        area.bottom() - 32,
        "CARRADINE, S (DECEASED)",
        0.52,
    );
}

fn draw_portrait_panel(img: &mut RgbaImage, assets: &Assets, area: PxRect) -> Result<()> {
    let portrait_crop = crop_norm(&assets.source, PORTRAIT_CROP)?;
    let target = area.inner(4, 4);
    overlay_fit(img, &portrait_crop, target);
    stroke_rect(img, area, BORDER, 3);
    Ok(())
}

fn draw_fingerprint_panel(img: &mut RgbaImage, assets: &Assets, area: PxRect) -> Result<()> {
    let labels_y = area.y + 10;
    let cell_y = area.y + 28;
    let cell_h = area.h - 32;
    let step = area.w / 5;

    for i in 0..5u32 {
        let x = area.x + i * step;
        if i > 0 {
            draw_filled_rect_mut(img, Rect::at(x as i32, area.y as i32).of_size(3, area.h), BORDER);
        }

        draw_centered_text(
            img,
            &assets.narrow,
            18.0,
            VALUE,
            PxRect::new(x, labels_y, step, 20),
            labels_y,
            &format!("{:02}", i + 1),
            0.55,
        );

        let crop = fingerprint_crop(i as usize);
        let sprite = crop_norm(&assets.source, crop)?;
        let cell = PxRect::new(x + 4, cell_y, step.saturating_sub(8), cell_h.saturating_sub(6));
        overlay_fit(img, &sprite, cell);
    }

    stroke_rect(img, area, BORDER, 3);
    Ok(())
}

fn overlay_fit(dst: &mut RgbaImage, src: &DynamicImage, target: PxRect) {
    let resized = image::imageops::resize(
        &src.to_rgba8(),
        target.w.max(1),
        target.h.max(1),
        FilterType::CatmullRom,
    );
    image::imageops::overlay(dst, &resized, target.x.into(), target.y.into());
}

fn resize_image(img: &RgbaImage, scale: f32) -> RgbaImage {
    let w = (img.width() as f32 * scale).round().max(1.0) as u32;
    let h = (img.height() as f32 * scale).round().max(1.0) as u32;
    image::imageops::resize(img, w, h, FilterType::Nearest)
}

fn crop_norm(source: &DynamicImage, crop: CropRect) -> Result<DynamicImage> {
    let (w, h) = source.dimensions();
    let x = (w as f32 * crop.x).round() as u32;
    let y = (h as f32 * crop.y).round() as u32;
    let cw = (w as f32 * crop.w).round() as u32;
    let ch = (h as f32 * crop.h).round() as u32;
    Ok(source.crop_imm(
        x.min(w.saturating_sub(1)),
        y.min(h.saturating_sub(1)),
        cw.min(w.saturating_sub(x)).max(1),
        ch.min(h.saturating_sub(y)).max(1),
    ))
}

fn fingerprint_crop(index: usize) -> CropRect {
    let segment = PRINTS_SOURCE_CROP.w / 5.0;
    let inset = segment * 0.08;
    CropRect {
        x: PRINTS_SOURCE_CROP.x + segment * index as f32 + inset,
        y: PRINTS_SOURCE_CROP.y,
        w: segment - inset * 2.0,
        h: PRINTS_SOURCE_CROP.h,
    }
}

fn rgba_to_u32(img: &RgbaImage) -> Vec<u32> {
    img.pixels()
        .map(|p| ((p[0] as u32) << 16) | ((p[1] as u32) << 8) | p[2] as u32)
        .collect()
}

#[derive(Clone, Copy)]
struct PxRect {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
}

impl PxRect {
    const fn new(x: u32, y: u32, w: u32, h: u32) -> Self {
        Self { x, y, w, h }
    }

    fn right(self) -> u32 {
        self.x + self.w
    }

    fn bottom(self) -> u32 {
        self.y + self.h
    }

    fn inner(self, inset_x: u32, inset_y: u32) -> Self {
        Self {
            x: self.x + inset_x,
            y: self.y + inset_y,
            w: self.w.saturating_sub(inset_x * 2),
            h: self.h.saturating_sub(inset_y * 2),
        }
    }
}

fn draw_text(img: &mut RgbaImage, font: &FontArc, size: f32, color: Rgba<u8>, x: u32, y: u32, text: &str) {
    draw_text_mut(img, color, x as i32, y as i32, size, font, text);
}

fn draw_centered_text(
    img: &mut RgbaImage,
    font: &FontArc,
    size: f32,
    color: Rgba<u8>,
    area: PxRect,
    y: u32,
    text: &str,
    factor: f32,
) {
    let w = estimate_text_width(text, size, factor);
    let x = area.x + area.w.saturating_sub(w) / 2;
    draw_text(img, font, size, color, x, y, text);
}

fn draw_right_text(
    img: &mut RgbaImage,
    font: &FontArc,
    size: f32,
    color: Rgba<u8>,
    right: u32,
    y: u32,
    text: &str,
    factor: f32,
) {
    let w = estimate_text_width(text, size, factor);
    draw_text(img, font, size, color, right.saturating_sub(w), y, text);
}

fn estimate_text_width(text: &str, size: f32, factor: f32) -> u32 {
    (text.chars().count() as f32 * size * factor).round().max(1.0) as u32
}

fn stroke_rect(img: &mut RgbaImage, rect: PxRect, color: Rgba<u8>, thickness: u32) {
    for inset in 0..thickness {
        let r = Rect::at((rect.x + inset) as i32, (rect.y + inset) as i32)
            .of_size(rect.w.saturating_sub(inset * 2), rect.h.saturating_sub(inset * 2));
        draw_hollow_rect_mut(img, r, color);
    }
}

fn fill_stroke_rect(
    img: &mut RgbaImage,
    rect: PxRect,
    fill: Rgba<u8>,
    stroke: Rgba<u8>,
    thickness: u32,
) {
    draw_filled_rect_mut(img, Rect::at(rect.x as i32, rect.y as i32).of_size(rect.w, rect.h), fill);
    stroke_rect(img, rect, stroke, thickness);
}

fn draw_dashed_hline(
    img: &mut RgbaImage,
    x1: u32,
    x2: u32,
    y: u32,
    color: Rgba<u8>,
    thickness: u32,
    dash: u32,
    gap: u32,
) {
    let mut x = x1;
    while x < x2 {
        let seg = (x + dash).min(x2);
        draw_filled_rect_mut(
            img,
            Rect::at(x as i32, y as i32).of_size(seg.saturating_sub(x).max(1), thickness),
            color,
        );
        x = seg + gap;
    }
}
