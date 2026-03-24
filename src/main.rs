use std::{
    fs,
    io::{self, Stdout},
    path::{Path, PathBuf},
    time::Duration,
};

use ab_glyph::FontArc;
use color_eyre::{
    Result,
    eyre::{OptionExt, eyre},
};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use image::{DynamicImage, Rgba, RgbaImage, imageops::FilterType};
use imageproc::{
    drawing::{draw_filled_rect_mut, draw_text_mut},
    rect::Rect as PixelRect,
};
use ratatui::{
    Terminal,
    backend::{CrosstermBackend, TestBackend},
    buffer::Buffer,
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
};

const BG: Color = Color::Rgb(5, 10, 9);
const PANEL_BG: Color = Color::Rgb(8, 15, 12);
const BORDER: Color = Color::Rgb(18, 126, 92);
const HEADER_BG: Color = Color::Rgb(26, 82, 65);
const LABEL: Color = Color::Rgb(77, 181, 133);
const MUTED: Color = Color::Rgb(27, 78, 63);
const VALUE: Color = Color::Rgb(227, 183, 100);
const PHOTO_BRIGHT: Color = Color::Rgb(229, 205, 126);
const PHOTO_DIM: Color = Color::Rgb(96, 76, 36);
const PRINT_BRIGHT: Color = Color::Rgb(139, 207, 164);
const PRINT_DIM: Color = Color::Rgb(53, 100, 80);
const MIN_WIDTH: u16 = 100;
const MIN_HEIGHT: u16 = 34;

const DEFAULT_IMAGE: &str = "HEI1Ts9aIAETw1k.jpg";
const DEFAULT_FONT: &str = "/usr/share/fonts/truetype/dejavu/DejaVuSansMono-Bold.ttf";
const SCREENSHOT_COLS: u16 = 100;
const SCREENSHOT_ROWS: u16 = 45;
const CELL_WIDTH_PX: u32 = 12;
const CELL_HEIGHT_PX: u32 = 20;
const TEXT_SIZE_PX: f32 = 16.0;

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

fn main() -> Result<()> {
    color_eyre::install()?;

    let cli = Cli::parse()?;
    let app = App {
        art: ReferenceArt::load(&cli.image_path).ok(),
        image_path: cli.image_path,
    };

    if let Some(path) = cli.screenshot_path {
        app.capture_png(&path, SCREENSHOT_COLS, SCREENSHOT_ROWS)?;
        println!("saved screenshot to {}", path.display());
        return Ok(());
    }

    let mut terminal = setup_terminal()?;
    let result = app.run(&mut terminal);
    restore_terminal(&mut terminal)?;
    result
}

struct Cli {
    image_path: PathBuf,
    screenshot_path: Option<PathBuf>,
}

impl Cli {
    fn parse() -> Result<Self> {
        let mut image_path = PathBuf::from(DEFAULT_IMAGE);
        let mut screenshot_path = None;
        let mut args = std::env::args().skip(1);

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--screenshot" => {
                    let output = args
                        .next()
                        .ok_or_else(|| eyre!("missing output path after --screenshot"))?;
                    screenshot_path = Some(PathBuf::from(output));
                }
                "-h" | "--help" => {
                    println!(
                        "Usage:\n  romulus [image_path]\n  romulus --screenshot output.png [image_path]"
                    );
                    std::process::exit(0);
                }
                other => image_path = PathBuf::from(other),
            }
        }

        Ok(Self {
            image_path,
            screenshot_path,
        })
    }
}

fn setup_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    Ok(Terminal::new(CrosstermBackend::new(stdout))?)
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

struct App {
    art: Option<ReferenceArt>,
    image_path: PathBuf,
}

impl App {
    fn run(self, terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
        loop {
            terminal.draw(|frame| self.render(frame))?;

            if !event::poll(Duration::from_millis(200))? {
                continue;
            }

            let Event::Key(key) = event::read()? else {
                continue;
            };

            if key.kind != KeyEventKind::Press {
                continue;
            }

            match key.code {
                KeyCode::Esc | KeyCode::Char('q') => break,
                _ => {}
            }
        }

        Ok(())
    }

