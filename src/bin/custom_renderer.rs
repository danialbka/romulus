use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Instant,
};

use ab_glyph::{Font, FontArc, GlyphId, ScaleFont};
use anyhow::{Context, Result, bail};
use image::{DynamicImage, GenericImageView, Rgba, RgbaImage, imageops::FilterType};
use imageproc::{
    drawing::{draw_filled_rect_mut, draw_hollow_rect_mut, draw_text_mut},
    rect::Rect,
};
use minifb::{InputCallback, Key, KeyRepeat, MouseButton, MouseMode, Window, WindowOptions};

const BASE_W: u32 = 1197;
const BASE_H: u32 = 907;
const CLOCK_START_SECONDS: u32 = 2 * 3600 + 3 * 60 + 5;
const PORTRAIT_FADE_SECONDS: f32 = 2.4;
const DEFAULT_IMAGE_NAME: &str = "HEI1Ts9aIAETw1k.jpg";
const EMBEDDED_REFERENCE_JPG: &[u8] =
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/HEI1Ts9aIAETw1k.jpg"));
const EMBEDDED_MONO_FONT: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/fonts/DejaVuSansMono-Bold.ttf"
));
const EMBEDDED_NARROW_BOLD_FONT: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/fonts/LiberationSansNarrow-Bold.ttf"
));
const EMBEDDED_NARROW_REGULAR_FONT: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/fonts/LiberationSansNarrow-Regular.ttf"
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
    let static_frame = render_static_scene(&assets, &layout)?;

    if let Some(path) = cli.screenshot_path {
        let mut frame = static_frame.clone();
        render_scene(&mut frame, &static_frame, &assets, &layout, &state, false);
        frame.save(&path)?;
        println!("saved screenshot to {}", path.display());
        return Ok(());
    }

    show_window(&assets, &layout, &static_frame, cli.scale)
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
    portrait: RgbaImage,
    portrait_crt: RgbaImage,
    mono: FontArc,
    narrow_bold: FontArc,
    narrow_regular: FontArc,
}

