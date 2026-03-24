use std::{cmp::Ordering, fs, path::PathBuf};

use ab_glyph::FontArc;
use anyhow::{Context, Result, bail};
use image::{GrayImage, ImageBuffer, Luma, Rgba, RgbaImage, imageops::FilterType};
use imageproc::{
    drawing::{draw_filled_rect_mut, draw_hollow_rect_mut, draw_line_segment_mut, draw_text_mut},
    edges::canny,
    rect::Rect,
};

const DEFAULT_REFERENCE: &str = "HEI1Ts9aIAETw1k.jpg";
const DEFAULT_SHOT: &str = "tui-shot.png";
const DEFAULT_FONT: &str = "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf";

fn main() -> Result<()> {
    let cli = Cli::parse()?;
    fs::create_dir_all(&cli.out_dir)?;

    let font = load_font()?;
    let reference = image::open(&cli.reference)?.to_rgba8();
    let shot = image::open(&cli.shot)?.to_rgba8();

    let reference_frame = detect_frame_rect(&reference);
    let shot_frame = detect_frame_rect(&shot);
    let registered_shot = register_to_reference(&reference, &shot, reference_frame, shot_frame);
    let blend = blend_images(&reference, &registered_shot, 0.5);
    let delta = delta_heatmap(&reference, &registered_shot);
    let edges = edge_overlay(&reference, &registered_shot);
    let metrics = compute_metrics(&reference, &registered_shot, cli.grid_x, cli.grid_y);
    let cells = cell_heatmap(
        reference.dimensions(),
        &metrics,
        &font,
        cli.grid_x,
        cli.grid_y,
        reference_frame,
        shot_frame,
    );

    let ref_grid = overlay_grid(reference.clone(), &font, cli.grid_x, cli.grid_y, "reference");
    let shot_grid = overlay_grid(registered_shot, &font, cli.grid_x, cli.grid_y, "shot-registered");
    let blend_grid = overlay_grid(blend, &font, cli.grid_x, cli.grid_y, "blend");
    let delta_grid = overlay_grid(delta, &font, cli.grid_x, cli.grid_y, "delta");
    let edges_grid = overlay_grid(edges, &font, cli.grid_x, cli.grid_y, "edges");
    let cells_grid = overlay_grid(cells, &font, cli.grid_x, cli.grid_y, "cell-heat");

    let ref_path = cli.out_dir.join("trace-grid-reference.png");
    let shot_path = cli.out_dir.join("trace-grid-shot.png");
    let blend_path = cli.out_dir.join("trace-grid-blend.png");
    let delta_path = cli.out_dir.join("trace-grid-delta.png");
    let edges_path = cli.out_dir.join("trace-grid-edges.png");
    let cells_path = cli.out_dir.join("trace-grid-cells.png");
    let compare_path = cli.out_dir.join("trace-grid-compare.png");
    let metrics_path = cli.out_dir.join("trace-grid-metrics.txt");

    ref_grid.save(&ref_path)?;
    shot_grid.save(&shot_path)?;
    blend_grid.save(&blend_path)?;
    delta_grid.save(&delta_path)?;
    edges_grid.save(&edges_path)?;
    cells_grid.save(&cells_path)?;

    let compare = gallery(
        [
            ("reference", &ref_grid),
            ("shot", &shot_grid),
            ("blend", &blend_grid),
            ("delta", &delta_grid),
            ("edges", &edges_grid),
            ("cells", &cells_grid),
        ],
        &font,
    );
    compare.save(&compare_path)?;

    fs::write(
        &metrics_path,
        render_metrics_report(&metrics, reference_frame, shot_frame),
    )?;

    println!("wrote {}", ref_path.display());
    println!("wrote {}", shot_path.display());
    println!("wrote {}", blend_path.display());
    println!("wrote {}", delta_path.display());
    println!("wrote {}", edges_path.display());
    println!("wrote {}", cells_path.display());
    println!("wrote {}", compare_path.display());
    println!("wrote {}", metrics_path.display());
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

#[derive(Clone, Copy, Debug)]
struct BoxRect {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
}

impl BoxRect {
    fn right(self) -> u32 {
        self.x + self.w
    }

    fn bottom(self) -> u32 {
        self.y + self.h
    }
}

#[derive(Clone, Debug)]
struct CellMetric {
    col: u32,
    row: u32,
    mean_error: f32,
}

#[derive(Clone, Debug)]
struct Metrics {
    overall_mean_error: f32,
    peak_error: f32,
    cells: Vec<CellMetric>,
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

fn detect_frame_rect(image: &RgbaImage) -> BoxRect {
    let (w, h) = image.dimensions();
    let col_threshold = (h / 4).max(8);
    let row_threshold = (w / 4).max(8);

    let left = (0..w).find(|&x| borderish_count_col(image, x) >= col_threshold);
    let right = (0..w)
        .rev()
        .find(|&x| borderish_count_col(image, x) >= col_threshold);
    let top = (0..h).find(|&y| borderish_count_row(image, y) >= row_threshold);
    let bottom = (0..h)
        .rev()
        .find(|&y| borderish_count_row(image, y) >= row_threshold);

    if let (Some(left), Some(right), Some(top), Some(bottom)) = (left, right, top, bottom) {
        return expand_rect(
            BoxRect {
                x: left,
                y: top,
                w: right.saturating_sub(left) + 1,
                h: bottom.saturating_sub(top) + 1,
            },
            2,
            w,
            h,
        );
    }

    detect_active_rect(image)
}

fn borderish_count_col(image: &RgbaImage, x: u32) -> u32 {
    let mut count = 0;
    for y in 0..image.height() {
        if is_borderish(image.get_pixel(x, y).0) {
            count += 1;
        }
    }
    count
}

fn borderish_count_row(image: &RgbaImage, y: u32) -> u32 {
    let mut count = 0;
    for x in 0..image.width() {
        if is_borderish(image.get_pixel(x, y).0) {
            count += 1;
        }
    }
    count
}

fn is_borderish(pixel: [u8; 4]) -> bool {
    let [r, g, b, _] = pixel;
    g > 48 && b > 30 && g.saturating_sub(r) > 28
}

fn detect_active_rect(image: &RgbaImage) -> BoxRect {
    let (w, h) = image.dimensions();
    let bg = average_corner_color(image);
    let mut min_x = w;
    let mut min_y = h;
    let mut max_x = 0;
    let mut max_y = 0;
    let mut found = false;

    for y in 0..h {
        for x in 0..w {
            if is_active(image.get_pixel(x, y).0, bg) {
                found = true;
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
    }

    if !found {
        return BoxRect { x: 0, y: 0, w, h };
    }

    expand_rect(
        BoxRect {
            x: min_x,
            y: min_y,
            w: max_x.saturating_sub(min_x) + 1,
            h: max_y.saturating_sub(min_y) + 1,
        },
        2,
        w,
        h,
    )
}

fn is_active(pixel: [u8; 4], bg: [u8; 4]) -> bool {
    let delta = color_distance(pixel, bg);
    let luma = rgb_luma(pixel);
    delta > 18.0 || luma > 18.0
}

fn average_corner_color(image: &RgbaImage) -> [u8; 4] {
    let (w, h) = image.dimensions();
    let points = [
        image.get_pixel(0, 0).0,
        image.get_pixel(w.saturating_sub(1), 0).0,
        image.get_pixel(0, h.saturating_sub(1)).0,
        image.get_pixel(w.saturating_sub(1), h.saturating_sub(1)).0,
    ];
    let mut sum = [0u32; 4];
    for point in points {
        for i in 0..4 {
            sum[i] += point[i] as u32;
        }
    }
    [
        (sum[0] / 4) as u8,
        (sum[1] / 4) as u8,
        (sum[2] / 4) as u8,
        (sum[3] / 4) as u8,
    ]
}

fn color_distance(a: [u8; 4], b: [u8; 4]) -> f32 {
    let dr = a[0] as f32 - b[0] as f32;
    let dg = a[1] as f32 - b[1] as f32;
    let db = a[2] as f32 - b[2] as f32;
    (dr * dr + dg * dg + db * db).sqrt()
}

fn rgb_luma(pixel: [u8; 4]) -> f32 {
    0.2126 * pixel[0] as f32 + 0.7152 * pixel[1] as f32 + 0.0722 * pixel[2] as f32
}

fn expand_rect(rect: BoxRect, padding: u32, max_w: u32, max_h: u32) -> BoxRect {
    let x = rect.x.saturating_sub(padding);
    let y = rect.y.saturating_sub(padding);
    let right = (rect.right() + padding).min(max_w);
    let bottom = (rect.bottom() + padding).min(max_h);
    BoxRect {
        x,
        y,
        w: right.saturating_sub(x).max(1),
        h: bottom.saturating_sub(y).max(1),
    }
}

fn register_to_reference(
    reference: &RgbaImage,
    shot: &RgbaImage,
    reference_frame: BoxRect,
    shot_frame: BoxRect,
) -> RgbaImage {
    let (w, h) = reference.dimensions();
    let mut canvas = ImageBuffer::from_pixel(w, h, Rgba(average_corner_color(reference)));
    let cropped = image::imageops::crop_imm(shot, shot_frame.x, shot_frame.y, shot_frame.w, shot_frame.h)
        .to_image();
    let fitted = image::imageops::resize(
        &cropped,
        reference_frame.w,
        reference_frame.h,
        FilterType::Triangle,
    );
    image::imageops::overlay(
        &mut canvas,
        &fitted,
        reference_frame.x as i64,
        reference_frame.y as i64,
    );
    canvas
}

fn blend_images(a: &RgbaImage, b: &RgbaImage, alpha: f32) -> RgbaImage {
    let (w, h) = a.dimensions();
    let mut out = ImageBuffer::from_pixel(w, h, Rgba([0, 0, 0, 255]));
    for y in 0..h {
        for x in 0..w {
            let pa = a.get_pixel(x, y).0;
            let pb = b.get_pixel(x, y).0;
            let mix = |va: u8, vb: u8| ((va as f32 * (1.0 - alpha)) + (vb as f32 * alpha)) as u8;
            out.put_pixel(
                x,
                y,
                Rgba([mix(pa[0], pb[0]), mix(pa[1], pb[1]), mix(pa[2], pb[2]), 255]),
            );
        }
    }
    out
}

fn delta_heatmap(reference: &RgbaImage, shot: &RgbaImage) -> RgbaImage {
    let (w, h) = reference.dimensions();
    let mut out = ImageBuffer::from_pixel(w, h, Rgba([8, 10, 10, 255]));
    for y in 0..h {
        for x in 0..w {
            let a = reference.get_pixel(x, y).0;
            let b = shot.get_pixel(x, y).0;
            let error = mean_abs_diff(a, b);
            let t = (error / 255.0).powf(0.82);
            let heat = if t < 0.12 {
                [15, 35, 30]
            } else if t < 0.24 {
                [45, 95, 80]
            } else if t < 0.40 {
                [140, 120, 40]
            } else if t < 0.58 {
                [220, 130, 50]
            } else {
                [255, 70, 140]
            };
            let base = a;
            let mix = 0.18 + 0.70 * t;
            out.put_pixel(
                x,
                y,
                Rgba([
                    lerp_u8(base[0], heat[0], mix),
                    lerp_u8(base[1], heat[1], mix),
                    lerp_u8(base[2], heat[2], mix),
                    255,
                ]),
            );
        }
    }
    out
}

fn edge_overlay(reference: &RgbaImage, shot: &RgbaImage) -> RgbaImage {
    let ref_edges = canny(&to_luma(reference), 28.0, 92.0);
    let shot_edges = canny(&to_luma(shot), 28.0, 92.0);
    let (w, h) = reference.dimensions();
    let mut out = ImageBuffer::from_pixel(w, h, Rgba([8, 10, 12, 255]));
    for y in 0..h {
        for x in 0..w {
            let r = ref_edges.get_pixel(x, y)[0] > 0;
            let s = shot_edges.get_pixel(x, y)[0] > 0;
            let px = match (r, s) {
                (true, true) => Rgba([245, 245, 245, 255]),
                (true, false) => Rgba([60, 255, 220, 255]),
                (false, true) => Rgba([255, 180, 80, 255]),
                (false, false) => Rgba([8, 10, 12, 255]),
            };
            out.put_pixel(x, y, px);
        }
    }
    out
}

fn to_luma(image: &RgbaImage) -> GrayImage {
    let (w, h) = image.dimensions();
    let mut gray = GrayImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            gray.put_pixel(x, y, Luma([rgb_luma(image.get_pixel(x, y).0) as u8]));
        }
    }
    gray
}

fn compute_metrics(reference: &RgbaImage, shot: &RgbaImage, grid_x: u32, grid_y: u32) -> Metrics {
    let (w, h) = reference.dimensions();
    let step_x = w as f32 / grid_x.max(1) as f32;
    let step_y = h as f32 / grid_y.max(1) as f32;
    let mut cells = Vec::new();
    let mut overall_sum = 0.0f64;
    let mut overall_count = 0u64;
    let mut peak_error = 0.0f32;

    for row in 0..grid_y {
        let y0 = (row as f32 * step_y).floor() as u32;
        let y1 = (((row + 1) as f32 * step_y).ceil() as u32).min(h);
        for col in 0..grid_x {
            let x0 = (col as f32 * step_x).floor() as u32;
            let x1 = (((col + 1) as f32 * step_x).ceil() as u32).min(w);
            let mut cell_sum = 0.0f64;
            let mut cell_count = 0u64;
            for y in y0..y1 {
                for x in x0..x1 {
                    let error = mean_abs_diff(reference.get_pixel(x, y).0, shot.get_pixel(x, y).0);
                    cell_sum += error as f64;
                    overall_sum += error as f64;
                    cell_count += 1;
                    overall_count += 1;
                    peak_error = peak_error.max(error / 255.0);
                }
            }
            let mean_error = if cell_count == 0 {
                0.0
            } else {
                (cell_sum / cell_count as f64) as f32 / 255.0
            };
            cells.push(CellMetric { col, row, mean_error });
        }
    }

    Metrics {
        overall_mean_error: if overall_count == 0 {
            0.0
        } else {
            (overall_sum / overall_count as f64) as f32 / 255.0
        },
        peak_error,
        cells,
    }
}

fn cell_heatmap(
    dims: (u32, u32),
    metrics: &Metrics,
    font: &FontArc,
    grid_x: u32,
    grid_y: u32,
    reference_frame: BoxRect,
    shot_frame: BoxRect,
) -> RgbaImage {
    let (w, h) = dims;
    let mut image = ImageBuffer::from_pixel(w, h, Rgba([9, 12, 12, 255]));
    let step_x = w as f32 / grid_x.max(1) as f32;
    let step_y = h as f32 / grid_y.max(1) as f32;

    for metric in &metrics.cells {
        let x0 = (metric.col as f32 * step_x).round() as i32;
        let y0 = (metric.row as f32 * step_y).round() as i32;
        let x1 = (((metric.col + 1) as f32 * step_x).round() as i32).max(x0 + 1);
        let y1 = (((metric.row + 1) as f32 * step_y).round() as i32).max(y0 + 1);
        let error = metric.mean_error;
        let fill = if error < 0.04 {
            Rgba([12, 30, 26, 255])
        } else if error < 0.08 {
            Rgba([28, 72, 62, 255])
        } else if error < 0.12 {
            Rgba([92, 92, 44, 255])
        } else if error < 0.18 {
            Rgba([150, 98, 42, 255])
        } else {
            Rgba([180, 52, 86, 255])
        };
        draw_filled_rect_mut(
            &mut image,
            Rect::at(x0, y0).of_size((x1 - x0) as u32, (y1 - y0) as u32),
            fill,
        );
    }

    let mut worst = metrics.cells.clone();
    worst.sort_by(|a, b| b.mean_error.partial_cmp(&a.mean_error).unwrap_or(Ordering::Equal));
    for metric in worst.into_iter().take(8) {
        let cx = ((metric.col as f32 + 0.5) * step_x) as i32;
        let cy = ((metric.row as f32 + 0.5) * step_y) as i32;
        let label = format!("{} {:.0}%", cell_name(metric.col, metric.row), metric.mean_error * 100.0);
        draw_filled_rect_mut(&mut image, Rect::at(cx - 30, cy - 8).of_size(60, 16), Rgba([0, 0, 0, 200]));
        draw_text_mut(&mut image, Rgba([255, 255, 255, 255]), cx - 26, cy - 6, 12.0, font, &label);
    }

    draw_hollow_rect_mut(
        &mut image,
        Rect::at(reference_frame.x as i32, reference_frame.y as i32)
            .of_size(reference_frame.w.max(1), reference_frame.h.max(1)),
        Rgba([0, 255, 220, 255]),
    );
    draw_filled_rect_mut(&mut image, Rect::at(14, 14).of_size(270, 36), Rgba([0, 0, 0, 180]));
    draw_text_mut(
        &mut image,
        Rgba([255, 255, 255, 255]),
        18,
        18,
        16.0,
        font,
        &format!(
            "ref frame: x={} y={} w={} h={} | shot frame: x={} y={} w={} h={}",
            reference_frame.x,
            reference_frame.y,
            reference_frame.w,
            reference_frame.h,
            shot_frame.x,
            shot_frame.y,
            shot_frame.w,
            shot_frame.h,
        ),
    );
    image
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
            draw_filled_rect_mut(&mut image, Rect::at(tx - 2, 4).of_size(60, 16), panel);
            draw_text_mut(
                &mut image,
                Rgba([255, 255, 255, 255]),
                tx,
                4,
                12.0,
                font,
                &format!("{} {:.3}", column_name(i), i as f32 / grid_x as f32),
            );
        }
    }

    for i in 0..=grid_y {
        let y = (i as f32 * step_y).round();
        draw_line_segment_mut(
            &mut image,
            (0.0, y),
            (w as f32, y),
            if i % 3 == 0 { accent } else { Rgba([150, 0, 120, 150]) },
        );
        if i < grid_y {
            let ty = (y + 4.0) as i32;
            draw_filled_rect_mut(&mut image, Rect::at(4, ty - 2).of_size(58, 16), panel);
            draw_text_mut(
                &mut image,
                Rgba([255, 255, 255, 255]),
                6,
                ty,
                12.0,
                font,
                &format!("{} {:.3}", i + 1, i as f32 / grid_y as f32),
            );
        }
    }

    draw_hollow_rect_mut(
        &mut image,
        Rect::at(0, 0).of_size(w.saturating_sub(1), h.saturating_sub(1)),
        major,
    );
    draw_filled_rect_mut(&mut image, Rect::at(12, 12).of_size(176, 22), panel);
    draw_text_mut(&mut image, Rgba([255, 255, 255, 255]), 18, 16, 16.0, font, label);
    image
}