    fn capture_png(&self, output_path: &Path, cols: u16, rows: u16) -> Result<()> {
        let backend = TestBackend::new(cols, rows);
        let mut terminal = Terminal::new(backend)?;
        terminal.draw(|frame| self.render(frame))?;
        render_buffer_png(terminal.backend().buffer(), output_path)
    }

    fn render(&self, frame: &mut ratatui::Frame) {
        let area = frame.area();
        frame.render_widget(Block::default().style(Style::default().bg(BG)), area);

        if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
            let warning = Paragraph::new(Text::from(vec![
                Line::styled(
                    "ROMULUS // COLONY DOSSIER",
                    value_style().add_modifier(Modifier::BOLD),
                ),
                Line::raw(""),
                Line::styled(
                    format!("Resize terminal to at least {MIN_WIDTH}x{MIN_HEIGHT}."),
                    label_style(),
                ),
                Line::styled("Press q or Esc to exit.", muted_style()),
            ]))
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: false });
            frame.render_widget(warning, area);
            return;
        }

        let canvas = centered(area, area.width.saturating_sub(2), area.height.saturating_sub(2));
        let outer = dossier_block();
        frame.render_widget(outer.clone(), canvas);
        let inner = outer.inner(canvas);

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(5),
                Constraint::Min(0),
            ])
            .split(inner);

        self.render_header(frame, rows[0]);
        self.render_status_bar(frame, rows[1]);
        self.render_body(frame, rows[2]);
    }

    fn render_header(&self, frame: &mut ratatui::Frame, area: Rect) {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(43), Constraint::Min(0)])
            .split(area);

        let left_block = bar_block();
        frame.render_widget(left_block.clone(), columns[0]);
        let left_inner = left_block.inner(columns[0]).inner(Margin {
            horizontal: 1,
            vertical: 0,
        });
        let left_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(4), Constraint::Min(0)])
            .split(left_inner);

        frame.render_widget(
            Paragraph::new(Line::styled(
                "W",
                Style::default()
                    .fg(VALUE)
                    .bg(PANEL_BG)
                    .add_modifier(Modifier::BOLD),
            ))
            .style(Style::default().bg(PANEL_BG))
            .alignment(Alignment::Center),
            left_layout[0],
        );
        frame.render_widget(
            Paragraph::new(Line::styled(
                "WEYLAND-YUTANI CORP",
                Style::default()
                    .fg(VALUE)
                    .bg(HEADER_BG)
                    .add_modifier(Modifier::BOLD),
            ))
            .style(Style::default().bg(HEADER_BG))
            .alignment(Alignment::Left),
            left_layout[1],
        );

        frame.render_widget(
            Paragraph::new(Line::styled(
                "COLONY AFFAIRS DATABASE",
                Style::default()
                    .fg(Color::Rgb(178, 197, 128))
                    .bg(HEADER_BG)
                    .add_modifier(Modifier::BOLD),
            ))
            .block(bar_block())
            .style(Style::default().bg(HEADER_BG))
            .alignment(Alignment::Center),
            columns[1],
        );
    }

    fn render_status_bar(&self, frame: &mut ratatui::Frame, area: Rect) {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(0), Constraint::Length(41)])
            .split(area);

        let left = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(0), Constraint::Length(7)])
            .split(columns[0]);

        let meta = Text::from(vec![
            Line::from(vec![
                Span::styled(" USER:", label_style()),
                Span::raw("   "),
                Span::styled("OFFICER AD", value_style()),
            ]),
            Line::from(vec![
                Span::styled(" SETTLEMENT:", label_style()),
                Span::raw(" "),
                Span::styled("JACKSON'S STAR COLONY", value_style()),
            ]),
        ]);

        frame.render_widget(Paragraph::new(meta).block(panel_block()), left[0]);
        let badge_block = panel_block();
        frame.render_widget(badge_block.clone(), left[1]);
        let badge_inner = badge_block.inner(left[1]).inner(Margin {
            horizontal: 1,
            vertical: 1,
        });
        frame.render_widget(
            Paragraph::new(Line::styled(
                "A",
                Style::default()
                    .fg(BG)
                    .bg(VALUE)
                    .add_modifier(Modifier::BOLD),
            ))
            .style(Style::default().bg(VALUE))
            .alignment(Alignment::Center),
            badge_inner,
        );

        let right = Text::from(vec![
            Line::from(vec![
                Span::styled(" 02:03:05", label_style()),
                Span::raw("   "),
                Span::styled("SYSTEM ONLINE", value_style().add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::styled(" LOG_ID", label_style()),
                Span::raw(" "),
            ]),
        ]);

        frame.render_widget(Paragraph::new(right).block(panel_block()), columns[1]);
    }

    fn render_body(&self, frame: &mut ratatui::Frame, area: Rect) {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(51), Constraint::Percentage(49)])
            .split(area);

        self.render_info_panel(frame, columns[0]);
        self.render_media_panel(frame, columns[1]);
    }

    fn render_info_panel(&self, frame: &mut ratatui::Frame, area: Rect) {
        let block = panel_block();
        frame.render_widget(block.clone(), area);
        let inner = block.inner(area).inner(Margin {
            vertical: 1,
            horizontal: 1,
        });

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2),
                Constraint::Length(3),
                Constraint::Length(2),
                Constraint::Min(0),
            ])
            .split(inner);

        let citizen = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(18), Constraint::Min(0)])
            .split(rows[0]);
        frame.render_widget(
            Paragraph::new(Line::styled("CITIZEN ID:", label_style())).alignment(Alignment::Left),
            citizen[0],
        );
        frame.render_widget(
            Paragraph::new(Line::styled(
                "FWC25583",
                value_style().add_modifier(Modifier::BOLD),
            ))
            .alignment(Alignment::Right),
            citizen[1],
        );

        let dept_gender = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(28), Constraint::Min(0)])
            .split(rows[1]);
        self.render_department(frame, dept_gender[0]);
        self.render_gender(frame, dept_gender[1]);

        frame.render_widget(
            Paragraph::new(Text::from(vec![
                Line::styled(">", muted_style().add_modifier(Modifier::BOLD)),
                Line::styled(">", muted_style().add_modifier(Modifier::BOLD)),
            ])),
            rows[2],
        );

        render_profile_details(frame, rows[3]);
    }

    fn render_department(&self, frame: &mut ratatui::Frame, area: Rect) {
        let block = panel_block();
        frame.render_widget(block.clone(), area);
        let inner = block.inner(area);
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(8), Constraint::Min(0)])
            .split(inner);

        frame.render_widget(
            Paragraph::new(Line::styled(
                "DEPT.",
                Style::default()
                    .fg(BG)
                    .bg(LABEL)
                    .add_modifier(Modifier::BOLD),
            ))
            .alignment(Alignment::Center)
            .style(Style::default().bg(LABEL)),
            columns[0],
        );
        frame.render_widget(
            Paragraph::new(Line::styled("FARMING", value_style()))
                .alignment(Alignment::Center)
                .style(Style::default().bg(PANEL_BG)),
            columns[1],
        );
    }

    fn render_gender(&self, frame: &mut ratatui::Frame, area: Rect) {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(5),
                Constraint::Length(5),
                Constraint::Length(5),
                Constraint::Min(0),
            ])
            .split(area);

        for (index, label) in ["M", "F", "O"].into_iter().enumerate() {
            let active = label == "F";
            frame.render_widget(
                Paragraph::new(Line::styled(
                    label,
                    if active {
                        Style::default()
                            .fg(BG)
                            .bg(VALUE)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        value_style()
                    },
                ))
                .block(panel_block())
                .alignment(Alignment::Center)
                .style(Style::default().bg(if active { VALUE } else { PANEL_BG })),
                columns[index],
            );
        }
    }

    fn render_media_panel(&self, frame: &mut ratatui::Frame, area: Rect) {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(6)])
            .split(area);

        self.render_portrait(frame, rows[0]);
        self.render_fingerprints(frame, rows[1]);
    }

    fn render_portrait(&self, frame: &mut ratatui::Frame, area: Rect) {
        let block = panel_block();
        frame.render_widget(block.clone(), area);
        let inner = block.inner(area);

        let art = self
            .art
            .as_ref()
            .map(|art| {
                art.render_tinted_blocks(
                    PORTRAIT_CROP,
                    inner.width,
                    inner.height,
                    PHOTO_DIM,
                    PHOTO_BRIGHT,
                    0.94,
                    1.35,
                )
            })
            .unwrap_or_else(|| missing_art(inner.width, inner.height, &self.image_path));

        frame.render_widget(
            Paragraph::new(art)
                .wrap(Wrap { trim: false })
                .style(Style::default().bg(PANEL_BG)),
            inner,
        );
    }

    fn render_fingerprints(&self, frame: &mut ratatui::Frame, area: Rect) {
        let block = panel_block();
        frame.render_widget(block.clone(), area);
        let inner = block.inner(area);
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(0)])
            .split(inner);

        let labels = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(20),
                Constraint::Percentage(20),
                Constraint::Percentage(20),
                Constraint::Percentage(20),
                Constraint::Percentage(20),
            ])
            .split(rows[0]);

        for (i, label) in ["01", "02", "03", "04", "05"].into_iter().enumerate() {
            frame.render_widget(
                Paragraph::new(Line::styled(label, value_style()))
                    .alignment(Alignment::Center)
                    .style(Style::default().bg(PANEL_BG)),
                labels[i],
            );
        }

        let cells = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(20),
                Constraint::Percentage(20),
                Constraint::Percentage(20),
                Constraint::Percentage(20),
                Constraint::Percentage(20),
            ])
            .split(rows[1]);

        for (index, area) in cells.iter().copied().enumerate() {
            if index > 0 {
                frame.render_widget(
                    Block::default()
                        .borders(Borders::LEFT)
                        .border_style(Style::default().fg(BORDER))
                        .style(Style::default().bg(PANEL_BG)),
                    area,
                );
            }
            let inner = if index > 0 {
                area.inner(Margin {
                    horizontal: 1,
                    vertical: 0,
                })
            } else {
                area
            };

            let art = self
                .art
                .as_ref()
                .map(|art| {
                    art.render_tinted_blocks(
                        fingerprint_crop(index),
                        inner.width,
                        inner.height,
                        PRINT_DIM,
                        PRINT_BRIGHT,
                        1.0,
                        3.0,
                    )
                })
                .unwrap_or_else(|| missing_art(inner.width, inner.height, &self.image_path));

            frame.render_widget(
                Paragraph::new(art)
                    .wrap(Wrap { trim: false })
                    .style(Style::default().bg(PANEL_BG)),
                inner,
            );
        }
    }
}