impl Assets {
    fn load(image_path: Option<&Path>) -> Result<Self> {
        let source = load_source_image(image_path)?;
        let portrait = crop_norm(&source, PORTRAIT_CROP)?.to_rgba8();
        let portrait_crt = build_crt_portrait(&portrait);
        Ok(Self {
            source,
            portrait,
            portrait_crt,
            mono: load_font_bytes(EMBEDDED_MONO_FONT, "bundled mono font")?,
            narrow_bold: load_font_bytes(EMBEDDED_NARROW_BOLD_FONT, "bundled narrow bold font")?,
            narrow_regular: load_font_bytes(EMBEDDED_NARROW_REGULAR_FONT, "bundled narrow regular font")?,
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
    Field(EditableField),
    MenuItem(MenuAction),
    MenuPanel(MenuKind),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditableField {
    User,
    Settlement,
    CitizenId,
    Name,
    Resident,
    DateOfBirth,
    BirthPlace,
    Height,
    Weight,
    EducationRecords,
    RelationOne,
    RelationTwo,
}

impl EditableField {
    const ORDER: [Self; 12] = [
        Self::User,
        Self::Settlement,
        Self::CitizenId,
        Self::Name,
        Self::Resident,
        Self::DateOfBirth,
        Self::BirthPlace,
        Self::Height,
        Self::Weight,
        Self::EducationRecords,
        Self::RelationOne,
        Self::RelationTwo,
    ];

    fn next(self) -> Self {
        let idx = Self::ORDER.iter().position(|field| *field == self).unwrap_or(0);
        Self::ORDER[(idx + 1) % Self::ORDER.len()]
    }
}

#[derive(Clone, Debug)]
struct TextSelection {
    field: EditableField,
    anchor: usize,
    caret: usize,
}

impl TextSelection {
    fn sorted(&self) -> (usize, usize) {
        if self.anchor <= self.caret {
            (self.anchor, self.caret)
        } else {
            (self.caret, self.anchor)
        }
    }

    fn collapsed(field: EditableField, index: usize) -> Self {
        Self {
            field,
            anchor: index,
            caret: index,
        }
    }
}

#[derive(Clone, Debug)]
struct DossierData {
    user: String,
    settlement: String,
    citizen_id: String,
    name: String,
    resident: String,
    date_of_birth: String,
    birth_place: String,
    height: String,
    weight: String,
    education_records: String,
    relation_one: String,
    relation_two: String,
}

impl Default for DossierData {
    fn default() -> Self {
        Self {
            user: "OFFICER AD".to_owned(),
            settlement: "JACKSON'S STAR COLONY".to_owned(),
            citizen_id: "FWC25583".to_owned(),
            name: "MARIE RAINES CARRADINE".to_owned(),
            resident: "JACKSON'S STAR".to_owned(),
            date_of_birth: "18 FEBRUARY 2121".to_owned(),
            birth_place: "EARTH, 21 YRS - AQUARIUS".to_owned(),
            height: "150 CM".to_owned(),
            weight: "45 KG".to_owned(),
            education_records: "N/A".to_owned(),
            relation_one: "CARRADINE, E (DECEASED)".to_owned(),
            relation_two: "CARRADINE, S (DECEASED)".to_owned(),
        }
    }
}

#[derive(Clone)]
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
    portrait_progress: f32,
    has_interacted: bool,
    dossier: DossierData,
    selection: Option<TextSelection>,
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
            portrait_progress: 0.0,
            has_interacted: false,
            dossier: DossierData::default(),
            selection: None,
        }
    }
}

struct TextInputBuffer {
    chars: Arc<Mutex<Vec<char>>>,
}

impl InputCallback for TextInputBuffer {
    fn add_char(&mut self, uni_char: u32) {
        if let Some(ch) = char::from_u32(uni_char) {
            if !ch.is_control() && let Ok(mut chars) = self.chars.lock() {
                chars.push(ch);
            }
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

#[derive(Clone, Copy)]
enum TextAlign {
    Left,
    Right,
}

#[derive(Clone, Copy)]
enum FieldFont {
    Mono,
    NarrowRegular,
}

#[derive(Clone, Copy)]
struct FieldStyle {
    rect: PxRect,
    font: FieldFont,
    size: f32,
    x_scale: f32,
    color: Rgba<u8>,
    align: TextAlign,
    baseline_y: u32,
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
                finger.x + i as u32 * step + 11,
                finger.y + 20,
                step.saturating_sub(22),
                finger.h.saturating_sub(28),
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

fn field_style(layout: &Layout, field: EditableField) -> FieldStyle {
    match field {
        EditableField::User => FieldStyle {
            rect: PxRect::new(layout.status_left.x + 134, layout.status_left.y + 8, 290, 25),
            font: FieldFont::NarrowRegular,
            size: 18.5,
            x_scale: 0.93,
            color: VALUE,
            align: TextAlign::Left,
            baseline_y: layout.status_left.y + 13,
        },
        EditableField::Settlement => FieldStyle {
            rect: PxRect::new(layout.status_left.x + 134, layout.status_left.y + 32, 340, 25),
            font: FieldFont::NarrowRegular,
            size: 18.5,
            x_scale: 0.93,
            color: VALUE,
            align: TextAlign::Left,
            baseline_y: layout.status_left.y + 37,
        },
        EditableField::CitizenId => FieldStyle {
            rect: PxRect::new(layout.left_panel.x + 206, layout.left_panel.y + 12, layout.left_panel.w - 224, 36),
            font: FieldFont::Mono,
            size: 26.0,
            x_scale: 0.95,
            color: VALUE,
            align: TextAlign::Right,
            baseline_y: layout.left_panel.y + 21,
        },
        EditableField::Name => value_row_style(layout.left_panel, layout.left_panel.y + 248, 20.0),
        EditableField::Resident => value_row_style(layout.left_panel, layout.left_panel.y + 301, 20.0),
        EditableField::DateOfBirth => value_row_style(layout.left_panel, layout.left_panel.y + 394, 20.0),
        EditableField::BirthPlace => value_row_style(layout.left_panel, layout.left_panel.y + 447, 20.0),
        EditableField::Height => value_row_style(layout.left_panel, layout.left_panel.y + 500, 20.0),
        EditableField::Weight => value_row_style(layout.left_panel, layout.left_panel.y + 553, 20.0),
        EditableField::EducationRecords => value_row_style(layout.left_panel, layout.left_panel.bottom() - 118, 20.0),
        EditableField::RelationOne => value_row_style(layout.left_panel, layout.left_panel.bottom() - 62, 20.0),
        EditableField::RelationTwo => value_row_style(layout.left_panel, layout.left_panel.bottom() - 31, 20.0),
    }
}

fn value_row_style(area: PxRect, y: u32, size: f32) -> FieldStyle {
    FieldStyle {
        rect: PxRect::new(area.x + 248, y.saturating_sub(4), area.w - 266, 26),
        font: FieldFont::NarrowRegular,
        size,
        x_scale: 1.0,
        color: VALUE,
        align: TextAlign::Right,
        baseline_y: y.saturating_sub(2),
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

fn dossier_field(data: &DossierData, field: EditableField) -> &str {
    match field {
        EditableField::User => &data.user,
        EditableField::Settlement => &data.settlement,
        EditableField::CitizenId => &data.citizen_id,
        EditableField::Name => &data.name,
        EditableField::Resident => &data.resident,
        EditableField::DateOfBirth => &data.date_of_birth,
        EditableField::BirthPlace => &data.birth_place,
        EditableField::Height => &data.height,
        EditableField::Weight => &data.weight,
        EditableField::EducationRecords => &data.education_records,
        EditableField::RelationOne => &data.relation_one,
        EditableField::RelationTwo => &data.relation_two,
    }
}

fn dossier_field_mut(data: &mut DossierData, field: EditableField) -> &mut String {
    match field {
        EditableField::User => &mut data.user,
        EditableField::Settlement => &mut data.settlement,
        EditableField::CitizenId => &mut data.citizen_id,
        EditableField::Name => &mut data.name,
        EditableField::Resident => &mut data.resident,
        EditableField::DateOfBirth => &mut data.date_of_birth,
        EditableField::BirthPlace => &mut data.birth_place,
        EditableField::Height => &mut data.height,
        EditableField::Weight => &mut data.weight,
        EditableField::EducationRecords => &mut data.education_records,
        EditableField::RelationOne => &mut data.relation_one,
        EditableField::RelationTwo => &mut data.relation_two,
    }
}

fn show_window(assets: &Assets, layout: &Layout, static_frame: &RgbaImage, scale: f32) -> Result<()> {
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
    let text_input = Arc::new(Mutex::new(Vec::new()));
    window.set_input_callback(Box::new(TextInputBuffer {
        chars: text_input.clone(),
    }));

    let mut state = AppState::default();
    let mut frame = static_frame.clone();
    render_scene(&mut frame, static_frame, assets, layout, &state, true);
    let mut last_size = (start_w, start_h);
    let mut mouse_was_down = false;
    let clock_started = Instant::now();
    let fade_started = Instant::now();
    let mut needs_present = true;
    let mut drag_selection: Option<(EditableField, usize)> = None;

    while window.is_open() && !window.is_key_down(Key::Q) {
        let mut dirty = false;
        let next_clock_seconds = clock_started.elapsed().as_secs().min(u32::MAX as u64) as u32;
        if next_clock_seconds != state.clock_seconds {
            state.clock_seconds = next_clock_seconds;
            dirty = true;
        }

        let next_portrait_progress = (fade_started.elapsed().as_secs_f32() / PORTRAIT_FADE_SECONDS).clamp(0.0, 1.0);
        if (next_portrait_progress - state.portrait_progress).abs() > 0.002 {
            state.portrait_progress = next_portrait_progress;
            dirty = true;
        }

        if apply_text_input(&mut state, drain_text_input(&text_input)) {
            dirty = true;
        }
        if handle_text_edit_keys(&window, &mut state) {
            dirty = true;
        }

        if window.is_key_pressed(Key::Escape, KeyRepeat::No) {
            if state.selection.take().is_some() {
                drag_selection = None;
                dirty = true;
            } else if state.open_menu.take().is_some() {
                dirty = true;
            } else {
                break;
            }
        }

        if window.is_key_pressed(Key::Tab, KeyRepeat::No) {
            if let Some(selection) = state.selection.as_mut() {
                let next_field = selection.field.next();
                let next_len = dossier_field(&state.dossier, next_field).chars().count();
                *selection = TextSelection::collapsed(next_field, next_len);
            } else {
                state.active_tab = match state.active_tab {
                    HeaderTab::Corp => HeaderTab::Database,
                    HeaderTab::Database => HeaderTab::Corp,
                };
                state.open_menu = Some(state.active_tab.menu_kind());
                state.has_interacted = true;
            }
            dirty = true;
        }

        let (ww, hh) = window.get_size();
        let window_w = ww.max(1);
        let window_h = hh.max(1);
        let viewport = fit_viewport(window_w, window_h);
        let mouse_canvas = window
            .get_mouse_pos(MouseMode::Pass)
            .and_then(|(mx, my)| map_mouse_to_canvas(mx, my, viewport));
        let hover = mouse_canvas.and_then(|(x, y)| hit_test(layout, &state, x, y));

        if hover != state.hover {
            state.hover = hover;
            dirty = true;
        }

        let mouse_down = window.get_mouse_down(MouseButton::Left);
        if mouse_down && !mouse_was_down
            && let Some((x, _y)) = mouse_canvas
            && let Some(HitTarget::Field(field)) = hover
        {
            let index = field_char_index(assets, layout, &state, field, x);
            state.selection = Some(TextSelection::collapsed(field, index));
            state.open_menu = None;
            state.has_interacted = true;
            drag_selection = Some((field, index));
            dirty = true;
        }

        if mouse_down
            && let Some((field, anchor)) = drag_selection
            && let Some((x, _y)) = mouse_canvas
        {
            let index = field_char_index(assets, layout, &state, field, x);
            let mut changed = false;
            match state.selection.as_mut() {
                Some(selection) if selection.field == field => {
                    if selection.anchor != anchor || selection.caret != index {
                        selection.anchor = anchor;
                        selection.caret = index;
                        changed = true;
                    }
                }
                _ => {
                    state.selection = Some(TextSelection {
                        field,
                        anchor,
                        caret: index,
                    });
                    changed = true;
                }
            }
            if changed {
                dirty = true;
            }
        }

        if !mouse_down && mouse_was_down {
            if drag_selection.take().is_none() {
                if let Some(hit) = state.hover {
                    if handle_click(&mut state, hit) {
                        dirty = true;
                    }
                } else if state.open_menu.take().is_some() {
                    dirty = true;
                } else if state.selection.take().is_some() {
                    dirty = true;
                }
            }
        }
        mouse_was_down = mouse_down;

        if dirty {
            render_scene(&mut frame, static_frame, assets, layout, &state, true);
        }

        if needs_present || dirty || last_size != (window_w, window_h) {
            let buffer = present_buffer(&frame, window_w, window_h);
            last_size = (window_w, window_h);
            needs_present = false;
            window.update_with_buffer(&buffer, window_w, window_h)?;
        } else {
            window.update();
        }
    }

    Ok(())
}

fn render_static_scene(assets: &Assets, layout: &Layout) -> Result<RgbaImage> {
    let mut img = RgbaImage::from_pixel(BASE_W, BASE_H, BG);

    stroke_rect(&mut img, layout.outer, BORDER, 3);
    fill_stroke_rect(&mut img, layout.header_left, HEADER_BG, BORDER, 3);
    fill_stroke_rect(&mut img, layout.header_center, HEADER_BG, BORDER, 3);
    fill_stroke_rect(&mut img, layout.status_left, PANEL_BG, BORDER, 3);
    fill_stroke_rect(&mut img, layout.badge, PANEL_BG, BORDER, 3);
    fill_stroke_rect(&mut img, layout.status_right, PANEL_BG, BORDER, 3);
    fill_stroke_rect(&mut img, layout.left_panel, PANEL_BG, BORDER, 3);
    fill_stroke_rect(&mut img, layout.portrait, PANEL_BG, BORDER, 3);
    fill_stroke_rect(&mut img, layout.finger, PANEL_BG, BORDER, 3);

    draw_header_static(&mut img, assets, layout)?;
    draw_status_static(&mut img, assets, layout);
    draw_left_panel_static(&mut img, assets, layout);
    draw_portrait_panel_static(&mut img, assets, layout)?;
    draw_fingerprint_panel_static(&mut img, assets, layout)?;

    Ok(img)
}

fn render_scene(
    frame: &mut RgbaImage,
    static_frame: &RgbaImage,
    assets: &Assets,
    layout: &Layout,
    state: &AppState,
    interactive: bool,
) {
    frame.clone_from(static_frame);
    draw_header_dynamic(frame, layout, state, interactive);
    draw_status_dynamic(frame, assets, layout, state, interactive);
    draw_left_panel_dynamic(frame, assets, layout, state, interactive);
    draw_editable_fields_dynamic(frame, assets, layout, state, interactive);
    draw_portrait_panel_dynamic(frame, assets, layout, state, interactive);
    draw_fingerprint_panel_dynamic(frame, assets, layout, state, interactive);

    if interactive && state.has_interacted {
        match state.view {
            ViewMode::Profile => stroke_rect(frame, layout.left_panel.inner(6, 6), blend(BORDER, BADGE, 0.3), 2),
            ViewMode::Biometrics => {
                stroke_rect(frame, layout.portrait.inner(4, 4), blend(BORDER, BADGE, 0.34), 2);
                stroke_rect(frame, layout.finger.inner(4, 4), blend(BORDER, BADGE, 0.34), 2);
            }
            ViewMode::Relations => stroke_rect(frame, layout.relations_focus, blend(BORDER, BADGE, 0.34), 2),
        }
    }

    if interactive && let Some(kind) = state.open_menu {
        draw_menu_overlay(frame, assets, layout, state, kind);
    }
}

fn draw_header_static(
    img: &mut RgbaImage,
    assets: &Assets,
    layout: &Layout,
) -> Result<()> {
    let logo_area = PxRect::new(layout.header_left.x + 11, layout.header_left.y + 6, 56, 20);
    let logo = crop_norm(&assets.source, LOGO_CROP)?;
    overlay_fit(img, &logo, logo_area);
    draw_text_scaled(
        img,
        &assets.narrow_bold,
        21.0,
        VALUE,
        layout.header_left.x + 70,
        layout.header_left.y + 6,
        "WEYLAND-YUTANI CORP",
        0.94,
    );
    draw_centered_text_scaled(
        img,
        &assets.narrow_bold,
        21.0,
        VALUE,
        layout.header_center,
        layout.header_center.y + 6,
        "COLONY AFFAIRS DATABASE",
        0.58,
        0.94,
    );

    Ok(())
}

fn draw_header_dynamic(img: &mut RgbaImage, layout: &Layout, state: &AppState, interactive: bool) {
    if !interactive {
        return;
    }
    if state.hover == Some(HitTarget::Tab(HeaderTab::Corp)) || state.open_menu == Some(MenuKind::Corp) {
        let line = PxRect::new(layout.header_left.x + 8, layout.header_left.bottom() - 5, layout.header_left.w - 16, 2);
        draw_filled_rect_mut(img, Rect::at(line.x as i32, line.y as i32).of_size(line.w, line.h), BADGE);
    }
    if state.hover == Some(HitTarget::Tab(HeaderTab::Database)) || state.open_menu == Some(MenuKind::Database) {
        let line = PxRect::new(layout.header_center.x + 8, layout.header_center.bottom() - 5, layout.header_center.w - 16, 2);
        draw_filled_rect_mut(img, Rect::at(line.x as i32, line.y as i32).of_size(line.w, line.h), BADGE);
    }
}

fn draw_status_static(img: &mut RgbaImage, assets: &Assets, layout: &Layout) {
    draw_text_scaled(
        img,
        &assets.narrow_regular,
        18.0,
        LABEL,
        layout.status_left.x + 18,
        layout.status_left.y + 14,
        "USER:",
        0.95,
    );
    draw_text_scaled(
        img,
        &assets.narrow_regular,
        18.5,
        VALUE,
        layout.status_left.x + 137,
        layout.status_left.y + 13,
        "OFFICER AD",
        0.93,
    );
    draw_text_scaled(
        img,
        &assets.narrow_regular,
        18.0,
        LABEL,
        layout.status_left.x + 18,
        layout.status_left.y + 38,
        "SETTLEMENT:",
        0.95,
    );
    draw_text_scaled(
        img,
        &assets.narrow_regular,
        18.5,
        VALUE,
        layout.status_left.x + 137,
        layout.status_left.y + 37,
        "JACKSON'S STAR COLONY",
        0.93,
    );
    draw_text_scaled(
        img,
        &assets.narrow_regular,
        18.0,
        LABEL,
        layout.status_right.x + 22,
        layout.status_right.y + 38,
        "LOG_ID",
        0.95,
    );
}

fn draw_status_dynamic(
    img: &mut RgbaImage,
    assets: &Assets,
    layout: &Layout,
    state: &AppState,
    interactive: bool,
) {
    let inner = PxRect::new(layout.badge.x + 12, layout.badge.y + 10, layout.badge.w - 24, layout.badge.h - 20);
    let badge_fill = if interactive && (state.hover == Some(HitTarget::Badge) || state.open_menu == Some(MenuKind::Badge)) {
        blend(blend(BADGE, PANEL_BG, 0.18), VALUE, 0.08)
    } else {
        blend(BADGE, PANEL_BG, 0.18)
    };
    draw_filled_rect_mut(img, Rect::at(inner.x as i32, inner.y as i32).of_size(inner.w, inner.h), badge_fill);
    stroke_rect(img, inner, blend(BADGE, VALUE, 0.12), 1);
    draw_centered_text_scaled(
        img,
        &assets.narrow_bold,
        21.0,
        blend(PANEL_BG, MUTED, 0.18),
        inner,
        inner.y + 8,
        state.badge_mode.glyph(),
        0.62,
        0.9,
    );

    let clock = format_clock_text(state.clock_seconds);
    draw_text_scaled(
        img,
        &assets.narrow_regular,
        17.0,
        LABEL,
        layout.status_right.x + 22,
        layout.status_right.y + 15,
        &clock,
        0.95,
    );
    draw_text_scaled(
        img,
        &assets.narrow_bold,
        21.0,
        VALUE,
        layout.status_right.x + 182,
        layout.status_right.y + 11,
        if interactive && state.has_interacted {
            state.badge_mode.status_text()
        } else {
            "SYSTEM ONLINE"
        },
        0.93,
    );
    if interactive && state.has_interacted {
        let detail = match state.view {
            ViewMode::Profile => "PROFILE",
            ViewMode::Biometrics => "BIO-METRIC",
            ViewMode::Relations => "RELATIONS",
        };
        draw_text_scaled(
            img,
            &assets.narrow_regular,
            18.0,
            LABEL,
            layout.status_right.x + 102,
            layout.status_right.y + 38,
            detail,
            0.95,
        );
    }
}

fn draw_left_panel_static(img: &mut RgbaImage, assets: &Assets, layout: &Layout) {
    let area = layout.left_panel;
    draw_text(img, &assets.narrow_regular, 18.0, LABEL, area.x + 20, area.y + 34, "CITIZEN ID:");
    draw_right_text_scaled(
        img,
        &assets.mono,
        26.0,
        VALUE,
        area.right() - 24,
        area.y + 21,
        "FWC25583",
        0.58,
        0.95,
    );
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
        draw_text(img, &assets.narrow_regular, 18.0, LABEL, area.x + 20, y, &format!("{label}:"));
        draw_right_text(
            img,
            &assets.narrow_regular,
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
        &assets.narrow_regular,
        18.0,
        LABEL,
        area.x + 20,
        area.bottom() - 118,
        "Education Records:",
    );
    draw_right_text(
        img,
        &assets.narrow_regular,
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
        &assets.narrow_regular,
        18.0,
        LABEL,
        area.x + 20,
        area.bottom() - 62,
        "Known Relations:",
    );
    draw_right_text(
        img,
        &assets.narrow_regular,
        20.0,
        VALUE,
        area.right() - 18,
        area.bottom() - 63,
        "CARRADINE, E (DECEASED)",
        0.52,
    );
    draw_right_text(
        img,
        &assets.narrow_regular,
        20.0,
        VALUE,
        area.right() - 18,
        area.bottom() - 32,
        "CARRADINE, S (DECEASED)",
        0.52,
    );
}

fn draw_left_panel_dynamic(
    img: &mut RgbaImage,
    assets: &Assets,
    layout: &Layout,
    state: &AppState,
    interactive: bool,
) {
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
    draw_text(img, &assets.narrow_regular, 18.0, PANEL_BG, dept_label.x + 12, dept_label.y + 8, "DEPT.");
    draw_text(
        img,
        &assets.narrow_bold,
        19.0,
        VALUE,
        layout.dept.x + 92,
        layout.dept.y + 8,
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
            &assets.narrow_bold,
            22.0,
            if is_selected { PANEL_BG } else { VALUE },
            box_rect,
            box_rect.y + 7,
            gender.label(),
            0.55,
        );
    }
}

fn draw_editable_fields_dynamic(
    img: &mut RgbaImage,
    assets: &Assets,
    layout: &Layout,
    state: &AppState,
    interactive: bool,
) {
    for field in EditableField::ORDER {
        let style = field_style(layout, field);
        let is_hovered = interactive && state.hover == Some(HitTarget::Field(field));
        let is_active = state.selection.as_ref().map(|selection| selection.field) == Some(field);
        let fill = if is_active {
            blend(PANEL_BG, HEADER_BG, 0.08)
        } else {
            PANEL_BG
        };
        draw_filled_rect_mut(img, Rect::at(style.rect.x as i32, style.rect.y as i32).of_size(style.rect.w, style.rect.h), fill);
        draw_editable_text_value(img, assets, state, style, field);
        if is_active {
            stroke_rect(img, style.rect, BADGE, 1);
        } else if is_hovered {
            stroke_rect(img, style.rect, blend(BORDER, VALUE, 0.25), 1);
        }
    }
}

fn draw_editable_text_value(
    img: &mut RgbaImage,
    assets: &Assets,
    state: &AppState,
    style: FieldStyle,
    field: EditableField,
) {
    let font = resolve_field_font(assets, style.font);
    let text = dossier_field(&state.dossier, field);
    let text_width = estimate_text_width_scaled(font, text, style.size, 1.0, style.x_scale);
    let text_x = match style.align {
        TextAlign::Left => style.rect.x,
        TextAlign::Right => style.rect.right().saturating_sub(text_width),
    };

    if let Some(selection) = state.selection.as_ref().filter(|selection| selection.field == field) {
        draw_selection_highlight(img, font, style, text, selection, text_x);
    }

    draw_text_scaled(
        img,
        font,
        style.size,
        style.color,
        text_x,
        style.baseline_y,
        text,
        style.x_scale,
    );

    if let Some(selection) = state.selection.as_ref().filter(|selection| selection.field == field) {
        draw_text_caret(img, font, style, text, selection, text_x);
    }
}

fn draw_portrait_panel_static(img: &mut RgbaImage, assets: &Assets, layout: &Layout) -> Result<()> {
    let target = layout.portrait.inner(4, 4);
    overlay_fit_rgba(img, &assets.portrait_crt, target, FilterType::CatmullRom);
    stroke_rect(img, layout.portrait, BORDER, 3);
    Ok(())
}

fn draw_portrait_panel_dynamic(
    img: &mut RgbaImage,
    assets: &Assets,
    layout: &Layout,
    state: &AppState,
    interactive: bool,
) {
    if interactive && state.portrait_progress < 0.995 {
        let target = layout.portrait.inner(4, 4);
        draw_filled_rect_mut(img, Rect::at(target.x as i32, target.y as i32).of_size(target.w, target.h), PANEL_BG);
        draw_pixelated_fade(img, &assets.portrait, target, state.portrait_progress);
        let haze_alpha = ((state.portrait_progress - 0.58) / 0.42).clamp(0.0, 1.0) * 0.85;
        if haze_alpha > 0.001 {
            overlay_fit_rgba_alpha(
                img,
                &assets.portrait_crt,
                target,
                FilterType::CatmullRom,
                haze_alpha,
            );
        }
        stroke_rect(img, layout.portrait, BORDER, 3);
    }
    if interactive && state.hover == Some(HitTarget::Tab(HeaderTab::Database)) {
        stroke_rect(img, layout.portrait.inner(8, 8), blend(BORDER, VALUE, 0.2), 1);
    }
}

fn draw_fingerprint_panel_static(
    img: &mut RgbaImage,
    assets: &Assets,
    layout: &Layout,
) -> Result<()> {
    let labels_y = layout.finger.y + 8;
    let step = layout.finger.w / 5;

    for i in 0..5u32 {
        let x = layout.finger.x + i * step;
        if i > 0 {
            draw_filled_rect_mut(img, Rect::at(x as i32, layout.finger.y as i32).of_size(2, layout.finger.h), BORDER);
        }

        let label_area = PxRect::new(x, labels_y, step, 18);
        draw_centered_text(
            img,
            &assets.narrow_regular,
            14.0,
            VALUE,
            label_area,
            labels_y,
            &format!("{:02}", i + 1),
            0.55,
        );

        let crop = fingerprint_crop(i as usize);
        let sprite = crop_norm(&assets.source, crop)?;
        let cell = layout.finger_cells[i as usize];
        overlay_fit(img, &sprite, cell);
    }

    stroke_rect(img, layout.finger, BORDER, 3);
    Ok(())
}

fn draw_fingerprint_panel_dynamic(
    img: &mut RgbaImage,
    assets: &Assets,
    layout: &Layout,
    state: &AppState,
    interactive: bool,
) {
    if !interactive {
        return;
    }

    let labels_y = layout.finger.y + 8;
    let step = layout.finger.w / 5;

    for i in 0..5u32 {
        let is_selected = state.selected_print == i as usize;
        let is_hovered = state.hover == Some(HitTarget::Fingerprint(i as usize));
        if !is_selected && !is_hovered {
            continue;
        }

        let x = layout.finger.x + i * step;
        let chip = PxRect::new(x + step / 2 - 18, labels_y - 2, 36, 18);
        draw_filled_rect_mut(
            img,
            Rect::at(chip.x as i32, chip.y as i32).of_size(chip.w, chip.h),
            if is_selected { BADGE } else { blend(PANEL_BG, HEADER_BG, 0.32) },
        );
        draw_centered_text(
            img,
            &assets.narrow_bold,
            15.0,
            if is_selected { PANEL_BG } else { VALUE },
            chip,
            chip.y + 2,
            &format!("{:02}", i + 1),
            0.55,
        );

        let cell = layout.finger_cells[i as usize];
        stroke_rect(img, cell.inner(1, 1), if is_selected { BADGE } else { blend(BORDER, VALUE, 0.25) }, 2);
    }
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
    draw_text(img, &assets.narrow_bold, 17.0, VALUE, panel.x + 12, panel.y + 8, kind.title());
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
        draw_text(img, &assets.narrow_regular, 19.0, VALUE, row.x + 10, row.y + 3, entry.label);
        draw_right_text(
            img,
            &assets.narrow_regular,
            15.0,
            LABEL,
            row.right() - 8,
            row.y + 6,
            entry.detail,
            0.50,
        );
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
    for field in EditableField::ORDER {
        if field_style(layout, field).rect.contains(x, y) {
            return Some(HitTarget::Field(field));
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
        HitTarget::Field(_) => false,
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

fn drain_text_input(buffer: &Arc<Mutex<Vec<char>>>) -> Vec<char> {
    if let Ok(mut chars) = buffer.lock() {
        chars.drain(..).collect()
    } else {
        Vec::new()
    }
}

fn apply_text_input(state: &mut AppState, chars: Vec<char>) -> bool {
    if chars.is_empty() {
        return false;
    }
    let Some(selection) = state.selection.as_mut() else {
        return false;
    };
    let insert: String = chars.into_iter().collect();
    replace_selection_text(dossier_field_mut(&mut state.dossier, selection.field), selection, &insert);
    state.has_interacted = true;
    true
}

fn handle_text_edit_keys(window: &Window, state: &mut AppState) -> bool {
    let Some(selection) = state.selection.as_mut() else {
        return false;
    };
    let mut dirty = false;
    let text = dossier_field_mut(&mut state.dossier, selection.field);

    if window.is_key_pressed(Key::Backspace, KeyRepeat::Yes) {
        dirty |= backspace_text(text, selection);
    }
    if window.is_key_pressed(Key::Delete, KeyRepeat::Yes) {
        dirty |= delete_text(text, selection);
    }
    if window.is_key_pressed(Key::Left, KeyRepeat::Yes) {
        move_selection_left(selection);
        dirty = true;
    }
    if window.is_key_pressed(Key::Right, KeyRepeat::Yes) {
        move_selection_right(selection, text);
        dirty = true;
    }
    if window.is_key_pressed(Key::Home, KeyRepeat::Yes) {
        selection.anchor = 0;
        selection.caret = 0;
        dirty = true;
    }
    if window.is_key_pressed(Key::End, KeyRepeat::Yes) {
        let len = text.chars().count();
        selection.anchor = len;
        selection.caret = len;
        dirty = true;
    }
    if window.is_key_pressed(Key::Enter, KeyRepeat::No) {
        let next = selection.field.next();
        let len = dossier_field(&state.dossier, next).chars().count();
        *selection = TextSelection::collapsed(next, len);
        dirty = true;
    }

    if dirty {
        state.has_interacted = true;
    }
    dirty
}

fn replace_selection_text(text: &mut String, selection: &mut TextSelection, insert: &str) {
    let (start, end) = selection.sorted();
    let start_byte = byte_index_for_char(text, start);
    let end_byte = byte_index_for_char(text, end);
    text.replace_range(start_byte..end_byte, insert);
    let next = start + insert.chars().count();
    selection.anchor = next;
    selection.caret = next;
}

fn backspace_text(text: &mut String, selection: &mut TextSelection) -> bool {
    let (start, end) = selection.sorted();
    if start != end {
        replace_selection_text(text, selection, "");
        return true;
    }
    if selection.caret == 0 {
        return false;
    }
    let prev = selection.caret - 1;
    let start_byte = byte_index_for_char(text, prev);
    let end_byte = byte_index_for_char(text, selection.caret);
    text.replace_range(start_byte..end_byte, "");
    selection.anchor = prev;
    selection.caret = prev;
    true
}

fn delete_text(text: &mut String, selection: &mut TextSelection) -> bool {
    let (start, end) = selection.sorted();
    if start != end {
        replace_selection_text(text, selection, "");
        return true;
    }
    let len = text.chars().count();
    if selection.caret >= len {
        return false;
    }
    let start_byte = byte_index_for_char(text, selection.caret);
    let end_byte = byte_index_for_char(text, selection.caret + 1);
    text.replace_range(start_byte..end_byte, "");
    true
}

fn move_selection_left(selection: &mut TextSelection) {
    let (start, end) = selection.sorted();
    let next = if start != end {
        start
    } else {
        selection.caret.saturating_sub(1)
    };
    selection.anchor = next;
    selection.caret = next;
}

fn move_selection_right(selection: &mut TextSelection, text: &str) {
    let (start, end) = selection.sorted();
    let len = text.chars().count();
    let next = if start != end { end } else { (selection.caret + 1).min(len) };
    selection.anchor = next;
    selection.caret = next;
}

fn byte_index_for_char(text: &str, index: usize) -> usize {
    text.char_indices().nth(index).map(|(byte, _)| byte).unwrap_or(text.len())
}

fn field_char_index(
    assets: &Assets,
    layout: &Layout,
    state: &AppState,
    field: EditableField,
    x: u32,
) -> usize {
    let style = field_style(layout, field);
    let font = resolve_field_font(assets, style.font);
    let text = dossier_field(&state.dossier, field);
    let text_width = estimate_text_width_scaled(font, text, style.size, 1.0, style.x_scale);
    let text_x = match style.align {
        TextAlign::Left => style.rect.x,
        TextAlign::Right => style.rect.right().saturating_sub(text_width),
    };
    let positions = glyph_positions(font, text, style.size, style.x_scale);
    let local = x.saturating_sub(text_x) as f32;

    for idx in 0..positions.len().saturating_sub(1) {
        let left = positions[idx];
        let right = positions[idx + 1];
        if local < (left + right) * 0.5 {
            return idx;
        }
    }

    text.chars().count()
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

fn overlay_fit_rgba(dst: &mut RgbaImage, src: &RgbaImage, target: PxRect, filter: FilterType) {
    let resized = image::imageops::resize(src, target.w.max(1), target.h.max(1), filter);
    image::imageops::overlay(dst, &resized, target.x.into(), target.y.into());
}

fn overlay_fit_rgba_alpha(
    dst: &mut RgbaImage,
    src: &RgbaImage,
    target: PxRect,
    filter: FilterType,
    alpha: f32,
) {
    let alpha = alpha.clamp(0.0, 1.0);
    if alpha <= 0.0 {
        return;
    }
    let resized = image::imageops::resize(src, target.w.max(1), target.h.max(1), filter);
    for py in 0..target.h {
        for px in 0..target.w {
            let sp = resized.get_pixel(px, py);
            let dp = dst.get_pixel_mut(target.x + px, target.y + py);
            for c in 0..3 {
                let mixed = dp[c] as f32 * (1.0 - alpha) + sp[c] as f32 * alpha;
                dp[c] = mixed.round().clamp(0.0, 255.0) as u8;
            }
            dp[3] = 255;
        }
    }
}

fn resize_to_window(img: &RgbaImage, w: u32, h: u32) -> RgbaImage {
    image::imageops::resize(img, w.max(1), h.max(1), FilterType::Nearest)
}

#[derive(Clone, Copy)]
struct Viewport {
    x: usize,
    y: usize,
    w: usize,
    h: usize,
}

fn fit_viewport(window_w: usize, window_h: usize) -> Viewport {
    let scale = (window_w as f32 / BASE_W as f32)
        .min(window_h as f32 / BASE_H as f32)
        .max(0.0001);
    let w = (BASE_W as f32 * scale).round().max(1.0) as usize;
    let h = (BASE_H as f32 * scale).round().max(1.0) as usize;
    Viewport {
        x: (window_w.saturating_sub(w)) / 2,
        y: (window_h.saturating_sub(h)) / 2,
        w,
        h,
    }
}

fn map_mouse_to_canvas(mx: f32, my: f32, viewport: Viewport) -> Option<(u32, u32)> {
    let vx = viewport.x as f32;
    let vy = viewport.y as f32;
    let vw = viewport.w as f32;
    let vh = viewport.h as f32;
    if mx < vx || my < vy || mx >= vx + vw || my >= vy + vh {
        return None;
    }

    let x = (((mx - vx) / vw) * BASE_W as f32)
        .floor()
        .clamp(0.0, (BASE_W - 1) as f32) as u32;
    let y = (((my - vy) / vh) * BASE_H as f32)
        .floor()
        .clamp(0.0, (BASE_H - 1) as f32) as u32;
    Some((x, y))
}

fn present_buffer(frame: &RgbaImage, window_w: usize, window_h: usize) -> Vec<u32> {
    if window_w as u32 == BASE_W && window_h as u32 == BASE_H {
        return rgba_to_u32(frame);
    }

    let viewport = fit_viewport(window_w, window_h);
    let scaled = resize_to_window(frame, viewport.w as u32, viewport.h as u32);
    let scaled_u32 = rgba_to_u32(&scaled);
    let bg = ((BG[0] as u32) << 16) | ((BG[1] as u32) << 8) | BG[2] as u32;
    let mut buffer = vec![bg; window_w * window_h];

    for row in 0..viewport.h {
        let src_start = row * viewport.w;
        let src_end = src_start + viewport.w;
        let dst_start = (viewport.y + row) * window_w + viewport.x;
        let dst_end = dst_start + viewport.w;
        buffer[dst_start..dst_end].copy_from_slice(&scaled_u32[src_start..src_end]);
    }

    buffer
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
    let inset = segment * 0.05;
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

fn resolve_field_font<'a>(assets: &'a Assets, font: FieldFont) -> &'a FontArc {
    match font {
        FieldFont::Mono => &assets.mono,
        FieldFont::NarrowRegular => &assets.narrow_regular,
    }
}

fn draw_text_scaled(
    img: &mut RgbaImage,
    font: &FontArc,
    size: f32,
    color: Rgba<u8>,
    x: u32,
    y: u32,
    text: &str,
    x_scale: f32,
) {
    if (x_scale - 1.0).abs() < 0.01 {
        draw_text(img, font, size, color, x, y, text);
        return;
    }

    let base_w = estimate_text_width(font, text, size, 1.0).saturating_add(8);
    let base_h = ((size * 1.65).ceil() as u32).saturating_add(8);
    let mut temp = RgbaImage::from_pixel(base_w.max(1), base_h.max(1), Rgba([0, 0, 0, 0]));
    draw_text_mut(&mut temp, color, 0, 0, size, font, text);
    let scaled_w = ((base_w as f32) * x_scale).round().max(1.0) as u32;
    let scaled = image::imageops::resize(&temp, scaled_w, base_h, FilterType::CatmullRom);
    image::imageops::overlay(img, &scaled, x.into(), y.into());
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
    let w = estimate_text_width(font, text, size, factor);
    let x = area.x + area.w.saturating_sub(w) / 2;
    draw_text(img, font, size, color, x, y, text);
}

fn draw_centered_text_scaled(
    img: &mut RgbaImage,
    font: &FontArc,
    size: f32,
    color: Rgba<u8>,
    area: PxRect,
    y: u32,
    text: &str,
    factor: f32,
    x_scale: f32,
) {
    let w = estimate_text_width_scaled(font, text, size, factor, x_scale);
    let x = area.x + area.w.saturating_sub(w) / 2;
    draw_text_scaled(img, font, size, color, x, y, text, x_scale);
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
    let w = estimate_text_width(font, text, size, factor);
    draw_text(img, font, size, color, right.saturating_sub(w), y, text);
}

fn draw_right_text_scaled(
    img: &mut RgbaImage,
    font: &FontArc,
    size: f32,
    color: Rgba<u8>,
    right: u32,
    y: u32,
    text: &str,
    factor: f32,
    x_scale: f32,
) {
    let w = estimate_text_width_scaled(font, text, size, factor, x_scale);
    draw_text_scaled(img, font, size, color, right.saturating_sub(w), y, text, x_scale);
}

fn estimate_text_width(font: &FontArc, text: &str, size: f32, _factor: f32) -> u32 {
    let scaled = font.as_scaled(size);
    let mut width = 0.0f32;
    let mut previous: Option<GlyphId> = None;

    for ch in text.chars() {
        let glyph = scaled.glyph_id(ch);
        if let Some(prev) = previous {
            width += scaled.kern(prev, glyph);
        }
        width += scaled.h_advance(glyph);
        previous = Some(glyph);
    }

    width.ceil().max(1.0) as u32
}

fn estimate_text_width_scaled(font: &FontArc, text: &str, size: f32, factor: f32, x_scale: f32) -> u32 {
    ((estimate_text_width(font, text, size, factor) as f32) * x_scale)
        .round()
        .max(1.0) as u32
}

fn glyph_positions(font: &FontArc, text: &str, size: f32, x_scale: f32) -> Vec<f32> {
    let scaled = font.as_scaled(size);
    let mut width = 0.0f32;
    let mut previous: Option<GlyphId> = None;
    let mut positions = vec![0.0];

    for ch in text.chars() {
        let glyph = scaled.glyph_id(ch);
        if let Some(prev) = previous {
            width += scaled.kern(prev, glyph);
        }
        width += scaled.h_advance(glyph);
        positions.push(width * x_scale);
        previous = Some(glyph);
    }

    positions
}

fn draw_selection_highlight(
    img: &mut RgbaImage,
    font: &FontArc,
    style: FieldStyle,
    text: &str,
    selection: &TextSelection,
    text_x: u32,
) {
    let (start, end) = selection.sorted();
    let positions = glyph_positions(font, text, style.size, style.x_scale);
    let left = positions.get(start).copied().unwrap_or(0.0);
    let right = positions.get(end).copied().unwrap_or(left);
    if right <= left {
        return;
    }
    let x = text_x + left.round() as u32;
    let w = (right - left).round().max(1.0) as u32;
    let y = style.rect.y + 2;
    let h = style.rect.h.saturating_sub(4);
    draw_filled_rect_mut(
        img,
        Rect::at(x as i32, y as i32).of_size(w, h),
        blend(BADGE, PANEL_BG, 0.18),
    );
}

fn draw_text_caret(
    img: &mut RgbaImage,
    font: &FontArc,
    style: FieldStyle,
    text: &str,
    selection: &TextSelection,
    text_x: u32,
) {
    let (start, end) = selection.sorted();
    if start != end {
        return;
    }
    let positions = glyph_positions(font, text, style.size, style.x_scale);
    let x = text_x + positions.get(selection.caret).copied().unwrap_or(0.0).round() as u32;
    let y = style.rect.y + 3;
    let h = style.rect.h.saturating_sub(6).max(1);
    draw_filled_rect_mut(img, Rect::at(x as i32, y as i32).of_size(2, h), BADGE);
}

fn draw_pixelated_fade(dst: &mut RgbaImage, src: &RgbaImage, target: PxRect, progress: f32) {
    let eased = progress.clamp(0.0, 1.0);
    let downsample = (((1.0 - eased).powf(1.8) * 34.0).round() as u32).max(1);
    let small_w = (target.w / downsample).max(1);
    let small_h = (target.h / downsample).max(1);
    let tiny = image::imageops::resize(src, small_w, small_h, FilterType::Triangle);
    let pixelated = image::imageops::resize(&tiny, target.w.max(1), target.h.max(1), FilterType::Nearest);
    let alpha = (eased * eased).clamp(0.0, 1.0);

    for py in 0..target.h {
        for px in 0..target.w {
            let src_px = pixelated.get_pixel(px, py);
            let dst_px = dst.get_pixel_mut(target.x + px, target.y + py);
            for channel in 0..3 {
                let mixed = (dst_px[channel] as f32 * (1.0 - alpha)) + (src_px[channel] as f32 * alpha);
                dst_px[channel] = mixed.round().clamp(0.0, 255.0) as u8;
            }
            dst_px[3] = 255;
        }
    }
}

fn build_crt_portrait(src: &RgbaImage) -> RgbaImage {
    let (w, h) = src.dimensions();
    let bloom_small = image::imageops::resize(
        src,
        (w / 5).max(1),
        (h / 5).max(1),
        FilterType::Triangle,
    );
    let bloom = image::imageops::resize(&bloom_small, w.max(1), h.max(1), FilterType::CatmullRom);
    let smear_small = image::imageops::resize(
        src,
        (w / 3).max(1),
        (h / 9).max(1),
        FilterType::Triangle,
    );
    let smear = image::imageops::resize(&smear_small, w.max(1), h.max(1), FilterType::CatmullRom);
    let mut out = src.clone();

    for y in 0..h {
        let scan = match y % 3 {
            0 => 0.91,
            1 => 1.02,
            _ => 0.965,
        };
        let fy = y as f32 / h.max(1) as f32 - 0.5;

        for x in 0..w {
            let fx = x as f32 / w.max(1) as f32 - 0.5;
            let vignette = (1.035 - fx.abs() * 0.12 - fy.abs() * 0.09).clamp(0.88, 1.04);
            let sp = src.get_pixel(x, y);
            let bp = bloom.get_pixel(x, y);
            let hp = smear.get_pixel(x, y);
            let luma =
                (0.299 * sp[0] as f32 + 0.587 * sp[1] as f32 + 0.114 * sp[2] as f32) / 255.0;
            let bloom_mix = 0.22 + luma * luma * 0.24;
            let smear_mix = 0.08 + luma * 0.10;
            let haze = 10.0 + luma * 18.0;
            let glass = 4.0 + (0.5 - fy.abs()).max(0.0) * 6.0;
            let px = out.get_pixel_mut(x, y);

            let base_r = sp[0] as f32 * (1.0 - bloom_mix - smear_mix)
                + bp[0] as f32 * bloom_mix
                + hp[0] as f32 * smear_mix;
            let base_g = sp[1] as f32 * (1.0 - bloom_mix - smear_mix)
                + bp[1] as f32 * bloom_mix
                + hp[1] as f32 * smear_mix;
            let base_b = sp[2] as f32 * (1.0 - bloom_mix - smear_mix)
                + bp[2] as f32 * bloom_mix
                + hp[2] as f32 * smear_mix;

            let r = (base_r + haze * 0.90 + glass * 0.55)
                * scan
                * vignette;
            let g = (base_g + haze * 1.05 + glass * 0.78)
                * scan
                * vignette;
            let b = (base_b + haze * 0.72 + glass * 0.42)
                * scan
                * vignette;

            px[0] = r.round().clamp(0.0, 255.0) as u8;
            px[1] = g.round().clamp(0.0, 255.0) as u8;
            px[2] = b.round().clamp(0.0, 255.0) as u8;
            px[3] = 255;
        }
    }

    out
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
