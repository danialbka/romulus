use std::{
    path::{Path, PathBuf},
    time::Instant,
};

use ab_glyph::FontArc;
use anyhow::{Context, Result, bail};
use image::{DynamicImage, GenericImageView, Rgba, RgbaImage, imageops::FilterType};
use imageproc::{
    drawing::{draw_filled_rect_mut, draw_hollow_rect_mut, draw_text_mut},
    rect::Rect,
};
use minifb::{Key, KeyRepeat, MouseButton, MouseMode, Window, WindowOptions};

const BASE_W: u32 = 1197;
const BASE_H: u32 = 907;
const CLOCK_START_SECONDS: u32 = 2 * 3600 + 3 * 60 + 5;
const DEFAULT_IMAGE_NAME: &str = "HEI1Ts9aIAETw1k.jpg";
const EMBEDDED_REFERENCE_JPG: &[u8] =
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/HEI1Ts9aIAETw1k.jpg"));
const EMBEDDED_MONO_FONT: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/fonts/DejaVuSansMono-Bold.ttf"
));
const EMBEDDED_NARROW_FONT: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/fonts/LiberationSansNarrow-Bold.ttf"
));

const BG: Rgba<u8> = Rgba([6, 9, 8, 255]);
const PANEL_BG: Rgba<u8> = Rgba([6, 11, 10, 255]);
const MENU_BG: Rgba<u8> = Rgba([8, 16, 14, 255]);
const SHADOW: Rgba<u8> = Rgba([1, 4, 3, 255]);
const BORDER: Rgba<u8> = Rgba([13, 115, 84, 255]);
const HEADER_BG: Rgba<u8> = Rgba([30, 99, 75, 255]);
const LABEL: Rgba<u8> = Rgba([62, 155, 117, 255]);
const MUTED: Rgba<u8> = Rgba([18, 60, 49, 255]);
const VALUE: Rgba<u8> = Rgba([219, 177, 91, 255]);
const BADGE: Rgba<u8> = Rgba([212, 165, 78, 255]);

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

#[derive(Clone, Copy)]
struct MenuEntry {
    label: &'static str,
    detail: &'static str,
    action: MenuAction,
}

const CORP_MENU: [MenuEntry; 3] = [
    MenuEntry {
        label: "Personnel",
        detail: "Officer AD",
        action: MenuAction::SelectHeader(HeaderTab::Corp),
    },
    MenuEntry {
        label: "Freight",
        detail: "Jackson's Star",
        action: MenuAction::SelectDepartment(Department::Farming),
    },
    MenuEntry {
        label: "Archive",
        detail: "Internal",
        action: MenuAction::SelectBadge(BadgeMode::Audit),
    },
];

const DATABASE_MENU: [MenuEntry; 3] = [
    MenuEntry {
        label: "Profile",
        detail: "Identity",
        action: MenuAction::SelectView(ViewMode::Profile),
    },
    MenuEntry {
        label: "Biometrics",
        detail: "Portrait / Prints",
        action: MenuAction::SelectView(ViewMode::Biometrics),
    },
    MenuEntry {
        label: "Relations",
        detail: "Family / Notes",
        action: MenuAction::SelectView(ViewMode::Relations),
    },
];

const BADGE_MENU: [MenuEntry; 3] = [
    MenuEntry {
        label: "Access A",
        detail: "Default",
        action: MenuAction::SelectBadge(BadgeMode::AccessA),
    },
    MenuEntry {
        label: "Secure",
        detail: "Monitoring",
        action: MenuAction::SelectBadge(BadgeMode::Secure),
    },
    MenuEntry {
        label: "Audit",
        detail: "Trace",
        action: MenuAction::SelectBadge(BadgeMode::Audit),
    },
];

const DEPARTMENT_MENU: [MenuEntry; 4] = [
    MenuEntry {
        label: "Farming",
        detail: "Current",
        action: MenuAction::SelectDepartment(Department::Farming),
    },
    MenuEntry {
        label: "Hydroponics",
        detail: "Annex B",
        action: MenuAction::SelectDepartment(Department::Hydroponics),
    },
    MenuEntry {
        label: "Survey",
        detail: "Field",
        action: MenuAction::SelectDepartment(Department::Survey),
    },
    MenuEntry {
        label: "Terraform",
        detail: "Ops",
        action: MenuAction::SelectDepartment(Department::Terraform),
    },
];