#[derive(Clone, Copy)]
struct CropRect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

struct ReferenceArt {
    image: DynamicImage,
}

#[derive(Clone, Copy)]
struct PixelSample {
    intensity: f32,
    color: Color,
}

impl ReferenceArt {
    fn load(path: &Path) -> Result<Self> {
        Ok(Self {
            image: image::open(path)?,
        })
    }

    fn render_tinted_blocks(
        &self,
        crop: CropRect,
        width: u16,
        height: u16,
        dim: Color,
        bright: Color,
        scanline_strength: f32,
        contrast: f32,
    ) -> Text<'static> {
        if width == 0 || height == 0 {
            return Text::default();
        }

        let Some(cropped) = self.crop(crop) else {
            return Text::default();
        };

        let grayscale = cropped
            .resize_exact(width as u32 * 2, height as u32 * 2, FilterType::CatmullRom)
            .to_luma8();
        let mut lines = Vec::with_capacity(height as usize);

        for y in 0..height as u32 {
            let mut spans = Vec::with_capacity(width as usize);
            for x in 0..width as u32 {
                let samples = [
                    sample_color(
                        &grayscale,
                        x * 2,
                        y * 2,
                        dim,
                        bright,
                        scanline_strength,
                        contrast,
                    ),
                    sample_color(
                        &grayscale,
                        x * 2 + 1,
                        y * 2,
                        dim,
                        bright,
                        scanline_strength,
                        contrast,
                    ),
                    sample_color(
                        &grayscale,
                        x * 2,
                        y * 2 + 1,
                        dim,
                        bright,
                        scanline_strength,
                        contrast,
                    ),
                    sample_color(
                        &grayscale,
                        x * 2 + 1,
                        y * 2 + 1,
                        dim,
                        bright,
                        scanline_strength,
                        contrast,
                    ),
                ];

                let mut min_intensity = f32::MAX;
                let mut max_intensity = f32::MIN;
                for sample in &samples {
                    min_intensity = min_intensity.min(sample.intensity);
                    max_intensity = max_intensity.max(sample.intensity);
                }

                let average_bg = average_color(samples.iter().map(|sample| sample.color));
                if max_intensity - min_intensity < 20.0 {
                    spans.push(Span::styled(
                        " ".to_string(),
                        Style::default().bg(average_bg),
                    ));
                    continue;
                }

                let threshold = (min_intensity + max_intensity) * 0.5;
                let mut mask = 0u8;
                let mut fg_colors = Vec::with_capacity(4);
                let mut bg_colors = Vec::with_capacity(4);

                for (index, sample) in samples.iter().enumerate() {
                    if sample.intensity >= threshold {
                        mask |= 1 << index;
                        fg_colors.push(sample.color);
                    } else {
                        bg_colors.push(sample.color);
                    }
                }

                if mask == 0 || mask == 0b1111 {
                    spans.push(Span::styled(
                        " ".to_string(),
                        Style::default().bg(average_bg),
                    ));
                    continue;
                }

                spans.push(Span::styled(
                    quadrant_char(mask).to_string(),
                    Style::default()
                        .fg(average_color(fg_colors))
                        .bg(average_color(bg_colors)),
                ));
            }
            lines.push(Line::from(spans));
        }

