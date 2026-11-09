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

//! Pure menu state machine: pages, focus, back stack, input handling.

use super::model::{MenuAction, MenuInput, MenuModel};
use super::widgets::{page_layout, Activation, PageId, PageLayout, Role, SECTORS_PER_PAGE};

/// A page left behind by navigation, restored by Back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Frame {
    page: PageId,
    focus: usize,
    list_page: usize,
}

/// A pointer press remembered until release: page, list page, widget index.
type PressTarget = (PageId, usize, usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Jump {
    Prev,
    Next,
    First,
    Last,
}

#[derive(Debug, Clone)]
pub struct Menu {
    page: PageId,
    focus: usize,
    list_page: usize,
    stack: Vec<Frame>,
    last_pointer: Option<(f32, f32)>,
    pressed_on: Option<PressTarget>,
    renderer_dirty: bool,
}

impl Default for Menu {
    fn default() -> Self {
        Self::new()
    }
}

impl Menu {
    /// A menu on the Home page.
    pub fn new() -> Self {
        Self {
            page: PageId::Home,
            focus: 0,
            list_page: 0,
            stack: Vec::new(),
            last_pointer: None,
            pressed_on: None,
            renderer_dirty: false,
        }
    }

    /// Shows the Home page with an empty back stack.
    pub fn open_home(&mut self) {
        *self = Self::new();
    }

    /// Shows the Pause page with an empty back stack (Escape does nothing there).
    pub fn open_pause(&mut self) {
        *self = Self::new();
        self.page = PageId::Pause;
    }

    pub fn page(&self) -> PageId {
        self.page
    }

    pub fn focus(&self) -> usize {
        self.focus
    }

    pub fn list_page(&self) -> usize {
        self.list_page
    }

    /// True when a Back action would leave the current page.
    pub fn can_go_back(&self) -> bool {
        !self.stack.is_empty()
    }

    /// True while a previewed renderer change has not been persisted yet.
    pub fn renderer_dirty(&self) -> bool {
        self.renderer_dirty
    }

    /// Layout of the current page for `model`.
    pub fn layout(&self, model: &MenuModel) -> PageLayout {
        page_layout(self.page, model, self.list_page)
    }

    /// True when the focused widget starts play when activated.
    pub fn focused_starts_play(&self, model: &MenuModel) -> bool {
        let layout = self.layout(model);
        layout
            .widgets
            .get(self.focus)
            .is_some_and(|w| w.enabled && w.starts_play)
    }

    /// True when the widget under `pointer` starts play when activated.
    pub fn starts_play_at(&self, model: &MenuModel, pointer: (f32, f32)) -> bool {
        let layout = self.layout(model);
        layout
            .hit(pointer.0, pointer.1)
            .and_then(|i| layout.widgets.get(i))
            .is_some_and(|w| w.enabled && w.starts_play)
    }

    /// Advances the menu by one frame and returns the action the app must perform.
    pub fn update(&mut self, input: &MenuInput, model: &MenuModel) -> MenuAction {
        let layout = self.layout(model);
        self.list_page = layout.list_page;
        self.clamp_focus(&layout);

        if input.pointer != self.last_pointer {
            if let Some((x, y)) = input.pointer {
                if let Some(i) = layout.hit(x, y) {
                    self.focus = i;
                }
            }
            self.last_pointer = input.pointer;
        }
        if input.pointer_pressed {
            self.pressed_on = input
                .pointer
                .and_then(|(x, y)| layout.hit(x, y))
                .map(|i| (self.page, self.list_page, i));
        }
        if input.pointer_released {
            let started = self.pressed_on.take();
            let target = input.pointer.and_then(|(x, y)| layout.hit(x, y));
            if let (Some(started), Some(target)) = (started, target) {
                if started == (self.page, self.list_page, target) {
                    self.focus = target;
                    return self.activate(&layout, target, model);
                }
            }
            return MenuAction::None;
        }

        if input.escape {
            return self.go_back();
        }
        if input.enter {
            return self.activate(&layout, self.focus, model);
        }
        if input.up {
            self.move_vertical(model, -1);
        } else if input.down {
            self.move_vertical(model, 1);
        } else if input.left || input.right {
            return self.lateral(&layout, input.right, model);
        } else if input.page_up {
            self.jump(model, Jump::Prev);
        } else if input.page_down {
            self.jump(model, Jump::Next);
        } else if input.home {
            self.jump(model, Jump::First);
        } else if input.end {
            self.jump(model, Jump::Last);
        }
        MenuAction::None
    }

    fn clamp_focus(&mut self, layout: &PageLayout) {
        self.focus = self.focus.min(layout.widgets.len().saturating_sub(1));
    }

    fn activate(&mut self, layout: &PageLayout, index: usize, model: &MenuModel) -> MenuAction {
        let Some(widget) = layout.widgets.get(index) else {
            return MenuAction::None;
        };
        if !widget.enabled {
            return MenuAction::None;
        }
        match widget.activation {
            Activation::Navigate(page) => self.navigate(page, model),
            Activation::Back => self.go_back(),
            Activation::Emit(action) => {
                if action == MenuAction::CycleRenderer {
                    self.renderer_dirty = true;
                }
                action
            }
        }
    }

    /// Action owed for leaving the current page (persisting a previewed renderer).
    fn leave_action(&mut self) -> MenuAction {
        if self.page == PageId::Options && self.renderer_dirty {
            self.renderer_dirty = false;
            MenuAction::PersistRenderer
        } else {
            MenuAction::None
        }
    }

    fn navigate(&mut self, page: PageId, model: &MenuModel) -> MenuAction {
        let action = self.leave_action();
        self.stack.push(Frame {
            page: self.page,
            focus: self.focus,
            list_page: self.list_page,
        });
        self.page = page;
        self.pressed_on = None;
        self.enter_page(model);
        action
    }

    fn go_back(&mut self) -> MenuAction {
        let Some(frame) = self.stack.pop() else {
            return MenuAction::None;
        };
        let action = self.leave_action();
        self.page = frame.page;
        self.focus = frame.focus;
        self.list_page = frame.list_page;
        self.pressed_on = None;
        action
    }

    /// Sets the initial list page and focus of the page just entered.
    fn enter_page(&mut self, model: &MenuModel) {
        self.list_page = 0;
        self.focus = 0;
        if let PageId::Sectors(world) = self.page {
            let Some(row) = model.worlds.get(world) else {
                return;
            };
            if row.sectors.is_empty() {
                return;
            }
            let offset = row.focus_offset.min(row.sectors.len() - 1);
            self.list_page = offset / SECTORS_PER_PAGE;
            let layout = self.layout(model);
            self.focus = layout.widget_for_item(offset).unwrap_or(0);
            return;
        }
        let layout = self.layout(model);
        self.focus = layout.widgets.iter().position(|w| w.enabled).unwrap_or(0);
    }

    fn crosses_pages(&self) -> bool {
        matches!(
            self.page,
            PageId::Chapters | PageId::Sectors(_) | PageId::Satisfaction
        )
    }

    fn move_vertical(&mut self, model: &MenuModel, dir: i64) {
        let layout = self.layout(model);
        let count = layout.widgets.len();
        if count == 0 {
            return;
        }
        self.clamp_focus(&layout);
        if self.crosses_pages() {
            if let Role::Item(abs) = layout.widgets[self.focus].role {
                let first = layout.list_page * layout.page_size;
                if dir > 0 && abs + 1 < layout.item_count && abs + 1 == first + layout.page_size {
                    self.list_page += 1;
                    self.focus = self.layout(model).widget_for_item(abs + 1).unwrap_or(0);
                    return;
                }
                if dir < 0 && abs > 0 && abs == first {
                    self.list_page -= 1;
                    self.focus = self.layout(model).widget_for_item(abs - 1).unwrap_or(0);
                    return;
                }
            }
        }
        self.focus = (self.focus as i64 + dir).rem_euclid(count as i64) as usize;
    }

    fn lateral(&mut self, layout: &PageLayout, right: bool, model: &MenuModel) -> MenuAction {
        let Some(widget) = layout.widgets.get(self.focus) else {
            return MenuAction::None;
        };
        if widget.adjust {
            return self.activate(layout, self.focus, model);
        }
        if self.page == PageId::Library {
            match (widget.role, right) {
                (Role::Item(row), true) => {
                    if layout.widgets.get(self.focus + 1).map(|w| w.role) == Some(Role::Delete(row))
                    {
                        self.focus += 1;
                    }
                }
                (Role::Delete(_), false) => self.focus = self.focus.saturating_sub(1),
                _ => {}
            }
        }
        MenuAction::None
    }

    fn jump(&mut self, model: &MenuModel, jump: Jump) {
        let layout = self.layout(model);
        if layout.page_count <= 1 {
            return;
        }
        self.clamp_focus(&layout);
        let ordinal = match layout.widgets[self.focus].role {
            Role::Item(abs) | Role::Delete(abs) => abs - layout.list_page * layout.page_size,
            _ => 0,
        };
        self.list_page = match jump {
            Jump::Prev => layout.list_page.saturating_sub(1),
            Jump::Next => (layout.list_page + 1).min(layout.page_count - 1),
            Jump::First => 0,
            Jump::Last => layout.page_count - 1,
        };
        let moved = self.layout(model);
        let range = moved.item_range();
        if range.is_empty() {
            return;
        }
        let abs = match jump {
            Jump::First => range.start,
            Jump::Last => range.end - 1,
            Jump::Prev | Jump::Next => (range.start + ordinal).min(range.end - 1),
        };
        self.focus = moved.widget_for_item(abs).unwrap_or(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::menu::model::{LibraryEntry, SectorRow, WorldRow};
    use kinetix_core::i18n::Language;

    fn world(name: &str, unlocked: bool, sectors: usize, first: usize) -> WorldRow {
        WorldRow {
            name: name.to_owned(),
            unlocked,
            cleared: 0,
            total: sectors,
            sectors: (0..sectors)
                .map(|i| SectorRow {
                    index: first + i,
                    label: format!("S{}", first + i),
                    unlocked: true,
                })
                .collect(),
            focus_offset: 0,
        }
    }

    fn model() -> MenuModel {
        let mut m = MenuModel::empty();
        m.worlds = vec![world("W1", true, 25, 0), world("W2", false, 5, 25)];
        m.satisfaction_levels = vec!["A".to_owned(), "B".to_owned()];
        m.library = vec![
            LibraryEntry {
                id: 11,
                name: "one".to_owned(),
            },
            LibraryEntry {
                id: 12,
                name: "two".to_owned(),
            },
        ];
        m.library_max = 50;
        m.quit_supported = true;
        m
    }

    fn input() -> MenuInput {
        MenuInput::default()
    }

    fn enter() -> MenuInput {
        MenuInput {
            enter: true,
            ..input()
        }
    }

    fn escape() -> MenuInput {
        MenuInput {
            escape: true,
            ..input()
        }
    }

    fn down() -> MenuInput {
        MenuInput {
            down: true,
            ..input()
        }
    }

    fn up() -> MenuInput {
        MenuInput {
            up: true,
            ..input()
        }
    }

    fn center(menu: &Menu, m: &MenuModel, index: usize) -> (f32, f32) {
        let r = menu.layout(m).widgets[index].rect;
        (r.x + r.w * 0.5, r.y + r.h * 0.5)
    }

    #[test]
    fn home_navigates_to_pages_and_escape_returns() {
        let m = model();
        let mut menu = Menu::new();
        assert_eq!(menu.page(), PageId::Home);
        assert_eq!(menu.update(&enter(), &m), MenuAction::None);
        assert_eq!(menu.page(), PageId::Chapters);
        assert!(menu.can_go_back());
        assert_eq!(menu.update(&escape(), &m), MenuAction::None);
        assert_eq!(menu.page(), PageId::Home);
        assert!(!menu.can_go_back());
    }

    #[test]
    fn escape_on_home_does_nothing() {
        let m = model();
        let mut menu = Menu::new();
        assert_eq!(menu.update(&escape(), &m), MenuAction::None);
        assert_eq!(menu.page(), PageId::Home);
    }

    #[test]
    fn escape_on_pause_does_not_resume() {
        let m = model();
        let mut menu = Menu::new();
        menu.open_pause();
        assert_eq!(menu.update(&escape(), &m), MenuAction::None);
        assert_eq!(menu.page(), PageId::Pause);
        assert_eq!(menu.update(&enter(), &m), MenuAction::Resume);
    }

    #[test]
    fn pause_options_returns_to_pause() {
        let m = model();
        let mut menu = Menu::new();
        menu.open_pause();
        menu.update(&down(), &m);
        menu.update(&down(), &m);
        assert_eq!(menu.update(&enter(), &m), MenuAction::None);
        assert_eq!(menu.page(), PageId::Options);
        menu.update(&escape(), &m);
        assert_eq!(menu.page(), PageId::Pause);
        assert_eq!(menu.focus(), 2);
    }

    #[test]
    fn pause_actions() {
        let m = model();
        let mut menu = Menu::new();
        menu.open_pause();
        menu.update(&down(), &m);
        assert_eq!(menu.update(&enter(), &m), MenuAction::Restart);
        menu.update(&down(), &m);
        menu.update(&down(), &m);
        assert_eq!(menu.update(&enter(), &m), MenuAction::QuitToTitle);
    }

    #[test]
    fn focus_wraps_both_ways() {
        let m = model();
        let mut menu = Menu::new();
        let count = menu.layout(&m).widgets.len();
        menu.update(&up(), &m);
        assert_eq!(menu.focus(), count - 1);
        menu.update(&down(), &m);
        assert_eq!(menu.focus(), 0);
    }

    #[test]
    fn home_quit_emits_quit_app() {
        let m = model();
        let mut menu = Menu::new();
        menu.update(&up(), &m);
        assert_eq!(menu.update(&enter(), &m), MenuAction::QuitApp);
    }

    #[test]
    fn locked_chapter_is_not_activatable() {
        let m = model();
        let mut menu = Menu::new();
        menu.update(&enter(), &m);
        menu.update(&down(), &m);
        assert_eq!(menu.focus(), 1);
        assert_eq!(menu.update(&enter(), &m), MenuAction::None);
        assert_eq!(menu.page(), PageId::Chapters);
    }

    #[test]
    fn unlocked_chapter_opens_sectors_and_starts_a_sector() {
        let m = model();
        let mut menu = Menu::new();
        menu.update(&enter(), &m);
        assert_eq!(menu.update(&enter(), &m), MenuAction::None);
        assert_eq!(menu.page(), PageId::Sectors(0));
        assert_eq!(menu.update(&enter(), &m), MenuAction::StartSector(0));
    }

    #[test]
    fn locked_sector_is_not_activatable() {
        let mut m = model();
        m.worlds[0].sectors[0].unlocked = false;
        let mut menu = Menu::new();
        menu.update(&enter(), &m);
        menu.update(&enter(), &m);
        assert_eq!(menu.update(&enter(), &m), MenuAction::None);
        assert_eq!(menu.page(), PageId::Sectors(0));
    }

    #[test]
    fn sector_focus_offset_selects_the_right_page() {
        let mut m = model();
        m.worlds[0].focus_offset = 13;
        let mut menu = Menu::new();
        menu.update(&enter(), &m);
        menu.update(&enter(), &m);
        assert_eq!(menu.list_page(), 1);
        assert_eq!(menu.update(&enter(), &m), MenuAction::StartSector(13));
    }

    #[test]
    fn down_crosses_sector_pages_and_up_comes_back() {
        let m = model();
        let mut menu = Menu::new();
        menu.update(&enter(), &m);
        menu.update(&enter(), &m);
        for _ in 0..SECTORS_PER_PAGE {
            menu.update(&down(), &m);
        }
        assert_eq!(menu.list_page(), 1);
        assert_eq!(
            menu.update(&enter(), &m),
            MenuAction::StartSector(SECTORS_PER_PAGE)
        );
        menu.update(&up(), &m);
        assert_eq!(menu.list_page(), 0);
        assert_eq!(
            menu.update(&enter(), &m),
            MenuAction::StartSector(SECTORS_PER_PAGE - 1)
        );
    }

    #[test]
    fn last_sector_down_reaches_back_then_wraps() {
        let m = model();
        let mut menu = Menu::new();
        menu.update(&enter(), &m);
        menu.update(&enter(), &m);
        for _ in 0..25 {
            menu.update(&down(), &m);
        }
        let layout = menu.layout(&m);
        assert_eq!(layout.widgets[menu.focus()].role, Role::Back);
        menu.update(&down(), &m);
        assert_eq!(menu.list_page(), 2);
        assert_eq!(menu.focus(), 0);
    }

    #[test]
    fn page_keys_home_end() {
        let m = model();
        let mut menu = Menu::new();
        menu.update(&enter(), &m);
        menu.update(&enter(), &m);
        let page_down = MenuInput {
            page_down: true,
            ..input()
        };
        menu.update(&page_down, &m);
        assert_eq!(menu.list_page(), 1);
        menu.update(&page_down, &m);
        menu.update(&page_down, &m);
        assert_eq!(menu.list_page(), 2);
        let page_up = MenuInput {
            page_up: true,
            ..input()
        };
        menu.update(&page_up, &m);
        assert_eq!(menu.list_page(), 1);
        let end = MenuInput {
            end: true,
            ..input()
        };
        menu.update(&end, &m);
        assert_eq!(menu.list_page(), 2);
        assert_eq!(menu.update(&enter(), &m), MenuAction::StartSector(24));
        let home = MenuInput {
            home: true,
            ..input()
        };
        menu.update(&home, &m);
        assert_eq!(menu.list_page(), 0);
        assert_eq!(menu.update(&enter(), &m), MenuAction::StartSector(0));
    }

    #[test]
    fn back_restores_previous_focus_and_page() {
        let m = model();
        let mut menu = Menu::new();
        menu.update(&down(), &m);
        menu.update(&enter(), &m);
        assert_eq!(menu.page(), PageId::Satisfaction);
        menu.update(&escape(), &m);
        assert_eq!(menu.page(), PageId::Home);
        assert_eq!(menu.focus(), 1);
    }

    #[test]
    fn satisfaction_starts_levels() {
        let m = model();
        let mut menu = Menu::new();
        menu.update(&down(), &m);
        menu.update(&enter(), &m);
        menu.update(&down(), &m);
        assert_eq!(menu.update(&enter(), &m), MenuAction::StartSatisfaction(1));
        assert!(menu.focused_starts_play(&m));
    }

    #[test]
    fn options_toggle_and_language() {
        let mut m = model();
        m.language = Language::Fr;
        let mut menu = Menu::new();
        for _ in 0..3 {
            menu.update(&down(), &m);
        }
        menu.update(&enter(), &m);
        assert_eq!(menu.page(), PageId::Options);
        assert_eq!(
            menu.update(&enter(), &m),
            MenuAction::SetLanguage(Language::En)
        );
        menu.update(&down(), &m);
        menu.update(&down(), &m);
        let right = MenuInput {
            right: true,
            ..input()
        };
        assert_eq!(menu.update(&right, &m), MenuAction::ToggleSfx);
        menu.update(&down(), &m);
        let left = MenuInput {
            left: true,
            ..input()
        };
        assert_eq!(menu.update(&left, &m), MenuAction::ToggleMusic);
    }

    #[test]
    fn renderer_is_previewed_then_persisted_when_leaving() {
        let m = model();
        let mut menu = Menu::new();
        for _ in 0..3 {
            menu.update(&down(), &m);
        }
        menu.update(&enter(), &m);
        menu.update(&down(), &m);
        assert_eq!(menu.update(&enter(), &m), MenuAction::CycleRenderer);
        assert!(menu.renderer_dirty());
        assert_eq!(menu.update(&escape(), &m), MenuAction::PersistRenderer);
        assert!(!menu.renderer_dirty());
        assert_eq!(menu.page(), PageId::Home);
        assert_eq!(menu.update(&escape(), &m), MenuAction::None);
    }

    #[test]
    fn renderer_is_persisted_when_opening_controls() {
        let m = model();
        let mut menu = Menu::new();
        for _ in 0..3 {
            menu.update(&down(), &m);
        }
        menu.update(&enter(), &m);
        menu.update(&down(), &m);
        menu.update(&enter(), &m);
        let layout = menu.layout(&m);
        let controls = layout.widgets.len() - 2;
        while menu.focus() != controls {
            menu.update(&down(), &m);
        }
        assert_eq!(menu.update(&enter(), &m), MenuAction::PersistRenderer);
        assert_eq!(menu.page(), PageId::Controls);
        menu.update(&escape(), &m);
        assert_eq!(menu.page(), PageId::Options);
    }

    #[test]
    fn library_flow() {
        let m = model();
        let mut menu = Menu::new();
        menu.update(&down(), &m);
        menu.update(&down(), &m);
        menu.update(&enter(), &m);
        assert_eq!(menu.page(), PageId::Library);
        assert_eq!(menu.update(&enter(), &m), MenuAction::OpenEditor);
        menu.update(&down(), &m);
        assert_eq!(menu.update(&enter(), &m), MenuAction::PlayLibrary(11));
        let right = MenuInput {
            right: true,
            ..input()
        };
        menu.update(&right, &m);
        assert_eq!(menu.update(&enter(), &m), MenuAction::DeleteLibrary(11));
        let left = MenuInput {
            left: true,
            ..input()
        };
        menu.update(&left, &m);
        assert_eq!(menu.update(&enter(), &m), MenuAction::PlayLibrary(11));
        menu.update(&left, &m);
        assert_eq!(menu.update(&enter(), &m), MenuAction::PlayLibrary(11));
    }

    #[test]
    fn empty_library_only_offers_new_and_back() {
        let mut m = model();
        m.library.clear();
        let mut menu = Menu::new();
        menu.update(&down(), &m);
        menu.update(&down(), &m);
        menu.update(&enter(), &m);
        assert_eq!(menu.layout(&m).widgets.len(), 2);
        menu.update(&down(), &m);
        assert_eq!(menu.update(&enter(), &m), MenuAction::None);
        assert_eq!(menu.page(), PageId::Home);
    }

    #[test]
    fn pointer_click_activates_and_release_elsewhere_cancels() {
        let m = model();
        let mut menu = Menu::new();
        let target = center(&menu, &m, 1);
        let press = MenuInput {
            pointer: Some(target),
            pointer_pressed: true,
            ..input()
        };
        menu.update(&press, &m);
        assert_eq!(menu.focus(), 1);
        let elsewhere = MenuInput {
            pointer: Some(center(&menu, &m, 0)),
            pointer_released: true,
            ..input()
        };
        assert_eq!(menu.update(&elsewhere, &m), MenuAction::None);
        assert_eq!(menu.page(), PageId::Home);

        menu.update(&press, &m);
        let release = MenuInput {
            pointer: Some(target),
            pointer_released: true,
            ..input()
        };
        assert_eq!(menu.update(&release, &m), MenuAction::None);
        assert_eq!(menu.page(), PageId::Satisfaction);
    }

    #[test]
    fn pointer_release_without_position_cancels() {
        let m = model();
        let mut menu = Menu::new();
        let press = MenuInput {
            pointer: Some(center(&menu, &m, 0)),
            pointer_pressed: true,
            ..input()
        };
        menu.update(&press, &m);
        let release = MenuInput {
            pointer_released: true,
            ..input()
        };
        menu.update(&release, &m);
        assert_eq!(menu.page(), PageId::Home);
    }

    #[test]
    fn touch_tap_on_back_goes_back() {
        let m = model();
        let mut menu = Menu::new();
        menu.update(&enter(), &m);
        let layout = menu.layout(&m);
        let back = layout.widgets.len() - 1;
        let at = center(&menu, &m, back);
        let tap_down = MenuInput {
            pointer: Some(at),
            pointer_pressed: true,
            ..input()
        };
        let tap_up = MenuInput {
            pointer: Some(at),
            pointer_released: true,
            ..input()
        };
        menu.update(&tap_down, &m);
        menu.update(&tap_up, &m);
        assert_eq!(menu.page(), PageId::Home);
    }

    #[test]
    fn locked_row_click_does_nothing() {
        let m = model();
        let mut menu = Menu::new();
        menu.update(&enter(), &m);
        let at = center(&menu, &m, 1);
        let tap_down = MenuInput {
            pointer: Some(at),
            pointer_pressed: true,
            ..input()
        };
        let tap_up = MenuInput {
            pointer: Some(at),
            pointer_released: true,
            ..input()
        };
        menu.update(&tap_down, &m);
        assert_eq!(menu.update(&tap_up, &m), MenuAction::None);
        assert_eq!(menu.page(), PageId::Chapters);
    }

    #[test]
    fn hover_moves_focus_only_when_the_pointer_moves() {
        let m = model();
        let mut menu = Menu::new();
        let at = center(&menu, &m, 2);
        let hover = MenuInput {
            pointer: Some(at),
            ..input()
        };
        menu.update(&hover, &m);
        assert_eq!(menu.focus(), 2);
        let down_with_same_pointer = MenuInput {
            down: true,
            pointer: Some(at),
            ..input()
        };
        menu.update(&down_with_same_pointer, &m);
        assert_eq!(menu.focus(), 3);
    }

    #[test]
    fn starts_play_queries() {
        let m = model();
        let mut menu = Menu::new();
        menu.open_pause();
        assert!(menu.focused_starts_play(&m));
        let resume = center(&menu, &m, 0);
        let options = center(&menu, &m, 2);
        assert!(menu.starts_play_at(&m, resume));
        assert!(!menu.starts_play_at(&m, options));
        assert!(!menu.starts_play_at(&m, (0.0, 0.0)));
        menu.update(&down(), &m);
        menu.update(&down(), &m);
        assert!(!menu.focused_starts_play(&m));
    }

    #[test]
    fn locked_sector_does_not_start_play() {
        let mut m = model();
        m.worlds[0].sectors[0].unlocked = false;
        let mut menu = Menu::new();
        menu.update(&enter(), &m);
        menu.update(&enter(), &m);
        assert!(!menu.focused_starts_play(&m));
    }

    #[test]
    fn model_shrinking_keeps_focus_valid() {
        let mut m = model();
        let mut menu = Menu::new();
        menu.update(&down(), &m);
        menu.update(&enter(), &m);
        menu.update(&down(), &m);
        m.satisfaction_levels.clear();
        assert_eq!(menu.update(&enter(), &m), MenuAction::None);
        assert_eq!(menu.page(), PageId::Home);
    }
}
