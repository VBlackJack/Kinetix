/*
 * Copyright 2026 Julien Bombled
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

//! Pure page layout: widgets, texts and hit-testing, all in arena coordinates.

use std::ops::Range;

use kinetix_core::i18n::I18nCatalog;

use super::model::{MenuAction, MenuModel};

// ---- Fonts -------------------------------------------------------------------------------

/// Every font size the menu draws with except the Home title; the application preloads these
/// glyph sizes for the full character set.
pub const MENU_FONT_SIZES: &[u16] = &[21, 22, 24, 28, 30, 32, 48];
/// Size of the Home title: only the title characters are preloaded at this size, and it is the
/// single source for both the menu layout and the glyph atlas prewarm.
pub const SIZE_HOME_TITLE: u16 = 96;
pub const SIZE_PAGE_TITLE: u16 = 48;
pub const SIZE_SUBTITLE: u16 = 32;
pub const SIZE_BUTTON: u16 = 30;
pub const SIZE_VALUE: u16 = 28;
pub const SIZE_SMALL: u16 = 24;
pub const SIZE_CONTROLS: u16 = 22;

// ---- Panel geometry (same frame as the original title screen) ------------------------------

pub const PANEL_X: f32 = 310.0;
pub const PANEL_Y: f32 = 110.0;
pub const PANEL_W: f32 = 1300.0;
pub const PANEL_H: f32 = 860.0;
pub const PANEL_RADIUS: f32 = 26.0;
pub const PANEL_BORDER: f32 = 4.0;
pub const HEADER_INSET_X: f32 = 30.0;
pub const HEADER_INSET_Y: f32 = 16.0;
pub const HEADER_H: f32 = 90.0;
pub const HEADER_RADIUS: f32 = 16.0;

const PANEL_CENTER_X: f32 = PANEL_X + PANEL_W * 0.5;
const PANEL_RIGHT_INSET: f32 = 60.0;

// ---- Text rows -----------------------------------------------------------------------------

const HOME_TITLE_Y: f32 = 250.0;
const HOME_SUBTITLE_Y: f32 = 300.0;
const PAGE_TITLE_Y: f32 = 190.0;
const SUBTITLE_Y: f32 = 240.0;
const COUNT_Y: f32 = 190.0;
const EMPTY_TEXT_Y: f32 = 360.0;
const CONTROLS_FIRST_Y: f32 = 290.0;
const CONTROLS_STEP_Y: f32 = 56.0;
const PAGE_INDICATOR_GAP: f32 = 40.0;
const PAGE_INDICATOR_DY: f32 = 40.0;
const HIGH_SCORE_Y: f32 = 900.0;
/// Digits of the high score shown on the Home page.
const HIGH_SCORE_DIGITS: usize = 7;

// ---- Widget geometry -----------------------------------------------------------------------

const BUTTON_W: f32 = 640.0;
const BUTTON_H: f32 = 68.0;
const BUTTON_GAP: f32 = 16.0;
const HOME_BUTTONS_Y: f32 = 370.0;
const PAUSE_BUTTONS_Y: f32 = 300.0;
const ROW_W: f32 = 1000.0;
const ROW_H: f32 = 50.0;
const ROW_GAP: f32 = 8.0;
const LIST_TOP_Y: f32 = 250.0;
const SATISFACTION_LIST_TOP_Y: f32 = 290.0;
const OPTION_ROW_H: f32 = 64.0;
const OPTION_ROW_GAP: f32 = 14.0;
const DELETE_W: f32 = 220.0;
const DELETE_GAP: f32 = 12.0;
const BACK_W: f32 = 320.0;
const BACK_H: f32 = 60.0;
const BACK_Y: f32 = PANEL_Y + PANEL_H - 110.0;
const PAGE_BUTTON_W: f32 = 160.0;
const PAGE_BUTTON_GAP: f32 = 24.0;

// ---- Paging --------------------------------------------------------------------------------

pub const CHAPTERS_PER_PAGE: usize = 8;
pub const SECTORS_PER_PAGE: usize = 10;
pub const SATISFACTION_PER_PAGE: usize = 8;
pub const LIBRARY_PER_PAGE: usize = 8;

/// Label of an armed library Delete button.
pub const CONFIRM_DELETE_KEY: &str = "menu.library.button.confirm_delete";

/// Locale keys of the read-only controls reference, in display order.
pub const CONTROLS_KEYS: &[&str] = &[
    "controls.launch_hint",
    "controls.laser_hint",
    "controls.theme_hint",
    "controls.lang_hint",
    "controls.level_hint",
    "controls.editor_hint",
    "controls.mute_hint",
    "controls.pause_hint",
    "controls.restart_hint",
];

/// Pages of the menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageId {
    Home,
    Chapters,
    /// Sectors of the world at this index of `MenuModel::worlds`.
    Sectors(usize),
    Options,
    Controls,
    Pause,
    Satisfaction,
    Library,
}

/// Axis-aligned rectangle in arena coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl UiRect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px < self.x + self.w && py >= self.y && py < self.y + self.h
    }
}

/// Text that is resolved against the locale catalog at draw time.
#[derive(Debug, Clone, PartialEq)]
pub enum Label {
    Key(&'static str),
    Text(String),
    Template {
        key: &'static str,
        args: Vec<(&'static str, String)>,
    },
}

impl Label {
    pub fn resolve(&self, i18n: &I18nCatalog) -> String {
        match self {
            Self::Key(key) => i18n.t(key).to_owned(),
            Self::Text(text) => text.clone(),
            Self::Template { key, args } => substitute(i18n.t(key), args),
        }
    }

    /// The locale key this label reads, if any.
    pub fn key(&self) -> Option<&'static str> {
        match self {
            Self::Key(key) | Self::Template { key, .. } => Some(key),
            Self::Text(_) => None,
        }
    }
}

/// Replaces every `{name}` placeholder of `template` by its value.
pub fn substitute(template: &str, args: &[(&'static str, String)]) -> String {
    let mut out = template.to_owned();
    for (name, value) in args {
        out = out.replace(&format!("{{{name}}}"), value);
    }
    out
}

/// What a widget is, for navigation purposes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// A list row; the value is the absolute item index.
    Item(usize),
    /// The delete button of the library row with this item index.
    Delete(usize),
    Extra,
    Back,
    /// Pointer-only button that shows the previous list page.
    PagePrev,
    /// Pointer-only button that shows the next list page.
    PageNext,
}

impl Role {
    /// True for the pointer-only paging buttons, which keyboard focus skips.
    pub fn is_page_button(self) -> bool {
        matches!(self, Self::PagePrev | Self::PageNext)
    }
}

/// Direction of a paging button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageStep {
    Prev,
    Next,
}

/// What activating a widget does.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Activation {
    Navigate(PageId),
    Back,
    Emit(MenuAction),
    /// Shows the previous or next list page.
    Page(PageStep),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Widget {
    pub rect: UiRect,
    pub label: Label,
    /// Right-aligned secondary text.
    pub value: Option<Label>,
    pub size: u16,
    pub role: Role,
    pub enabled: bool,
    /// True when activating this widget starts play (used for the pointer-lock mask).
    pub starts_play: bool,
    /// True when Left/Right also activate this widget (option rows).
    pub adjust: bool,
    pub activation: Activation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Primary,
    Accent,
}

/// A static text drawn by the page.
#[derive(Debug, Clone, PartialEq)]
pub struct TextItem {
    pub label: Label,
    pub x: f32,
    pub y: f32,
    pub size: u16,
    pub align: Align,
    pub tone: Tone,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PageLayout {
    pub page: PageId,
    /// Home draws a large logo block instead of a header band.
    pub is_home: bool,
    pub texts: Vec<TextItem>,
    pub widgets: Vec<Widget>,
    /// Total list items (0 for pages that are not lists).
    pub item_count: usize,
    pub page_size: usize,
    pub list_page: usize,
    pub page_count: usize,
}

impl PageLayout {
    /// Index of the widget under the pointer, if any.
    pub fn hit(&self, x: f32, y: f32) -> Option<usize> {
        self.widgets.iter().position(|w| w.rect.contains(x, y))
    }

    /// Index of the widget carrying the list row `item` (not its delete button).
    pub fn widget_for_item(&self, item: usize) -> Option<usize> {
        self.widgets.iter().position(|w| w.role == Role::Item(item))
    }

    /// Index of the delete button of the library row `item`.
    pub fn widget_for_delete(&self, item: usize) -> Option<usize> {
        self.widgets
            .iter()
            .position(|w| w.role == Role::Delete(item))
    }

    /// Number of widgets keyboard focus cycles through (the paging buttons come last and are
    /// pointer-only).
    pub fn focusable_count(&self) -> usize {
        self.widgets
            .iter()
            .take_while(|w| !w.role.is_page_button())
            .count()
    }

    /// Absolute index of the first and last list item shown on this page.
    pub fn item_range(&self) -> Range<usize> {
        visible_range(self.item_count, self.page_size, self.list_page)
    }
}

/// Number of pages for `item_count` items (at least one).
pub fn page_count(item_count: usize, page_size: usize) -> usize {
    if page_size == 0 {
        return 1;
    }
    item_count.div_ceil(page_size).max(1)
}

/// Items shown on `list_page`.
pub fn visible_range(item_count: usize, page_size: usize, list_page: usize) -> Range<usize> {
    if page_size == 0 {
        return 0..0;
    }
    let start = (list_page * page_size).min(item_count);
    let end = (start + page_size).min(item_count);
    start..end
}

/// Largest menu font size not above `preferred` whose width fits `limit`; the smallest menu
/// size when none fits. Never returns a size outside `MENU_FONT_SIZES` plus `SIZE_HOME_TITLE`.
pub fn fit_to_sizes(measure: impl Fn(u16) -> f32, preferred: u16, limit: f32) -> u16 {
    let mut fallback = MENU_FONT_SIZES[0];
    let candidates = MENU_FONT_SIZES
        .iter()
        .copied()
        .chain(std::iter::once(SIZE_HOME_TITLE));
    for size in candidates.rev() {
        if size > preferred {
            continue;
        }
        fallback = size;
        if measure(size) <= limit {
            return size;
        }
    }
    fallback
}

fn centered_x(width: f32) -> f32 {
    PANEL_CENTER_X - width * 0.5
}

fn widget(rect: UiRect, label: Label, size: u16, role: Role, activation: Activation) -> Widget {
    Widget {
        rect,
        label,
        value: None,
        size,
        role,
        enabled: true,
        starts_play: false,
        adjust: false,
        activation,
    }
}

fn back_widget() -> Widget {
    widget(
        UiRect::new(centered_x(BACK_W), BACK_Y, BACK_W, BACK_H),
        Label::Key("menu.common.button.back"),
        SIZE_BUTTON,
        Role::Back,
        Activation::Back,
    )
}

fn page_title(label: Label) -> TextItem {
    TextItem {
        label,
        x: PANEL_CENTER_X,
        y: PAGE_TITLE_Y,
        size: SIZE_PAGE_TITLE,
        align: Align::Center,
        tone: Tone::Accent,
    }
}

fn row_rect(top: f32, ordinal: usize, width: f32) -> UiRect {
    UiRect::new(
        centered_x(width),
        top + ordinal as f32 * (ROW_H + ROW_GAP),
        width,
        ROW_H,
    )
}

fn page_indicator(list_page: usize, pages: usize) -> TextItem {
    TextItem {
        label: Label::Template {
            key: "menu.sectors.label.page",
            args: vec![
                ("page", (list_page + 1).to_string()),
                ("pages", pages.to_string()),
            ],
        },
        x: centered_x(BACK_W) + BACK_W + PAGE_BUTTON_GAP + PAGE_BUTTON_W + PAGE_INDICATOR_GAP,
        y: BACK_Y + PAGE_INDICATOR_DY,
        size: SIZE_SMALL,
        align: Align::Left,
        tone: Tone::Primary,
    }
}

fn button_column(top: f32, index: usize) -> UiRect {
    UiRect::new(
        centered_x(BUTTON_W),
        top + index as f32 * (BUTTON_H + BUTTON_GAP),
        BUTTON_W,
        BUTTON_H,
    )
}

/// Builds the layout of `page`. `list_page` is clamped to the valid range.
pub fn page_layout(page: PageId, model: &MenuModel, list_page: usize) -> PageLayout {
    let mut layout = PageLayout {
        page,
        is_home: page == PageId::Home,
        texts: Vec::new(),
        widgets: Vec::new(),
        item_count: 0,
        page_size: 0,
        list_page: 0,
        page_count: 1,
    };
    match page {
        PageId::Home => build_home(&mut layout, model),
        PageId::Pause => build_pause(&mut layout),
        PageId::Chapters => build_chapters(&mut layout, model, list_page),
        PageId::Sectors(world) => build_sectors(&mut layout, model, world, list_page),
        PageId::Options => build_options(&mut layout, model),
        PageId::Controls => build_controls(&mut layout),
        PageId::Satisfaction => build_satisfaction(&mut layout, model, list_page),
        PageId::Library => build_library(&mut layout, model, list_page),
    }
    layout
}

fn set_paging(layout: &mut PageLayout, item_count: usize, page_size: usize, list_page: usize) {
    layout.item_count = item_count;
    layout.page_size = page_size;
    layout.page_count = page_count(item_count, page_size);
    layout.list_page = list_page.min(layout.page_count - 1);
}

fn build_home(layout: &mut PageLayout, model: &MenuModel) {
    layout.texts.push(TextItem {
        label: Label::Key("app.title"),
        x: PANEL_CENTER_X,
        y: HOME_TITLE_Y,
        size: SIZE_HOME_TITLE,
        align: Align::Center,
        tone: Tone::Accent,
    });
    layout.texts.push(TextItem {
        label: Label::Key("app.subtitle"),
        x: PANEL_CENTER_X,
        y: HOME_SUBTITLE_Y,
        size: SIZE_SUBTITLE,
        align: Align::Center,
        tone: Tone::Primary,
    });
    layout.texts.push(TextItem {
        label: Label::Template {
            key: "menu.best_score",
            args: vec![(
                "score",
                format!("{:0width$}", model.high_score, width = HIGH_SCORE_DIGITS),
            )],
        },
        x: PANEL_CENTER_X,
        y: HIGH_SCORE_Y,
        size: SIZE_VALUE,
        align: Align::Center,
        tone: Tone::Primary,
    });
    let mut entries = vec![
        (
            "menu.home.button.campaign",
            Activation::Navigate(PageId::Chapters),
        ),
        (
            "menu.home.button.satisfaction",
            Activation::Navigate(PageId::Satisfaction),
        ),
        (
            "menu.home.button.editor",
            Activation::Navigate(PageId::Library),
        ),
        (
            "menu.home.button.options",
            Activation::Navigate(PageId::Options),
        ),
    ];
    if model.quit_supported {
        entries.push((
            "menu.home.button.quit",
            Activation::Emit(MenuAction::QuitApp),
        ));
    }
    for (i, (key, activation)) in entries.into_iter().enumerate() {
        layout.widgets.push(widget(
            button_column(HOME_BUTTONS_Y, i),
            Label::Key(key),
            SIZE_BUTTON,
            Role::Extra,
            activation,
        ));
    }
}

fn build_pause(layout: &mut PageLayout) {
    layout
        .texts
        .push(page_title(Label::Key("menu.pause.title")));
    let entries = [
        (
            "menu.pause.button.resume",
            Activation::Emit(MenuAction::Resume),
            true,
        ),
        (
            "menu.pause.button.restart",
            Activation::Emit(MenuAction::Restart),
            true,
        ),
        (
            "menu.pause.button.options",
            Activation::Navigate(PageId::Options),
            false,
        ),
        (
            "menu.pause.button.quit",
            Activation::Emit(MenuAction::QuitToTitle),
            false,
        ),
    ];
    for (i, (key, activation, starts_play)) in entries.into_iter().enumerate() {
        let mut w = widget(
            button_column(PAUSE_BUTTONS_Y, i),
            Label::Key(key),
            SIZE_BUTTON,
            Role::Extra,
            activation,
        );
        w.starts_play = starts_play;
        layout.widgets.push(w);
    }
}

fn build_chapters(layout: &mut PageLayout, model: &MenuModel, list_page: usize) {
    layout
        .texts
        .push(page_title(Label::Key("menu.chapters.title")));
    set_paging(layout, model.worlds.len(), CHAPTERS_PER_PAGE, list_page);
    for item in layout.item_range() {
        let world = &model.worlds[item];
        let mut w = widget(
            row_rect(
                LIST_TOP_Y,
                item - layout.list_page * CHAPTERS_PER_PAGE,
                ROW_W,
            ),
            Label::Text(world.name.clone()),
            SIZE_BUTTON,
            Role::Item(item),
            Activation::Navigate(PageId::Sectors(item)),
        );
        w.enabled = world.unlocked;
        w.value = Some(if world.unlocked {
            Label::Template {
                key: "menu.chapters.label.progress",
                args: vec![
                    ("done", world.cleared.to_string()),
                    ("total", world.total.to_string()),
                ],
            }
        } else {
            Label::Key("menu.chapters.label.locked")
        });
        layout.widgets.push(w);
    }
    finish_list_page(layout);
}

fn build_sectors(layout: &mut PageLayout, model: &MenuModel, world: usize, list_page: usize) {
    let sectors = model
        .worlds
        .get(world)
        .map(|w| w.sectors.as_slice())
        .unwrap_or(&[]);
    let name = model
        .worlds
        .get(world)
        .map(|w| w.name.clone())
        .unwrap_or_default();
    layout.texts.push(page_title(Label::Template {
        key: "menu.sectors.title",
        args: vec![("world", name)],
    }));
    set_paging(layout, sectors.len(), SECTORS_PER_PAGE, list_page);
    for item in layout.item_range() {
        let sector = &sectors[item];
        let mut w = widget(
            row_rect(
                LIST_TOP_Y,
                item - layout.list_page * SECTORS_PER_PAGE,
                ROW_W,
            ),
            Label::Text(sector.label.clone()),
            SIZE_VALUE,
            Role::Item(item),
            Activation::Emit(MenuAction::StartSector(sector.index)),
        );
        w.enabled = sector.unlocked;
        w.starts_play = sector.unlocked;
        if !sector.unlocked {
            w.value = Some(Label::Key("menu.sectors.label.locked"));
        }
        layout.widgets.push(w);
    }
    finish_list_page(layout);
}

fn build_options(layout: &mut PageLayout, model: &MenuModel) {
    layout
        .texts
        .push(page_title(Label::Key("menu.options.title")));
    let on_off = |on: bool| {
        Label::Key(if on {
            "menu.options.value.on"
        } else {
            "menu.options.value.off"
        })
    };
    let rows: [(&'static str, Option<Label>, Activation); 5] = [
        (
            "menu.options.label.language",
            Some(Label::Text(model.language.code().to_owned())),
            Activation::Emit(MenuAction::SetLanguage(model.language.next())),
        ),
        (
            "menu.options.label.renderer",
            Some(Label::Text(model.renderer_name.clone())),
            Activation::Emit(MenuAction::CycleRenderer),
        ),
        (
            "menu.options.label.sfx",
            Some(on_off(model.sfx_on)),
            Activation::Emit(MenuAction::ToggleSfx),
        ),
        (
            "menu.options.label.music",
            Some(on_off(model.music_on)),
            Activation::Emit(MenuAction::ToggleMusic),
        ),
        (
            "menu.options.button.controls",
            None,
            Activation::Navigate(PageId::Controls),
        ),
    ];
    let last = rows.len() - 1;
    for (i, (key, value, activation)) in rows.into_iter().enumerate() {
        let mut w = widget(
            UiRect::new(
                centered_x(ROW_W),
                LIST_TOP_Y + i as f32 * (OPTION_ROW_H + OPTION_ROW_GAP),
                ROW_W,
                OPTION_ROW_H,
            ),
            Label::Key(key),
            SIZE_BUTTON,
            Role::Extra,
            activation,
        );
        w.value = value;
        w.adjust = i != last;
        layout.widgets.push(w);
    }
    layout.widgets.push(back_widget());
}

fn build_controls(layout: &mut PageLayout) {
    layout
        .texts
        .push(page_title(Label::Key("menu.controls.title")));
    for (i, key) in CONTROLS_KEYS.iter().enumerate() {
        layout.texts.push(TextItem {
            label: Label::Key(key),
            x: PANEL_CENTER_X,
            y: CONTROLS_FIRST_Y + i as f32 * CONTROLS_STEP_Y,
            size: SIZE_CONTROLS,
            align: Align::Center,
            tone: Tone::Primary,
        });
    }
    layout.widgets.push(back_widget());
}

fn build_satisfaction(layout: &mut PageLayout, model: &MenuModel, list_page: usize) {
    layout
        .texts
        .push(page_title(Label::Key("menu.satisfaction.title")));
    layout.texts.push(TextItem {
        label: Label::Key("menu.satisfaction.label.subtitle"),
        x: PANEL_CENTER_X,
        y: SUBTITLE_Y,
        size: SIZE_SMALL,
        align: Align::Center,
        tone: Tone::Primary,
    });
    set_paging(
        layout,
        model.satisfaction_levels.len(),
        SATISFACTION_PER_PAGE,
        list_page,
    );
    for item in layout.item_range() {
        let mut w = widget(
            row_rect(
                SATISFACTION_LIST_TOP_Y,
                item - layout.list_page * SATISFACTION_PER_PAGE,
                ROW_W,
            ),
            Label::Text(model.satisfaction_levels[item].clone()),
            SIZE_BUTTON,
            Role::Item(item),
            Activation::Emit(MenuAction::StartSatisfaction(item)),
        );
        w.starts_play = true;
        layout.widgets.push(w);
    }
    finish_list_page(layout);
}

fn build_library(layout: &mut PageLayout, model: &MenuModel, list_page: usize) {
    layout
        .texts
        .push(page_title(Label::Key("menu.library.title")));
    layout.texts.push(TextItem {
        label: Label::Template {
            key: "menu.library.label.count",
            args: vec![
                ("count", model.library.len().to_string()),
                ("max", model.library_max.to_string()),
            ],
        },
        x: PANEL_X + PANEL_W - PANEL_RIGHT_INSET,
        y: COUNT_Y,
        size: SIZE_VALUE,
        align: Align::Right,
        tone: Tone::Primary,
    });
    layout.widgets.push(widget(
        row_rect(LIST_TOP_Y, 0, ROW_W),
        Label::Key("menu.library.button.new"),
        SIZE_BUTTON,
        Role::Extra,
        Activation::Emit(MenuAction::OpenEditor),
    ));
    set_paging(layout, model.library.len(), LIBRARY_PER_PAGE, list_page);
    if model.library.is_empty() {
        layout.texts.push(TextItem {
            label: Label::Key("menu.library.label.empty"),
            x: PANEL_CENTER_X,
            y: EMPTY_TEXT_Y,
            size: SIZE_VALUE,
            align: Align::Center,
            tone: Tone::Primary,
        });
    }
    for item in layout.item_range() {
        let entry = &model.library[item];
        let ordinal = item - layout.list_page * LIBRARY_PER_PAGE + 1;
        let row = row_rect(LIST_TOP_Y, ordinal, ROW_W);
        let play_rect = UiRect::new(row.x, row.y, row.w - DELETE_W - DELETE_GAP, row.h);
        let delete_rect = UiRect::new(row.x + row.w - DELETE_W, row.y, DELETE_W, row.h);
        let mut play = widget(
            play_rect,
            Label::Text(entry.name.clone()),
            SIZE_VALUE,
            Role::Item(item),
            Activation::Emit(MenuAction::PlayLibrary(entry.id)),
        );
        play.value = Some(Label::Key("menu.library.button.play"));
        play.starts_play = true;
        layout.widgets.push(play);
        layout.widgets.push(widget(
            delete_rect,
            Label::Key("menu.library.button.delete"),
            SIZE_VALUE,
            Role::Delete(item),
            Activation::Emit(MenuAction::DeleteLibrary(entry.id)),
        ));
    }
    finish_list_page(layout);
}

fn page_button(step: PageStep, enabled: bool) -> Widget {
    let (x, key, role) = match step {
        PageStep::Prev => (
            centered_x(BACK_W) - PAGE_BUTTON_GAP - PAGE_BUTTON_W,
            "menu.common.button.prev_page",
            Role::PagePrev,
        ),
        PageStep::Next => (
            centered_x(BACK_W) + BACK_W + PAGE_BUTTON_GAP,
            "menu.common.button.next_page",
            Role::PageNext,
        ),
    };
    let mut w = widget(
        UiRect::new(x, BACK_Y, PAGE_BUTTON_W, BACK_H),
        Label::Key(key),
        SIZE_BUTTON,
        role,
        Activation::Page(step),
    );
    w.enabled = enabled;
    w
}

/// Adds the Back button and, on a multi-page list, the paging buttons (last, pointer-only) and
/// the page indicator.
fn finish_list_page(layout: &mut PageLayout) {
    layout.widgets.push(back_widget());
    if layout.page_count > 1 {
        layout
            .widgets
            .push(page_button(PageStep::Prev, layout.list_page > 0));
        layout.widgets.push(page_button(
            PageStep::Next,
            layout.list_page + 1 < layout.page_count,
        ));
        let indicator = page_indicator(layout.list_page, layout.page_count);
        layout.texts.push(indicator);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::menu::model::{LibraryEntry, SectorRow, WorldRow};

    fn model_with_world(sectors: usize) -> MenuModel {
        let mut model = MenuModel::empty();
        model.worlds.push(WorldRow {
            name: "WORLD".to_owned(),
            unlocked: true,
            cleared: 0,
            total: sectors,
            sectors: (0..sectors)
                .map(|i| SectorRow {
                    index: i,
                    label: format!("S{i}"),
                    unlocked: true,
                })
                .collect(),
            focus_offset: 0,
        });
        model
    }

    #[test]
    fn page_count_is_at_least_one_and_rounds_up() {
        assert_eq!(page_count(0, 10), 1);
        assert_eq!(page_count(10, 10), 1);
        assert_eq!(page_count(11, 10), 2);
        assert_eq!(page_count(5, 0), 1);
    }

    #[test]
    fn visible_range_clamps() {
        assert_eq!(visible_range(25, 10, 2), 20..25);
        assert_eq!(visible_range(25, 10, 9), 25..25);
        assert_eq!(visible_range(0, 10, 0), 0..0);
    }

    #[test]
    fn rect_contains_is_half_open() {
        let r = UiRect::new(10.0, 10.0, 20.0, 20.0);
        assert!(r.contains(10.0, 10.0));
        assert!(r.contains(29.9, 29.9));
        assert!(!r.contains(30.0, 15.0));
        assert!(!r.contains(9.9, 15.0));
    }

    #[test]
    fn hit_test_finds_the_widget_under_the_point() {
        let layout = page_layout(PageId::Pause, &MenuModel::empty(), 0);
        for (i, w) in layout.widgets.iter().enumerate() {
            let cx = w.rect.x + w.rect.w * 0.5;
            let cy = w.rect.y + w.rect.h * 0.5;
            assert_eq!(layout.hit(cx, cy), Some(i));
        }
        assert_eq!(layout.hit(0.0, 0.0), None);
    }

    #[test]
    fn widgets_never_overlap_and_stay_in_the_panel() {
        let mut model = model_with_world(25);
        for i in 0..20 {
            model.library.push(LibraryEntry {
                id: i,
                name: format!("L{i}"),
            });
        }
        model.satisfaction_levels = (0..12).map(|i| format!("Q{i}")).collect();
        model.quit_supported = true;
        let panel = UiRect::new(PANEL_X, PANEL_Y, PANEL_W, PANEL_H);
        let pages = [
            PageId::Home,
            PageId::Chapters,
            PageId::Sectors(0),
            PageId::Options,
            PageId::Controls,
            PageId::Pause,
            PageId::Satisfaction,
            PageId::Library,
        ];
        for page in pages {
            for list_page in 0..3 {
                let layout = page_layout(page, &model, list_page);
                for (i, a) in layout.widgets.iter().enumerate() {
                    assert!(a.rect.x >= panel.x && a.rect.x + a.rect.w <= panel.x + panel.w);
                    assert!(a.rect.y >= panel.y && a.rect.y + a.rect.h <= panel.y + panel.h);
                    for b in layout.widgets.iter().skip(i + 1) {
                        let apart = a.rect.x + a.rect.w <= b.rect.x
                            || b.rect.x + b.rect.w <= a.rect.x
                            || a.rect.y + a.rect.h <= b.rect.y
                            || b.rect.y + b.rect.h <= a.rect.y;
                        assert!(apart, "overlap on {page:?} page {list_page}");
                    }
                }
            }
        }
    }

    #[test]
    fn every_page_except_home_and_pause_has_a_back_widget() {
        let model = model_with_world(3);
        for page in [
            PageId::Chapters,
            PageId::Sectors(0),
            PageId::Options,
            PageId::Controls,
            PageId::Satisfaction,
            PageId::Library,
        ] {
            let layout = page_layout(page, &model, 0);
            assert!(
                layout.widgets.iter().any(|w| w.role == Role::Back),
                "{page:?}"
            );
        }
        for page in [PageId::Home, PageId::Pause] {
            let layout = page_layout(page, &model, 0);
            assert!(layout.widgets.iter().all(|w| w.role != Role::Back));
        }
    }

    #[test]
    fn quit_button_only_when_supported() {
        let mut model = MenuModel::empty();
        let without = page_layout(PageId::Home, &model, 0).widgets.len();
        model.quit_supported = true;
        let with = page_layout(PageId::Home, &model, 0).widgets.len();
        assert_eq!(with, without + 1);
    }

    #[test]
    fn locked_rows_are_disabled() {
        let mut model = model_with_world(3);
        model.worlds[0].sectors[1].unlocked = false;
        model.worlds[0].unlocked = false;
        let sectors = page_layout(PageId::Sectors(0), &model, 0);
        assert!(!sectors.widgets[1].enabled);
        assert!(!sectors.widgets[1].starts_play);
        assert!(sectors.widgets[0].enabled);
        let chapters = page_layout(PageId::Chapters, &model, 0);
        assert!(!chapters.widgets[0].enabled);
    }

    #[test]
    fn sectors_page_shows_one_page_of_rows() {
        let model = model_with_world(25);
        let layout = page_layout(PageId::Sectors(0), &model, 2);
        assert_eq!(layout.page_count, 3);
        assert_eq!(layout.list_page, 2);
        // 5 rows on the last page, Back, then the previous / next paging buttons.
        assert_eq!(layout.widgets.len(), 8);
        assert_eq!(layout.widgets[0].role, Role::Item(20));
        assert_eq!(layout.widgets[5].role, Role::Back);
        assert_eq!(layout.widgets[6].role, Role::PagePrev);
        assert_eq!(layout.widgets[7].role, Role::PageNext);
        assert_eq!(layout.focusable_count(), 6);
        let clamped = page_layout(PageId::Sectors(0), &model, 99);
        assert_eq!(clamped.list_page, 2);
    }

    #[test]
    fn starts_play_flags_follow_the_spec() {
        let mut model = model_with_world(2);
        model.satisfaction_levels = vec!["A".to_owned()];
        model.library.push(LibraryEntry {
            id: 7,
            name: "X".to_owned(),
        });
        let pause = page_layout(PageId::Pause, &model, 0);
        let flags: Vec<bool> = pause.widgets.iter().map(|w| w.starts_play).collect();
        assert_eq!(flags, vec![true, true, false, false]);
        assert!(page_layout(PageId::Satisfaction, &model, 0).widgets[0].starts_play);
        let library = page_layout(PageId::Library, &model, 0);
        assert!(!library.widgets[0].starts_play); // New
        assert!(library.widgets[1].starts_play); // Play
        assert!(!library.widgets[2].starts_play); // Delete
        assert!(page_layout(PageId::Sectors(0), &model, 0).widgets[0].starts_play);
        assert!(page_layout(PageId::Home, &model, 0)
            .widgets
            .iter()
            .all(|w| !w.starts_play));
    }

    #[test]
    fn every_font_size_in_use_is_preloaded() {
        let mut model = model_with_world(12);
        model.library.push(LibraryEntry {
            id: 1,
            name: "X".to_owned(),
        });
        model.satisfaction_levels = vec!["A".to_owned(); 12];
        model.quit_supported = true;
        for page in [
            PageId::Home,
            PageId::Chapters,
            PageId::Sectors(0),
            PageId::Options,
            PageId::Controls,
            PageId::Pause,
            PageId::Satisfaction,
            PageId::Library,
        ] {
            for list_page in 0..2 {
                let layout = page_layout(page, &model, list_page);
                for t in &layout.texts {
                    // Only the Home title uses the title size, preloaded for its own characters.
                    let allowed = MENU_FONT_SIZES.contains(&t.size)
                        || (page == PageId::Home && t.size == SIZE_HOME_TITLE);
                    assert!(allowed, "{page:?} text {}", t.size);
                }
                for w in &layout.widgets {
                    assert!(
                        MENU_FONT_SIZES.contains(&w.size),
                        "{page:?} widget {}",
                        w.size
                    );
                }
            }
        }
        assert!(!MENU_FONT_SIZES.contains(&SIZE_HOME_TITLE));
        for size in [
            SIZE_PAGE_TITLE,
            SIZE_SUBTITLE,
            SIZE_BUTTON,
            SIZE_VALUE,
            SIZE_SMALL,
            SIZE_CONTROLS,
        ] {
            assert!(MENU_FONT_SIZES.contains(&size));
        }
    }

    #[test]
    fn fit_to_sizes_only_returns_menu_sizes() {
        let measure = |size: u16| size as f32 * 10.0;
        for limit in [1.0, 215.0, 250.0, 301.0, 1000.0] {
            for preferred in [21, 22, 25, 30, 48, 96] {
                let size = fit_to_sizes(measure, preferred, limit);
                assert!(MENU_FONT_SIZES.contains(&size) || size == SIZE_HOME_TITLE);
                assert!(size <= preferred.max(MENU_FONT_SIZES[0]));
            }
        }
        assert_eq!(fit_to_sizes(measure, 30, 1000.0), 30);
        // The title size is reachable only when it is asked for.
        assert_eq!(
            fit_to_sizes(measure, SIZE_HOME_TITLE, 10_000.0),
            SIZE_HOME_TITLE
        );
        assert_eq!(fit_to_sizes(measure, 60, 10_000.0), 48);
        assert_eq!(fit_to_sizes(measure, 30, 250.0), 24);
        assert_eq!(fit_to_sizes(measure, 30, 1.0), 21);
    }

    #[test]
    fn paged_lists_get_pointer_only_paging_buttons() {
        let mut model = model_with_world(25);
        model.satisfaction_levels = (0..12).map(|i| format!("Q{i}")).collect();
        model.library = (0..12)
            .map(|id| LibraryEntry {
                id,
                name: format!("L{id}"),
            })
            .collect();
        let template = model.worlds[0].clone();
        model.worlds.extend((0..9).map(|_| template.clone()));
        for page in [
            PageId::Chapters,
            PageId::Sectors(0),
            PageId::Satisfaction,
            PageId::Library,
        ] {
            let layout = page_layout(page, &model, 0);
            assert!(layout.page_count > 1, "{page:?}");
            let n = layout.widgets.len();
            assert_eq!(layout.widgets[n - 2].role, Role::PagePrev, "{page:?}");
            assert_eq!(layout.widgets[n - 1].role, Role::PageNext, "{page:?}");
            assert_eq!(layout.focusable_count(), n - 2, "{page:?}");
            // On the first page there is nothing before; on the last there is nothing after.
            assert!(!layout.widgets[n - 2].enabled);
            assert!(layout.widgets[n - 1].enabled);
            let last = page_layout(page, &model, layout.page_count - 1);
            let m = last.widgets.len();
            assert!(last.widgets[m - 2].enabled);
            assert!(!last.widgets[m - 1].enabled);
            // Each button is hit at its own centre and nowhere else.
            for index in [m - 2, m - 1] {
                let r = last.widgets[index].rect;
                assert_eq!(last.hit(r.x + r.w * 0.5, r.y + r.h * 0.5), Some(index));
            }
        }
    }

    #[test]
    fn short_lists_and_other_pages_have_no_paging_buttons() {
        let mut model = model_with_world(3);
        model.satisfaction_levels = vec!["A".to_owned()];
        for page in [
            PageId::Home,
            PageId::Options,
            PageId::Controls,
            PageId::Pause,
            PageId::Chapters,
            PageId::Sectors(0),
            PageId::Satisfaction,
            PageId::Library,
        ] {
            let layout = page_layout(page, &model, 0);
            assert!(
                layout.widgets.iter().all(|w| !w.role.is_page_button()),
                "{page:?}"
            );
            assert_eq!(layout.focusable_count(), layout.widgets.len());
        }
    }

    #[test]
    fn the_home_page_shows_the_high_score_with_leading_zeros() {
        let mut model = MenuModel::empty();
        model.high_score = 4200;
        let layout = page_layout(PageId::Home, &model, 0);
        let line = layout
            .texts
            .iter()
            .find(|t| t.label.key() == Some("menu.best_score"))
            .expect("high score line");
        assert_eq!(
            line.label,
            Label::Template {
                key: "menu.best_score",
                args: vec![("score", "0004200".to_owned())],
            }
        );
        assert!(MENU_FONT_SIZES.contains(&line.size));
    }

    #[test]
    fn substitute_replaces_named_placeholders() {
        let out = substitute(
            "{done}/{total}",
            &[("done", "3".to_owned()), ("total", "10".to_owned())],
        );
        assert_eq!(out, "3/10");
    }
}