        Text::from(lines)
    }

    fn crop(&self, crop: CropRect) -> Option<DynamicImage> {
        let width = self.image.width();
        let height = self.image.height();
        let x = (width as f32 * crop.x).round() as u32;
        let y = (height as f32 * crop.y).round() as u32;
        let w = (width as f32 * crop.w).round() as u32;
        let h = (height as f32 * crop.h).round() as u32;

        if x >= width || y >= height {
            return None;
        }

        Some(self.image.crop_imm(
            x,
            y,
            w.min(width - x).max(1),
            h.min(height - y).max(1),
        ))
    }
}

fn render_buffer_png(buffer: &Buffer, output_path: &Path) -> Result<()> {
    let font_bytes = fs::read(DEFAULT_FONT)
        .ok()
        .or_else(|| {
            fs::read("/usr/share/fonts/truetype/liberation/LiberationMono-Bold.ttf")
                .or_else(|_| fs::read("/usr/share/fonts/truetype/liberation2/LiberationMono-Bold.ttf"))
                .ok()
        })
        .ok_or_eyre("unable to locate a monospace font for screenshot capture")?;
    let font = FontArc::try_from_vec(font_bytes).map_err(|_| eyre!("invalid font data"))?;

    let width_px = buffer.area.width as u32 * CELL_WIDTH_PX;
    let height_px = buffer.area.height as u32 * CELL_HEIGHT_PX;
    let mut image = RgbaImage::from_pixel(width_px, height_px, rgba(BG));

    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            let cell = &buffer[(x, y)];
            let x_px = x as i32 * CELL_WIDTH_PX as i32;
            let y_px = y as i32 * CELL_HEIGHT_PX as i32;

            draw_filled_rect_mut(
                &mut image,
                PixelRect::at(x_px, y_px).of_size(CELL_WIDTH_PX, CELL_HEIGHT_PX),
                rgba_or(cell.bg, PANEL_BG),
            );

            let mut fg = rgba_or(cell.fg, VALUE);
            if cell.modifier.contains(Modifier::BOLD) {
                fg = brighten(fg, 1.15);
            }

            if let Some(mask) = quadrant_mask(cell.symbol()) {
                draw_quadrant_mask(&mut image, x_px, y_px, fg, mask);
            } else {
                match cell.symbol() {
                    " " => {}
                    symbol => {
                        draw_text_mut(
                            &mut image,
                            fg,
                            x_px + 1,
                            y_px - 2,
                            TEXT_SIZE_PX,
                            &font,
                            symbol,
                        );
                    }
                }
            }
        }
    }

    image.save(output_path)?;
    Ok(())
}