fn gallery<const N: usize>(items: [(&str, &RgbaImage); N], font: &FontArc) -> RgbaImage {
    let gap = 22;
    let title_h = 40;
    let cols = 3u32;
    let rows = (N as u32).div_ceil(cols);
    let (w, h) = items[0].1.dimensions();
    let canvas_w = cols * w + (cols + 1) * gap;
    let canvas_h = rows * (h + title_h) + (rows + 1) * gap;
    let mut canvas = ImageBuffer::from_pixel(canvas_w, canvas_h, Rgba([10, 12, 12, 255]));

    for (index, (label, image)) in items.into_iter().enumerate() {
        let col = index as u32 % cols;
        let row = index as u32 / cols;
        let x = gap + col * (w + gap);
        let y = gap + row * (h + title_h + gap / 2);
        draw_text_mut(&mut canvas, Rgba([255, 255, 255, 255]), x as i32, y as i32, 18.0, font, label);
        image::imageops::overlay(&mut canvas, image, x.into(), (y + title_h).into());
    }

    canvas
}

fn render_metrics_report(metrics: &Metrics, reference_frame: BoxRect, shot_frame: BoxRect) -> String {
    let mut cells = metrics.cells.clone();
    cells.sort_by(|a, b| b.mean_error.partial_cmp(&a.mean_error).unwrap_or(Ordering::Equal));

    let mut out = String::new();
    out.push_str("Tracing metrics\n");
    out.push_str("===============\n\n");
    out.push_str(&format!(
        "reference frame: x={} y={} w={} h={}\n",
        reference_frame.x, reference_frame.y, reference_frame.w, reference_frame.h
    ));
    out.push_str(&format!(
        "shot frame:      x={} y={} w={} h={}\n\n",
        shot_frame.x, shot_frame.y, shot_frame.w, shot_frame.h
    ));
    out.push_str(&format!(
        "overall mean absolute error: {:.2}%\npeak pixel error: {:.2}%\n\n",
        metrics.overall_mean_error * 100.0,
        metrics.peak_error * 100.0,
    ));
    out.push_str("worst cells\n-----------\n");
    for cell in cells.into_iter().take(20) {
        out.push_str(&format!("{}  {:.2}%\n", cell_name(cell.col, cell.row), cell.mean_error * 100.0));
    }
    out
}

fn column_name(index: u32) -> String {
    let mut n = index + 1;
    let mut out = String::new();
    while n > 0 {
        let rem = ((n - 1) % 26) as u8;
        out.insert(0, (b'A' + rem) as char);
        n = (n - 1) / 26;
    }
    out
}

fn cell_name(col: u32, row: u32) -> String {
    format!("{}{}", column_name(col), row + 1)
}

fn mean_abs_diff(a: [u8; 4], b: [u8; 4]) -> f32 {
    ((a[0].abs_diff(b[0]) as u32 + a[1].abs_diff(b[1]) as u32 + a[2].abs_diff(b[2]) as u32) as f32)
        / 3.0
}

fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t.clamp(0.0, 1.0)).round() as u8
}