fn main() -> Result<()> {
    let cli = Cli::parse()?;
    let assets = Assets::load(cli.image_path.as_deref())?;
    let layout = Layout::new();
    let state = AppState::default();

    if let Some(path) = cli.screenshot_path {
        let frame = render_scene(&assets, &layout, &state, false)?;
        frame.save(&path)?;
        println!("saved screenshot to {}", path.display());
        return Ok(());
    }

    show_window(&assets, &layout, cli.scale)
}

struct Cli {
    image_path: Option<PathBuf>,
    screenshot_path: Option<PathBuf>,
    scale: f32,
}

impl Cli {
    fn parse() -> Result<Self> {
        let mut image_path = None;
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
                        "Usage:\n  cargo run -- [image_path]\n  cargo run -- --screenshot out.png [image_path]\n  cargo run -- --scale 0.75\n\nWhen no image path is given, the bundled {DEFAULT_IMAGE_NAME} reference is used."
                    );
                    std::process::exit(0);
                }
                other if other.starts_with('-') => bail!("unrecognized argument: {other}"),
                other => image_path = Some(PathBuf::from(other)),
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
    fn load(image_path: Option<&Path>) -> Result<Self> {
        Ok(Self {
            source: load_source_image(image_path)?,
            mono: load_font_bytes(EMBEDDED_MONO_FONT, "bundled mono font")?,
            narrow: load_font_bytes(EMBEDDED_NARROW_FONT, "bundled narrow font")?,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HeaderTab {
    Corp,
    Database,
}

impl HeaderTab {
    fn menu_kind(self) -> MenuKind {
        match self {
            Self::Corp => MenuKind::Corp,
            Self::Database => MenuKind::Database,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MenuKind {
    Corp,
    Database,
    Badge,
    Department,
}

impl MenuKind {
    fn title(self) -> &'static str {
        match self {
            Self::Corp => "CORPORATE ACCESS",
            Self::Database => "DATABASE ROUTING",
            Self::Badge => "CLEARANCE MENU",
            Self::Department => "DEPARTMENT INDEX",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ViewMode {
    Profile,
    Biometrics,
    Relations,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Gender {
    Male,
    Female,
    Other,
}

impl Gender {
    fn label(self) -> &'static str {
        match self {
            Self::Male => "M",
            Self::Female => "F",
            Self::Other => "O",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Department {
    Farming,
    Hydroponics,
    Survey,
    Terraform,
}

impl Department {
    fn label(self) -> &'static str {
        match self {
            Self::Farming => "FARMING",
            Self::Hydroponics => "HYDROPONICS",
            Self::Survey => "SURVEY",
            Self::Terraform => "TERRAFORM",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BadgeMode {
    AccessA,
    Secure,
    Audit,
}

impl BadgeMode {
    fn glyph(self) -> &'static str {
        match self {
            Self::AccessA => "A",
            Self::Secure => "S",
            Self::Audit => "!",
        }
    }

    fn status_text(self) -> &'static str {
        match self {
            Self::AccessA => "SYSTEM ONLINE",
            Self::Secure => "SYSTEM SECURE",
            Self::Audit => "TRACE ENABLED",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MenuAction {
    SelectHeader(HeaderTab),
    SelectView(ViewMode),
    SelectDepartment(Department),
    SelectBadge(BadgeMode),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HitTarget {
    Tab(HeaderTab),
    Badge,
    Department,
    Gender(Gender),
    Fingerprint(usize),
    MenuItem(MenuAction),
    MenuPanel(MenuKind),
}

#[derive(Clone, Copy)]
struct AppState {
    hover: Option<HitTarget>,
    active_tab: HeaderTab,
    open_menu: Option<MenuKind>,
    view: ViewMode,
    gender: Gender,
    department: Department,
    badge_mode: BadgeMode,
    selected_print: usize,
    clock_seconds: u32,
    has_interacted: bool,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            hover: None,
            active_tab: HeaderTab::Database,
            open_menu: None,
            view: ViewMode::Biometrics,
            gender: Gender::Female,
            department: Department::Farming,
            badge_mode: BadgeMode::AccessA,
            selected_print: 1,
            clock_seconds: 0,
            has_interacted: false,
        }
    }
}

struct Layout {
    outer: PxRect,
    header_left: PxRect,
    header_center: PxRect,
    status_left: PxRect,
    badge: PxRect,
    status_right: PxRect,
    left_panel: PxRect,
    portrait: PxRect,
    finger: PxRect,
    dept: PxRect,
    gender_boxes: [(Gender, PxRect); 3],
    finger_cells: [PxRect; 5],
    relations_focus: PxRect,
}

impl Layout {
    fn new() -> Self {
        let outer = PxRect::new(12, 22, 1179, 880);
        let header_left = PxRect::new(31, 37, 404, 35);
        let header_center = PxRect::new(438, 37, 696, 35);
        let status_left = PxRect::new(31, 74, 520, 75);
        let badge = PxRect::new(556, 74, 54, 75);
        let status_right = PxRect::new(618, 74, 540, 75);
        let left_panel = PxRect::new(31, 154, 572, 748);
        let portrait = PxRect::new(605, 154, 553, 604);
        let finger = PxRect::new(605, 758, 553, 144);
        let dept = PxRect::new(left_panel.x + 14, left_panel.y + 115, 188, 38);
        let gender_boxes = [
            (Gender::Male, PxRect::new(left_panel.x + 228, left_panel.y + 115, 37, 38)),
            (Gender::Female, PxRect::new(left_panel.x + 275, left_panel.y + 115, 37, 38)),
            (Gender::Other, PxRect::new(left_panel.x + 322, left_panel.y + 115, 37, 38)),
        ];
        let step = finger.w / 5;
        let finger_cells = std::array::from_fn(|i| {
            PxRect::new(
                finger.x + i as u32 * step + 4,
                finger.y + 24,
                step.saturating_sub(8),
                finger.h.saturating_sub(30),
            )
        });
        let relations_focus = PxRect::new(left_panel.x + 14, left_panel.bottom() - 84, left_panel.w - 28, 64);

        Self {
            outer,
            header_left,
            header_center,
            status_left,
            badge,
            status_right,
            left_panel,
            portrait,
            finger,
            dept,
            gender_boxes,
            finger_cells,
            relations_focus,
        }
    }

    fn menu_rect(&self, kind: MenuKind) -> PxRect {
        match kind {
            MenuKind::Corp => PxRect::new(self.header_left.x, self.header_left.bottom() + 6, 252, 126),
            MenuKind::Database => PxRect::new(self.header_center.x + self.header_center.w - 266, self.header_center.bottom() + 6, 266, 126),
            MenuKind::Badge => PxRect::new(self.badge.x.saturating_sub(72), self.badge.bottom() + 6, 200, 126),
            MenuKind::Department => PxRect::new(self.dept.x, self.dept.bottom() + 8, 236, 156),
        }
    }
}

fn load_source_image(image_path: Option<&Path>) -> Result<DynamicImage> {
    match image_path {
        Some(path) => image::open(path)
            .with_context(|| format!("failed to open {}", path.display())),
        None => image::load_from_memory(EMBEDDED_REFERENCE_JPG)
            .context("failed to decode bundled reference image"),
    }
}

fn load_font_bytes(bytes: &[u8], label: &str) -> Result<FontArc> {
    FontArc::try_from_vec(bytes.to_vec()).map_err(|_| anyhow::anyhow!("invalid font data in {label}"))
}

fn show_window(assets: &Assets, layout: &Layout, scale: f32) -> Result<()> {
    let start_w = (BASE_W as f32 * scale).round().max(1.0) as usize;
    let start_h = (BASE_H as f32 * scale).round().max(1.0) as usize;
    let mut window = Window::new(
        "Romulus Custom Renderer",
        start_w,
        start_h,
        WindowOptions {
            resize: true,
            ..WindowOptions::default()
        },
    )
    .context("failed to open a GUI window; live mode needs a desktop environment. In headless setups, use `cargo run -- --screenshot out.png`.")?;
    window.set_target_fps(60);

    let mut state = AppState::default();
    let mut frame = render_scene(assets, layout, &state, true)?;
    let mut scaled = resize_to_window(&frame, start_w as u32, start_h as u32);
    let mut buffer = rgba_to_u32(&scaled);
    let mut last_size = (start_w, start_h);
    let mut mouse_was_down = false;
    let clock_started = Instant::now();

    while window.is_open() && !window.is_key_down(Key::Q) {
        let mut dirty = false;
        let next_clock_seconds = clock_started.elapsed().as_secs().min(u32::MAX as u64) as u32;
        if next_clock_seconds != state.clock_seconds {
            state.clock_seconds = next_clock_seconds;
            dirty = true;
        }

        if window.is_key_pressed(Key::Escape, KeyRepeat::No) {
            if state.open_menu.take().is_some() {
                dirty = true;
            } else {
                break;
            }
        }

        if window.is_key_pressed(Key::Tab, KeyRepeat::No) {
            state.active_tab = match state.active_tab {
                HeaderTab::Corp => HeaderTab::Database,
                HeaderTab::Database => HeaderTab::Corp,
            };
            state.open_menu = Some(state.active_tab.menu_kind());
            state.has_interacted = true;
            dirty = true;
        }

        let (ww, hh) = window.get_size();
        let window_w = ww.max(1);
        let window_h = hh.max(1);
        let hover = window
            .get_mouse_pos(MouseMode::Clamp)
            .map(|(mx, my)| {
                let x = ((mx.max(0.0) / window_w as f32) * BASE_W as f32)
                    .clamp(0.0, (BASE_W - 1) as f32) as u32;
                let y = ((my.max(0.0) / window_h as f32) * BASE_H as f32)
                    .clamp(0.0, (BASE_H - 1) as f32) as u32;
                hit_test(layout, &state, x, y)
            })
            .flatten();

        if hover != state.hover {
            state.hover = hover;
            dirty = true;
        }

        let mouse_down = window.get_mouse_down(MouseButton::Left);
        if !mouse_down && mouse_was_down {
            if let Some(hit) = state.hover {
                if handle_click(&mut state, hit) {
                    dirty = true;
                }
            } else if state.open_menu.take().is_some() {
                dirty = true;
            }
        }
        mouse_was_down = mouse_down;

        if dirty {
            frame = render_scene(assets, layout, &state, true)?;
        }

        if dirty || last_size != (window_w, window_h) {
            scaled = resize_to_window(&frame, window_w as u32, window_h as u32);
            buffer = rgba_to_u32(&scaled);
            last_size = (window_w, window_h);
        }

        window.update_with_buffer(&buffer, window_w, window_h)?;
    }

    Ok(())
}

fn render_scene(assets: &Assets, layout: &Layout, state: &AppState, interactive: bool) -> Result<RgbaImage> {
    let mut img = RgbaImage::from_pixel(BASE_W, BASE_H, BG);

    let corp_hot = interactive
        && (state.hover == Some(HitTarget::Tab(HeaderTab::Corp)) || state.open_menu == Some(MenuKind::Corp));
    let db_hot = interactive
        && (state.hover == Some(HitTarget::Tab(HeaderTab::Database))
            || state.open_menu == Some(MenuKind::Database));

    stroke_rect(&mut img, layout.outer, BORDER, 3);
    fill_stroke_rect(
        &mut img,
        layout.header_left,
        if corp_hot { blend(HEADER_BG, BADGE, 0.12) } else { HEADER_BG },
        BORDER,
        3,
    );
    fill_stroke_rect(
        &mut img,
        layout.header_center,
        if db_hot { blend(HEADER_BG, BADGE, 0.12) } else { HEADER_BG },
        BORDER,
        3,
    );
    fill_stroke_rect(&mut img, layout.status_left, PANEL_BG, BORDER, 3);
    fill_stroke_rect(&mut img, layout.badge, PANEL_BG, BORDER, 3);
    fill_stroke_rect(&mut img, layout.status_right, PANEL_BG, BORDER, 3);
    fill_stroke_rect(&mut img, layout.left_panel, PANEL_BG, BORDER, 3);
    fill_stroke_rect(&mut img, layout.portrait, PANEL_BG, BORDER, 3);
    fill_stroke_rect(&mut img, layout.finger, PANEL_BG, BORDER, 3);

    draw_header(&mut img, assets, layout, state, interactive)?;
    draw_status(&mut img, assets, layout, state, interactive);
    draw_left_panel(&mut img, assets, layout, state, interactive);
    draw_portrait_panel(&mut img, assets, layout, state, interactive)?;
    draw_fingerprint_panel(&mut img, assets, layout, state, interactive)?;

    if interactive && state.has_interacted {
        match state.view {
            ViewMode::Profile => stroke_rect(&mut img, layout.left_panel.inner(6, 6), blend(BORDER, BADGE, 0.3), 2),
            ViewMode::Biometrics => {
                stroke_rect(&mut img, layout.portrait.inner(4, 4), blend(BORDER, BADGE, 0.34), 2);
                stroke_rect(&mut img, layout.finger.inner(4, 4), blend(BORDER, BADGE, 0.34), 2);
            }
            ViewMode::Relations => stroke_rect(&mut img, layout.relations_focus, blend(BORDER, BADGE, 0.34), 2),
        }
    }

    if interactive {
        if let Some(kind) = state.open_menu {
            draw_menu_overlay(&mut img, assets, layout, state, kind);
        }
    }

    Ok(img)
}

fn draw_header(
    img: &mut RgbaImage,
    assets: &Assets,
    layout: &Layout,
    state: &AppState,
    interactive: bool,
) -> Result<()> {
    let logo_area = PxRect::new(layout.header_left.x + 12, layout.header_left.y + 7, 50, 18);
    let logo = crop_norm(&assets.source, LOGO_CROP)?;
    overlay_fit(img, &logo, logo_area);
    draw_text(
        img,
        &assets.narrow,
        21.0,
        VALUE,
        layout.header_left.x + 70,
        layout.header_left.y + 6,
        "WEYLAND-YUTANI CORP",
    );
    draw_centered_text(
        img,
        &assets.narrow,
        21.0,
        VALUE,
        layout.header_center,
        layout.header_center.y + 6,
        "COLONY AFFAIRS DATABASE",
        0.58,
    );

    if interactive {
        if state.hover == Some(HitTarget::Tab(HeaderTab::Corp)) || state.open_menu == Some(MenuKind::Corp) {
            let line = PxRect::new(layout.header_left.x + 8, layout.header_left.bottom() - 5, layout.header_left.w - 16, 2);
            draw_filled_rect_mut(img, Rect::at(line.x as i32, line.y as i32).of_size(line.w, line.h), BADGE);
        }
        if state.hover == Some(HitTarget::Tab(HeaderTab::Database)) || state.open_menu == Some(MenuKind::Database) {
            let line = PxRect::new(layout.header_center.x + 8, layout.header_center.bottom() - 5, layout.header_center.w - 16, 2);
            draw_filled_rect_mut(img, Rect::at(line.x as i32, line.y as i32).of_size(line.w, line.h), BADGE);
        }
    }

    Ok(())
}

fn draw_status(
    img: &mut RgbaImage,
    assets: &Assets,
    layout: &Layout,
    state: &AppState,
    interactive: bool,
) {
    draw_text(img, &assets.narrow, 18.0, LABEL, layout.status_left.x + 18, layout.status_left.y + 14, "USER:");
    draw_text(
        img,
        &assets.narrow,
        20.0,
        VALUE,
        layout.status_left.x + 139,
        layout.status_left.y + 11,
        "OFFICER AD",
    );
    draw_text(
        img,
        &assets.narrow,
        18.0,
        LABEL,
        layout.status_left.x + 18,
        layout.status_left.y + 38,
        "SETTLEMENT:",
    );
    draw_text(
        img,
        &assets.narrow,
        20.0,
        VALUE,
        layout.status_left.x + 139,
        layout.status_left.y + 35,
        "JACKSON'S STAR COLONY",
    );

    let inner = PxRect::new(layout.badge.x + 11, layout.badge.y + 9, layout.badge.w - 22, layout.badge.h - 18);
    let badge_fill = if interactive && (state.hover == Some(HitTarget::Badge) || state.open_menu == Some(MenuKind::Badge)) {
        blend(BADGE, VALUE, 0.12)
    } else {
        BADGE
    };
    draw_filled_rect_mut(img, Rect::at(inner.x as i32, inner.y as i32).of_size(inner.w, inner.h), badge_fill);
    draw_centered_text(img, &assets.narrow, 24.0, PANEL_BG, inner, inner.y + 6, state.badge_mode.glyph(), 0.62);

    let clock = format_clock_text(state.clock_seconds);
    draw_text(img, &assets.narrow, 18.0, LABEL, layout.status_right.x + 22, layout.status_right.y + 14, &clock);
    draw_text(
        img,
        &assets.narrow,
        22.0,
        VALUE,
        layout.status_right.x + 188,
        layout.status_right.y + 11,
        if interactive && state.has_interacted {
            state.badge_mode.status_text()
        } else {
            "SYSTEM ONLINE"
        },
    );
    draw_text(img, &assets.narrow, 18.0, LABEL, layout.status_right.x + 22, layout.status_right.y + 38, "LOG_ID");
    if interactive && state.has_interacted {
        let detail = match state.view {
            ViewMode::Profile => "PROFILE",
            ViewMode::Biometrics => "BIO-METRIC",
            ViewMode::Relations => "RELATIONS",
        };
        draw_text(img, &assets.narrow, 18.0, LABEL, layout.status_right.x + 102, layout.status_right.y + 38, detail);
    }
}

fn draw_left_panel(
    img: &mut RgbaImage,
    assets: &Assets,
    layout: &Layout,
    state: &AppState,
    interactive: bool,
) {
    let area = layout.left_panel;

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

    fill_stroke_rect(
        img,
        layout.dept,
        if interactive && (state.hover == Some(HitTarget::Department) || state.open_menu == Some(MenuKind::Department)) {
            blend(PANEL_BG, HEADER_BG, 0.25)
        } else {
            PANEL_BG
        },
        BORDER,
        3,
    );
    let dept_label = PxRect::new(layout.dept.x, layout.dept.y, 74, layout.dept.h);
    draw_filled_rect_mut(
        img,
        Rect::at(dept_label.x as i32, dept_label.y as i32).of_size(dept_label.w, dept_label.h),
        LABEL,
    );
    draw_text(img, &assets.narrow, 18.0, PANEL_BG, dept_label.x + 12, dept_label.y + 8, "DEPT.");
    draw_text(
        img,
        &assets.narrow,
        20.0,
        VALUE,
        layout.dept.x + 92,
        layout.dept.y + 7,
        state.department.label(),
    );

    for (gender, box_rect) in layout.gender_boxes {
        let is_selected = state.gender == gender;
        let is_hovered = interactive && state.hover == Some(HitTarget::Gender(gender));
        fill_stroke_rect(
            img,
            box_rect,
            if is_selected {
                BADGE
            } else if is_hovered {
                blend(PANEL_BG, HEADER_BG, 0.25)
            } else {
                PANEL_BG
            },
            if is_hovered { blend(BORDER, VALUE, 0.35) } else { BORDER },
            3,
        );
        draw_centered_text(
            img,
            &assets.narrow,
            22.0,
            if is_selected { PANEL_BG } else { VALUE },
            box_rect,
            box_rect.y + 7,
            gender.label(),
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
        draw_dashed_hline(img, area.x + 16, area.right() - 18, line_y, MUTED, 1, 8, 6);
        if idx == 1 {
            draw_dashed_hline(img, area.x + 16, area.right() - 18, area.y + 374, MUTED, 1, 8, 6);
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
    draw_dashed_hline(img, area.x + 16, area.right() - 18, area.bottom() - 92, MUTED, 1, 8, 6);

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

fn draw_portrait_panel(
    img: &mut RgbaImage,
    assets: &Assets,
    layout: &Layout,
    state: &AppState,
    interactive: bool,
) -> Result<()> {
    let portrait_crop = crop_norm(&assets.source, PORTRAIT_CROP)?;
    let target = layout.portrait.inner(4, 4);
    overlay_fit(img, &portrait_crop, target);
    stroke_rect(img, layout.portrait, BORDER, 3);

    if interactive && state.hover == Some(HitTarget::Tab(HeaderTab::Database)) {
        stroke_rect(img, layout.portrait.inner(8, 8), blend(BORDER, VALUE, 0.2), 1);
    }

    Ok(())
}

fn draw_fingerprint_panel(
    img: &mut RgbaImage,
    assets: &Assets,
    layout: &Layout,
    state: &AppState,
    interactive: bool,
) -> Result<()> {
    let labels_y = layout.finger.y + 8;
    let step = layout.finger.w / 5;

    for i in 0..5u32 {
        let x = layout.finger.x + i * step;
        if i > 0 {
            draw_filled_rect_mut(img, Rect::at(x as i32, layout.finger.y as i32).of_size(2, layout.finger.h), BORDER);
        }

        let label_area = PxRect::new(x, labels_y, step, 18);
        let is_selected = state.selected_print == i as usize;
        let is_hovered = interactive && state.hover == Some(HitTarget::Fingerprint(i as usize));
        if interactive && (is_selected || is_hovered) {
            let chip = PxRect::new(x + step / 2 - 18, labels_y - 2, 36, 18);
            draw_filled_rect_mut(
                img,
                Rect::at(chip.x as i32, chip.y as i32).of_size(chip.w, chip.h),
                if is_selected { BADGE } else { blend(PANEL_BG, HEADER_BG, 0.32) },
            );
            draw_centered_text(
                img,
                &assets.narrow,
                16.0,
                if is_selected { PANEL_BG } else { VALUE },
                chip,
                chip.y + 1,
                &format!("{:02}", i + 1),
                0.55,
            );
        } else {
            draw_centered_text(
                img,
                &assets.narrow,
                16.0,
                VALUE,
                label_area,
                labels_y,
                &format!("{:02}", i + 1),
                0.55,
            );
        }

        let crop = fingerprint_crop(i as usize);
        let sprite = crop_norm(&assets.source, crop)?;
        let cell = layout.finger_cells[i as usize];
        overlay_fit(img, &sprite, cell);
        if interactive && (is_selected || is_hovered) {
            stroke_rect(img, cell.inner(1, 1), if is_selected { BADGE } else { blend(BORDER, VALUE, 0.25) }, 2);
        }
    }

    stroke_rect(img, layout.finger, BORDER, 3);
    Ok(())
}

fn draw_menu_overlay(
    img: &mut RgbaImage,
    assets: &Assets,
    layout: &Layout,
    state: &AppState,
    kind: MenuKind,
) {
    let panel = layout.menu_rect(kind);
    let shadow = PxRect::new(panel.x + 4, panel.y + 4, panel.w, panel.h);
    draw_filled_rect_mut(img, Rect::at(shadow.x as i32, shadow.y as i32).of_size(shadow.w, shadow.h), SHADOW);
    fill_stroke_rect(img, panel, MENU_BG, BORDER, 3);
    draw_text(img, &assets.narrow, 17.0, VALUE, panel.x + 12, panel.y + 8, kind.title());
    draw_dashed_hline(img, panel.x + 10, panel.right() - 10, panel.y + 28, MUTED, 1, 8, 5);

    for (row, entry) in menu_item_rects(layout, kind) {
        let hovered = state.hover == Some(HitTarget::MenuItem(entry.action));
        let selected = action_selected(entry.action, state);
        if hovered || selected {
            let fill = if hovered {
                blend(PANEL_BG, BADGE, 0.24)
            } else {
                blend(PANEL_BG, HEADER_BG, 0.36)
            };
            draw_filled_rect_mut(img, Rect::at(row.x as i32, row.y as i32).of_size(row.w, row.h), fill);
            stroke_rect(img, row, if hovered { BADGE } else { BORDER }, 1);
        }
        draw_text(img, &assets.narrow, 19.0, VALUE, row.x + 10, row.y + 3, entry.label);
        draw_right_text(img, &assets.narrow, 15.0, LABEL, row.right() - 8, row.y + 6, entry.detail, 0.50);
    }
}

fn menu_item_rects(layout: &Layout, kind: MenuKind) -> Vec<(PxRect, MenuEntry)> {
    let panel = layout.menu_rect(kind);
    menu_entries(kind)
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            (
                PxRect::new(panel.x + 10, panel.y + 36 + i as u32 * 28, panel.w - 20, 24),
                *entry,
            )
        })
        .collect()
}

fn menu_entries(kind: MenuKind) -> &'static [MenuEntry] {
    match kind {
        MenuKind::Corp => &CORP_MENU,
        MenuKind::Database => &DATABASE_MENU,
        MenuKind::Badge => &BADGE_MENU,
        MenuKind::Department => &DEPARTMENT_MENU,
    }
}

fn action_selected(action: MenuAction, state: &AppState) -> bool {
    match action {
        MenuAction::SelectHeader(tab) => state.active_tab == tab,
        MenuAction::SelectView(view) => state.view == view,
        MenuAction::SelectDepartment(dept) => state.department == dept,
        MenuAction::SelectBadge(mode) => state.badge_mode == mode,
    }
}

fn hit_test(layout: &Layout, state: &AppState, x: u32, y: u32) -> Option<HitTarget> {
    if let Some(kind) = state.open_menu {
        let panel = layout.menu_rect(kind);
        for (row, entry) in menu_item_rects(layout, kind) {
            if row.contains(x, y) {
                return Some(HitTarget::MenuItem(entry.action));
            }
        }
        if panel.contains(x, y) {
            return Some(HitTarget::MenuPanel(kind));
        }
    }

    if layout.header_left.contains(x, y) {
        return Some(HitTarget::Tab(HeaderTab::Corp));
    }
    if layout.header_center.contains(x, y) {
        return Some(HitTarget::Tab(HeaderTab::Database));
    }
    if layout.badge.contains(x, y) {
        return Some(HitTarget::Badge);
    }
    if layout.dept.contains(x, y) {
        return Some(HitTarget::Department);
    }
    for (gender, rect) in layout.gender_boxes {
        if rect.contains(x, y) {
            return Some(HitTarget::Gender(gender));
        }
    }
    for (idx, rect) in layout.finger_cells.iter().enumerate() {
        if rect.contains(x, y) {
            return Some(HitTarget::Fingerprint(idx));
        }
    }

    None
}

fn handle_click(state: &mut AppState, hit: HitTarget) -> bool {
    match hit {
        HitTarget::Tab(tab) => {
            state.active_tab = tab;
            state.open_menu = match state.open_menu {
                Some(kind) if kind == tab.menu_kind() => None,
                _ => Some(tab.menu_kind()),
            };
            state.has_interacted = true;
            true
        }
        HitTarget::Badge => {
            state.open_menu = match state.open_menu {
                Some(MenuKind::Badge) => None,
                _ => Some(MenuKind::Badge),
            };
            state.has_interacted = true;
            true
        }
        HitTarget::Department => {
            state.open_menu = match state.open_menu {
                Some(MenuKind::Department) => None,
                _ => Some(MenuKind::Department),
            };
            state.has_interacted = true;
            true
        }
        HitTarget::Gender(gender) => {
            state.gender = gender;
            state.has_interacted = true;
            true
        }
        HitTarget::Fingerprint(idx) => {
            state.active_tab = HeaderTab::Database;
            state.view = ViewMode::Biometrics;
            state.selected_print = idx;
            state.has_interacted = true;
            true
        }
        HitTarget::MenuItem(action) => {
            apply_action(state, action);
            state.open_menu = None;
            state.has_interacted = true;
            true
        }
        HitTarget::MenuPanel(_) => false,
    }
}

fn apply_action(state: &mut AppState, action: MenuAction) {
    match action {
        MenuAction::SelectHeader(tab) => state.active_tab = tab,
        MenuAction::SelectView(view) => {
            state.active_tab = HeaderTab::Database;
            state.view = view;
        }
        MenuAction::SelectDepartment(dept) => state.department = dept,
        MenuAction::SelectBadge(mode) => state.badge_mode = mode,
    }
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

fn resize_to_window(img: &RgbaImage, w: u32, h: u32) -> RgbaImage {
    image::imageops::resize(img, w.max(1), h.max(1), FilterType::Nearest)
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

    fn contains(self, x: u32, y: u32) -> bool {
        x >= self.x && x < self.right() && y >= self.y && y < self.bottom()
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

fn blend(a: Rgba<u8>, b: Rgba<u8>, t: f32) -> Rgba<u8> {
    let mix = |x: u8, y: u8| -> u8 { ((x as f32) + ((y as f32 - x as f32) * t)).round().clamp(0.0, 255.0) as u8 };
    Rgba([mix(a[0], b[0]), mix(a[1], b[1]), mix(a[2], b[2]), 255])
}

fn format_clock_text(elapsed_seconds: u32) -> String {
    let total = (CLOCK_START_SECONDS + elapsed_seconds) % (24 * 3600);
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let seconds = total % 60;
    format!("{hours:02}:{minutes:02}:{seconds:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clicking_header_toggles_its_menu() {
        let mut state = AppState::default();

        assert!(handle_click(&mut state, HitTarget::Tab(HeaderTab::Corp)));
        assert_eq!(state.active_tab, HeaderTab::Corp);
        assert_eq!(state.open_menu, Some(MenuKind::Corp));

        assert!(handle_click(&mut state, HitTarget::Tab(HeaderTab::Corp)));
        assert_eq!(state.open_menu, None);
    }

    #[test]
    fn clicking_database_menu_item_updates_view() {
        let mut state = AppState {
            open_menu: Some(MenuKind::Database),
            ..AppState::default()
        };

        assert!(handle_click(
            &mut state,
            HitTarget::MenuItem(MenuAction::SelectView(ViewMode::Relations)),
        ));

        assert_eq!(state.view, ViewMode::Relations);
        assert_eq!(state.active_tab, HeaderTab::Database);
        assert_eq!(state.open_menu, None);
    }

    #[test]
    fn hit_test_finds_menu_item_before_underlying_ui() {
        let layout = Layout::new();
        let state = AppState {
            open_menu: Some(MenuKind::Department),
            ..AppState::default()
        };
        let (row, entry) = menu_item_rects(&layout, MenuKind::Department)[1];
        let hit = hit_test(&layout, &state, row.x + 4, row.y + 4);

        assert_eq!(hit, Some(HitTarget::MenuItem(entry.action)));
    }

    #[test]
    fn clock_text_starts_from_reference_time_and_wraps() {
        assert_eq!(format_clock_text(0), "02:03:05");
        assert_eq!(format_clock_text(55), "02:04:00");
        assert_eq!(format_clock_text(24 * 3600), "02:03:05");
    }
}