fn tint(value: u8, dark: Color, light: Color, brightness: f32) -> Color {
    let [dr, dg, db] = rgb_components(dark);
    let [lr, lg, lb] = rgb_components(light);
    let mix = value as f32 / 255.0;
    Color::Rgb(
        shade_channel(dr, lr, mix, brightness),
        shade_channel(dg, lg, mix, brightness),
        shade_channel(db, lb, mix, brightness),
    )
}

fn shade_channel(dark: u8, light: u8, mix: f32, brightness: f32) -> u8 {
    ((dark as f32 + (light as f32 - dark as f32) * mix) * brightness).clamp(0.0, 255.0) as u8
}

fn rgb_components(color: Color) -> [u8; 3] {
    let [r, g, b, _] = rgba(color).0;
    [r, g, b]
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

fn sample_color(
    grayscale: &image::GrayImage,
    x: u32,
    y: u32,
    dim: Color,
    bright: Color,
    scanline_strength: f32,
    contrast: f32,
) -> PixelSample {
    let raw = grayscale
        .get_pixel(
            x.min(grayscale.width().saturating_sub(1)),
            y.min(grayscale.height().saturating_sub(1)),
        )[0];
    let value = contrast_adjust(raw, contrast);
    let brightness = if y % 2 == 0 { 1.0 } else { scanline_strength };
    PixelSample {
        intensity: value as f32 * brightness,
        color: tint(value, dim, bright, brightness),
    }
}

fn contrast_adjust(value: u8, contrast: f32) -> u8 {
    (((value as f32 - 128.0) * contrast) + 128.0).clamp(0.0, 255.0) as u8
}

fn average_color<I>(colors: I) -> Color
where
    I: IntoIterator<Item = Color>,
{
    let mut total = [0u32; 3];
    let mut count = 0u32;

    for color in colors {
        let [r, g, b] = rgb_components(color);
        total[0] += r as u32;
        total[1] += g as u32;
        total[2] += b as u32;
        count += 1;
    }

    if count == 0 {
        return PANEL_BG;
    }

    Color::Rgb(
        (total[0] / count) as u8,
        (total[1] / count) as u8,
        (total[2] / count) as u8,
    )
}

fn quadrant_char(mask: u8) -> &'static str {
    match mask {
        0b0001 => "▘",
        0b0010 => "▝",
        0b0011 => "▀",
        0b0100 => "▖",
        0b0101 => "▌",
        0b0110 => "▞",
        0b0111 => "▛",
        0b1000 => "▗",
        0b1001 => "▚",
        0b1010 => "▐",
        0b1011 => "▜",
        0b1100 => "▄",
        0b1101 => "▙",
        0b1110 => "▟",
        0b1111 => "█",
        _ => " ",
    }
}

fn quadrant_mask(symbol: &str) -> Option<u8> {
    Some(match symbol {
        "▘" => 0b0001,
        "▝" => 0b0010,
        "▀" => 0b0011,
        "▖" => 0b0100,
        "▌" => 0b0101,
        "▞" => 0b0110,
        "▛" => 0b0111,
        "▗" => 0b1000,
        "▚" => 0b1001,
        "▐" => 0b1010,
        "▜" => 0b1011,
        "▄" => 0b1100,
        "▙" => 0b1101,
        "▟" => 0b1110,
        "█" => 0b1111,
        _ => return None,
    })
}

fn draw_quadrant_mask(
    image: &mut RgbaImage,
    x_px: i32,
    y_px: i32,
    fg: Rgba<u8>,
    mask: u8,
) {
    let half_w = (CELL_WIDTH_PX / 2).max(1);
    let half_h = (CELL_HEIGHT_PX / 2).max(1);

    let quadrants = [
        (0b0001, x_px, y_px),
        (0b0010, x_px + half_w as i32, y_px),
        (0b0100, x_px, y_px + half_h as i32),
        (0b1000, x_px + half_w as i32, y_px + half_h as i32),
    ];

    for (bit, qx, qy) in quadrants {
        if mask & bit != 0 {
            draw_filled_rect_mut(
                image,
                PixelRect::at(qx, qy).of_size(half_w, half_h),
                fg,
            );
        }
    }
}

fn rgba(color: Color) -> Rgba<u8> {
    match color {
        Color::Reset => Rgba([0, 0, 0, 255]),
        Color::Black => Rgba([0, 0, 0, 255]),
        Color::Red => Rgba([205, 49, 49, 255]),
        Color::Green => Rgba([13, 188, 121, 255]),
        Color::Yellow => Rgba([229, 229, 16, 255]),
        Color::Blue => Rgba([36, 114, 200, 255]),
        Color::Magenta => Rgba([188, 63, 188, 255]),
        Color::Cyan => Rgba([17, 168, 205, 255]),
        Color::Gray => Rgba([229, 229, 229, 255]),
        Color::DarkGray => Rgba([102, 102, 102, 255]),
        Color::LightRed => Rgba([241, 76, 76, 255]),
        Color::LightGreen => Rgba([35, 209, 139, 255]),
        Color::LightYellow => Rgba([245, 245, 67, 255]),
        Color::LightBlue => Rgba([59, 142, 234, 255]),
        Color::LightMagenta => Rgba([214, 112, 214, 255]),
        Color::LightCyan => Rgba([41, 184, 219, 255]),
        Color::White => Rgba([255, 255, 255, 255]),
        Color::Rgb(r, g, b) => Rgba([r, g, b, 255]),
        Color::Indexed(index) => ansi_256(index),
    }
}

fn rgba_or(color: Color, fallback: Color) -> Rgba<u8> {
    if matches!(color, Color::Reset) {
        rgba(fallback)
    } else {
        rgba(color)
    }
}

fn brighten(color: Rgba<u8>, factor: f32) -> Rgba<u8> {
    let [r, g, b, a] = color.0;
    let scale = |value: u8| ((value as f32 * factor).min(255.0)) as u8;
    Rgba([scale(r), scale(g), scale(b), a])
}

fn ansi_256(index: u8) -> Rgba<u8> {
    match index {
        0 => Rgba([0, 0, 0, 255]),
        1 => Rgba([128, 0, 0, 255]),
        2 => Rgba([0, 128, 0, 255]),
        3 => Rgba([128, 128, 0, 255]),
        4 => Rgba([0, 0, 128, 255]),
        5 => Rgba([128, 0, 128, 255]),
        6 => Rgba([0, 128, 128, 255]),
        7 => Rgba([192, 192, 192, 255]),
        8 => Rgba([128, 128, 128, 255]),
        9 => Rgba([255, 0, 0, 255]),
        10 => Rgba([0, 255, 0, 255]),
        11 => Rgba([255, 255, 0, 255]),
        12 => Rgba([0, 0, 255, 255]),
        13 => Rgba([255, 0, 255, 255]),
        14 => Rgba([0, 255, 255, 255]),
        15 => Rgba([255, 255, 255, 255]),
        16..=231 => {
            let idx = index - 16;
            let r = idx / 36;
            let g = (idx % 36) / 6;
            let b = idx % 6;
            let map = |n: u8| if n == 0 { 0 } else { 55 + n * 40 };
            Rgba([map(r), map(g), map(b), 255])
        }
        232..=255 => {
            let shade = 8 + (index - 232) * 10;
            Rgba([shade, shade, shade, 255])
        }
    }
}

fn render_profile_details(frame: &mut ratatui::Frame, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(2),
        ])
        .split(area);

    let fields = [
        ("Name", "MARIE RAINES CARRADINE"),
        ("Resident", "JACKSON'S STAR"),
        ("Date of Birth", "18 FEBRUARY 2121"),
        ("Birth Place", "EARTH, 21 YRS - AQUARIUS"),
        ("Height", "150 CM"),
        ("Weight", "45 KG"),
    ];

    let mut row = 0usize;
    for (label, value) in fields {
        render_field_row(frame, rows[row], label, value);
        render_separator(frame, rows[row + 1]);
        row += 2;
    }

    render_separator(frame, rows[row]);
    render_field_row(frame, rows[14], "Education Records", "N/A");
    render_separator(frame, rows[15]);

    let known_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(rows[16]);
    render_field_row(frame, known_rows[0], "Known Relations", "CARRADINE, E (DECEASED)");
    render_field_row(frame, known_rows[1], "", "CARRADINE, S (DECEASED)");
}

fn render_field_row(frame: &mut ratatui::Frame, area: Rect, label: &str, value: &str) {
    let gutter = area.width.clamp(18, 24);
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(gutter), Constraint::Min(0)])
        .split(area);

    let label_text = if label.is_empty() {
        String::new()
    } else {
        format!("{}:", truncate(label, gutter.saturating_sub(1) as usize))
    };
    frame.render_widget(
        Paragraph::new(Line::styled(label_text, label_style())).alignment(Alignment::Left),
        columns[0],
    );
    frame.render_widget(
        Paragraph::new(Line::styled(value.to_string(), value_style())).alignment(Alignment::Right),
        columns[1],
    );
}

fn render_separator(frame: &mut ratatui::Frame, area: Rect) {
    let line = "┄".repeat(area.width as usize);
    frame.render_widget(Paragraph::new(Line::styled(line, muted_style())), area);
}

fn truncate(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }

    let mut out = String::with_capacity(width);
    let mut chars = text.chars();

    for _ in 0..width {
        let Some(ch) = chars.next() else {
            return out;
        };
        out.push(ch);
    }

    if chars.next().is_some() && width > 1 {
        out.pop();
        out.push('…');
    }

    out
}

fn missing_art(width: u16, height: u16, image_path: &Path) -> Text<'static> {
    let mut lines = Vec::new();
    let top_padding = height.saturating_div(2).saturating_sub(2);

    for _ in 0..top_padding {
        lines.push(Line::raw(" "));
    }

    lines.push(Line::styled("[ portrait feed unavailable ]", value_style()));
    lines.push(Line::styled(
        format!("source: {}", image_path.display()),
        muted_style(),
    ));
    lines.push(Line::styled(
        format!("target area: {}x{}", width, height),
        muted_style(),
    ));

    Text::from(lines)
}

fn bar_block() -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(BORDER))
        .style(Style::default().bg(HEADER_BG))
}

fn panel_block() -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(BORDER))
        .style(Style::default().bg(PANEL_BG))
}

fn dossier_block() -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(BORDER))
        .style(Style::default().bg(BG))
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

fn label_style() -> Style {
    Style::default().fg(LABEL).bg(PANEL_BG)
}

fn muted_style() -> Style {
    Style::default().fg(MUTED).bg(PANEL_BG)
}

fn value_style() -> Style {
    Style::default().fg(VALUE).bg(PANEL_BG)
}
