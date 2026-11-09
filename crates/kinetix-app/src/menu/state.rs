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

use super::model::{LibraryId, MenuAction, MenuInput, MenuModel};
use super::widgets::{
    page_layout, Activation, Label, PageId, PageLayout, PageStep, Role, CONFIRM_DELETE_KEY,
    SECTORS_PER_PAGE,
};

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
    /// Library entry whose Delete button was activated once and now asks for confirmation.
    armed_delete: Option<LibraryId>,
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
            armed_delete: None,
        }
    }

    /// Shows the Home page with an empty back stack. `pointer` is where the pointer already is:
    /// only a later move of the pointer changes focus.
    pub fn open_home(&mut self, pointer: Option<(f32, f32)>) {
        *self = Self::new();
        self.last_pointer = pointer;
    }

    /// Shows the Pause page, focused on Resume, with an empty back stack (Escape does nothing
    /// there). `pointer` is where the pointer already is: only a later move changes focus.
    pub fn open_pause(&mut self, pointer: Option<(f32, f32)>) {
        *self = Self::new();
        self.page = PageId::Pause;
        self.last_pointer = pointer;
    }

    /// Shows `page` the way a player gets there from Home: Back returns to Home with the button
    /// that leads to `page` focused. Home and Pause open as their own roots.
    pub fn open_page(&mut self, page: PageId, model: &MenuModel, pointer: Option<(f32, f32)>) {
        match page {
            PageId::Home => self.open_home(pointer),
            PageId::Pause => self.open_pause(pointer),
            _ => {
                self.open_home(pointer);
                let focus = self
                    .layout(model)
                    .widgets
                    .iter()
                    .position(|w| w.activation == Activation::Navigate(page))
                    .unwrap_or(0);
                self.stack.push(Frame {
                    page: PageId::Home,
                    focus,
                    list_page: 0,
                });
                self.page = page;
                self.enter_page(model);
            }
        }
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

    /// Layout of the current page for `model`; the armed library Delete button asks to confirm.
    pub fn layout(&self, model: &MenuModel) -> PageLayout {
        let mut layout = page_layout(self.page, model, self.list_page);
        if let Some(id) = self.armed_delete {
            for widget in &mut layout.widgets {
                if matches!(widget.role, Role::Delete(_))
                    && widget.activation == Activation::Emit(MenuAction::DeleteLibrary(id))
                {
                    widget.label = Label::Key(CONFIRM_DELETE_KEY);
                }
            }
        }
        layout
    }

    /// True while a library Delete button waits for its confirming activation.
    pub fn delete_armed(&self) -> bool {
        self.armed_delete.is_some()
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
        let action = self.step(input, model);
        self.disarm_if_stale(model);
        action
    }

    /// Drops the delete confirmation once focus left the armed button or the page changed.
    fn disarm_if_stale(&mut self, model: &MenuModel) {
        let Some(id) = self.armed_delete else {
            return;
        };
        let still_on_button =
            self.page == PageId::Library && self.focused_delete_id(model) == Some(id);
        if !still_on_button {
            self.armed_delete = None;
        }
    }

    /// Library id of the Delete button that has focus, if focus is on one.
    fn focused_delete_id(&self, model: &MenuModel) -> Option<LibraryId> {
        let layout = page_layout(self.page, model, self.list_page);
        let widget = layout.widgets.get(self.focus)?;
        match (widget.role, widget.activation) {
            (Role::Delete(_), Activation::Emit(MenuAction::DeleteLibrary(id))) => Some(id),
            _ => None,
        }
    }

    fn step(&mut self, input: &MenuInput, model: &MenuModel) -> MenuAction {
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
            let hit = input.pointer.and_then(|(x, y)| layout.hit(x, y));
            self.pressed_on = hit.map(|i| (self.page, self.list_page, i));
            // A widget that starts play acts on the press: the web pointer lock requested by
            // the press gesture only lives a few hundred milliseconds, far less than a click.
            if let Some(i) = hit.filter(|&i| layout.widgets[i].starts_play) {
                self.pressed_on = None;
                self.focus = i;
                return self.activate(&layout, i, model);
            }
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
            Activation::Page(step) => {
                self.turn_page(model, widget.role, step);
                MenuAction::None
            }
            Activation::Emit(MenuAction::DeleteLibrary(id)) => {
                if self.armed_delete == Some(id) {
                    self.armed_delete = None;
                    MenuAction::DeleteLibrary(id)
                } else {
                    self.armed_delete = Some(id);
                    MenuAction::None
                }
            }
            Activation::Emit(action) => {
                if action == MenuAction::CycleRenderer {
                    self.renderer_dirty = true;
                }
                action
            }
        }
    }

    /// Shows the previous or next list page and keeps focus on the paging button.
    fn turn_page(&mut self, model: &MenuModel, button: Role, step: PageStep) {
        self.jump(
            model,
            match step {
                PageStep::Prev => Jump::Prev,
                PageStep::Next => Jump::Next,
            },
        );
        self.focus = self
            .layout(model)
            .widgets
            .iter()
            .position(|w| w.role == button)
            .unwrap_or(0);
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
        self.armed_delete = None;
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
        self.armed_delete = None;
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
            PageId::Chapters | PageId::Sectors(_) | PageId::Satisfaction | PageId::Library
        )
    }

    fn move_vertical(&mut self, model: &MenuModel, dir: i64) {
        let layout = self.layout(model);
        let count = layout.focusable_count();
        if count == 0 {
            return;
        }
        // Focus parked on a pointer-only paging button comes back to the last focusable widget.
        if self.focus >= count {
            self.focus = count - 1;
            return;
        }
        if self.crosses_pages() && self.cross_page(model, &layout, dir) {
            return;
        }
        self.focus = (self.focus as i64 + dir).rem_euclid(count as i64) as usize;
    }

    /// Moves focus onto the neighbouring list page when Up / Down leaves the first or last row of
    /// the current one. Returns false when focus stays on the page.
    fn cross_page(&mut self, model: &MenuModel, layout: &PageLayout, dir: i64) -> bool {
        let first = layout.list_page * layout.page_size;
        let last = first + layout.page_size - 1;
        let library = self.page == PageId::Library;
        let more = |abs: usize| abs == last && abs + 1 < layout.item_count;
        // The page to show and the widget role to focus on it.
        let target = match (dir > 0, layout.widgets[self.focus].role) {
            (true, Role::Item(abs)) if !library && more(abs) => Some((1, Role::Item(abs + 1))),
            (true, Role::Delete(abs)) if more(abs) => Some((1, Role::Item(abs + 1))),
            (false, Role::Item(abs)) if abs > 0 && abs == first => {
                let role = if library {
                    Role::Delete(abs - 1)
                } else {
                    Role::Item(abs - 1)
                };
                Some((-1, role))
            }
            _ => None,
        };
        let Some((delta, role)) = target else {
            return false;
        };
        self.list_page = (self.list_page as i64 + delta) as usize;
        self.focus = self
            .layout(model)
            .widgets
            .iter()
            .position(|w| w.role == role)
            .unwrap_or(0);
        true
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
    use crate::menu::widgets::Label;
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
        menu.open_pause(None);
        assert_eq!(menu.update(&escape(), &m), MenuAction::None);
        assert_eq!(menu.page(), PageId::Pause);
        assert_eq!(menu.update(&enter(), &m), MenuAction::Resume);
    }

    #[test]
    fn pause_options_returns_to_pause() {
        let m = model();
        let mut menu = Menu::new();
        menu.open_pause(None);
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
        menu.open_pause(None);
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
        // Delete asks for a confirmation: the first activation only arms it.
        assert_eq!(menu.update(&enter(), &m), MenuAction::None);
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
        menu.open_pause(None);
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

    fn press(at: (f32, f32)) -> MenuInput {
        MenuInput {
            pointer: Some(at),
            pointer_pressed: true,
            ..input()
        }
    }

    fn release(at: (f32, f32)) -> MenuInput {
        MenuInput {
            pointer: Some(at),
            pointer_released: true,
            ..input()
        }
    }

    fn click(menu: &mut Menu, m: &MenuModel, at: (f32, f32)) -> MenuAction {
        let first = menu.update(&press(at), m);
        if first != MenuAction::None {
            return first;
        }
        menu.update(&release(at), m)
    }

    fn big_library_model(count: usize) -> MenuModel {
        let mut m = model();
        m.library = (0..count)
            .map(|i| LibraryEntry {
                id: 100 + i as u64,
                name: format!("lvl{i}"),
            })
            .collect();
        m
    }

    fn open_library(menu: &mut Menu, m: &MenuModel) {
        menu.open_page(PageId::Library, m, None);
        assert_eq!(menu.page(), PageId::Library);
    }

    fn focused_role(menu: &Menu, m: &MenuModel) -> Role {
        menu.layout(m).widgets[menu.focus()].role
    }

    // ---- pause and home open without a stale pointer stealing focus ----

    #[test]
    fn pause_opens_on_resume_even_with_the_pointer_over_another_button() {
        let m = model();
        let mut menu = Menu::new();
        menu.open_pause(None);
        let over_options = center(&menu, &m, 2);
        menu.open_pause(Some(over_options));
        assert_eq!(menu.focus(), 0);
        // The first frame carries the very same pointer: nothing moved, Resume stays focused.
        let hover = MenuInput {
            pointer: Some(over_options),
            ..input()
        };
        assert_eq!(menu.update(&hover, &m), MenuAction::None);
        assert_eq!(menu.focus(), 0);
        assert_eq!(menu.update(&enter(), &m), MenuAction::Resume);
    }

    #[test]
    fn only_a_real_pointer_move_changes_focus_after_opening() {
        let m = model();
        let mut menu = Menu::new();
        menu.open_pause(None);
        let over_options = center(&menu, &m, 2);
        let over_quit = center(&menu, &m, 3);
        menu.open_pause(Some(over_options));
        let moved = MenuInput {
            pointer: Some(over_quit),
            ..input()
        };
        menu.update(&moved, &m);
        assert_eq!(menu.focus(), 3);
    }

    #[test]
    fn home_opens_with_the_seeded_pointer_too() {
        let m = model();
        let mut menu = Menu::new();
        let over_third = center(&menu, &m, 2);
        menu.open_home(Some(over_third));
        let hover = MenuInput {
            pointer: Some(over_third),
            ..input()
        };
        menu.update(&hover, &m);
        assert_eq!(menu.focus(), 0);
        assert_eq!(menu.page(), PageId::Home);
    }

    // ---- open_page ----

    #[test]
    fn open_page_reaches_a_page_with_back_leading_to_the_focused_home_button() {
        let m = model();
        let mut menu = Menu::new();
        menu.open_page(PageId::Satisfaction, &m, None);
        assert_eq!(menu.page(), PageId::Satisfaction);
        assert!(menu.can_go_back());
        menu.update(&escape(), &m);
        assert_eq!(menu.page(), PageId::Home);
        assert_eq!(menu.focus(), 1, "the Satisfaction button");
        assert!(!menu.can_go_back());

        menu.open_page(PageId::Library, &m, None);
        menu.update(&escape(), &m);
        assert_eq!((menu.page(), menu.focus()), (PageId::Home, 2));
    }

    #[test]
    fn open_page_treats_home_and_pause_as_roots() {
        let m = model();
        let mut menu = Menu::new();
        menu.open_page(PageId::Pause, &m, None);
        assert_eq!(menu.page(), PageId::Pause);
        assert!(!menu.can_go_back());
        menu.open_page(PageId::Library, &m, None);
        menu.open_page(PageId::Home, &m, None);
        assert_eq!(menu.page(), PageId::Home);
        assert!(!menu.can_go_back());
    }

    #[test]
    fn open_page_seeds_the_pointer_and_focuses_the_first_widget_of_the_page() {
        let m = model();
        let mut menu = Menu::new();
        let somewhere = center(&menu, &m, 1);
        menu.open_page(PageId::Chapters, &m, Some(somewhere));
        assert_eq!(menu.focus(), 0);
        let hover = MenuInput {
            pointer: Some(somewhere),
            ..input()
        };
        menu.update(&hover, &m);
        assert_eq!(
            menu.focus(),
            0,
            "the unchanged pointer does not steal focus"
        );
    }

    // ---- press activation of the widgets that start play ----

    #[test]
    fn a_widget_that_starts_play_acts_on_the_press() {
        let m = model();
        let mut menu = Menu::new();
        menu.open_page(PageId::Sectors(0), &m, None);
        let at = center(&menu, &m, 0);
        assert_eq!(menu.update(&press(at), &m), MenuAction::StartSector(0));
        // The release that follows the press does nothing more.
        assert_eq!(menu.update(&release(at), &m), MenuAction::None);
    }

    #[test]
    fn pause_resume_acts_on_the_press_while_options_wait_for_the_release() {
        let m = model();
        let mut menu = Menu::new();
        menu.open_pause(None);
        let resume = center(&menu, &m, 0);
        assert_eq!(menu.update(&press(resume), &m), MenuAction::Resume);

        menu.open_pause(None);
        let options = center(&menu, &m, 2);
        assert_eq!(menu.update(&press(options), &m), MenuAction::None);
        assert_eq!(menu.page(), PageId::Pause, "not before the release");
        assert_eq!(menu.update(&release(options), &m), MenuAction::None);
        assert_eq!(menu.page(), PageId::Options);
    }

    #[test]
    fn a_locked_sector_does_not_act_on_the_press() {
        let mut m = model();
        m.worlds[0].sectors[0].unlocked = false;
        let mut menu = Menu::new();
        menu.open_page(PageId::Sectors(0), &m, None);
        let at = center(&menu, &m, 0);
        assert_eq!(menu.update(&press(at), &m), MenuAction::None);
        assert_eq!(menu.update(&release(at), &m), MenuAction::None);
    }

    #[test]
    fn library_play_acts_on_the_press_and_delete_on_the_release() {
        let m = model();
        let mut menu = Menu::new();
        open_library(&mut menu, &m);
        let layout = menu.layout(&m);
        let play = layout.widget_for_item(0).unwrap();
        let delete = layout.widget_for_delete(0).unwrap();
        assert_eq!(
            menu.update(&press(center(&menu, &m, play)), &m),
            MenuAction::PlayLibrary(11)
        );
        let at = center(&menu, &m, delete);
        assert_eq!(menu.update(&press(at), &m), MenuAction::None);
        // Releasing arms the confirmation, it does not delete.
        assert_eq!(menu.update(&release(at), &m), MenuAction::None);
        assert!(menu.delete_armed());
    }

    // ---- library delete confirmation ----

    fn focus_delete(menu: &mut Menu, m: &MenuModel, row: usize) {
        let layout = menu.layout(m);
        let target = layout.widget_for_delete(row).unwrap();
        while menu.focus() != target {
            menu.update(&down(), m);
        }
    }

    #[test]
    fn delete_needs_a_second_activation_on_the_same_entry() {
        let m = model();
        let mut menu = Menu::new();
        open_library(&mut menu, &m);
        focus_delete(&mut menu, &m, 0);
        assert_eq!(menu.update(&enter(), &m), MenuAction::None);
        assert!(menu.delete_armed());
        let armed = menu.layout(&m).widgets[menu.focus()].label.clone();
        assert_eq!(armed, Label::Key(CONFIRM_DELETE_KEY));
        assert_eq!(menu.update(&enter(), &m), MenuAction::DeleteLibrary(11));
        assert!(!menu.delete_armed());
        assert_eq!(
            menu.layout(&m).widgets[menu.focus()].label,
            Label::Key("menu.library.button.delete")
        );
    }

    #[test]
    fn moving_focus_disarms_the_delete_button() {
        let m = model();
        let mut menu = Menu::new();
        open_library(&mut menu, &m);
        focus_delete(&mut menu, &m, 0);
        menu.update(&enter(), &m);
        assert!(menu.delete_armed());
        menu.update(&down(), &m);
        assert!(!menu.delete_armed());
        // Coming back, one activation arms again; it does not delete.
        menu.update(&up(), &m);
        assert_eq!(menu.update(&enter(), &m), MenuAction::None);
        assert!(menu.delete_armed());
    }

    #[test]
    fn arming_one_entry_never_confirms_another() {
        let m = model();
        let mut menu = Menu::new();
        open_library(&mut menu, &m);
        focus_delete(&mut menu, &m, 0);
        menu.update(&enter(), &m);
        focus_delete(&mut menu, &m, 1);
        assert!(!menu.delete_armed());
        assert_eq!(menu.update(&enter(), &m), MenuAction::None);
        assert_eq!(menu.update(&enter(), &m), MenuAction::DeleteLibrary(12));
    }

    #[test]
    fn leaving_the_page_disarms_the_delete_button() {
        let m = model();
        let mut menu = Menu::new();
        open_library(&mut menu, &m);
        focus_delete(&mut menu, &m, 0);
        menu.update(&enter(), &m);
        assert!(menu.delete_armed());
        menu.update(&escape(), &m);
        assert!(!menu.delete_armed());
        open_library(&mut menu, &m);
        focus_delete(&mut menu, &m, 0);
        assert_eq!(menu.update(&enter(), &m), MenuAction::None);
    }

    #[test]
    fn a_pointer_hover_elsewhere_disarms_the_delete_button() {
        let m = model();
        let mut menu = Menu::new();
        open_library(&mut menu, &m);
        let layout = menu.layout(&m);
        let delete = layout.widget_for_delete(0).unwrap();
        let other = layout.widget_for_item(1).unwrap();
        let at = center(&menu, &m, delete);
        assert_eq!(click(&mut menu, &m, at), MenuAction::None);
        assert!(menu.delete_armed());
        let hover = MenuInput {
            pointer: Some(center(&menu, &m, other)),
            ..input()
        };
        menu.update(&hover, &m);
        assert!(!menu.delete_armed());
    }

    // ---- paging by pointer, wheel and keyboard ----

    #[test]
    fn the_next_and_previous_buttons_turn_list_pages_by_pointer() {
        let m = model();
        let mut menu = Menu::new();
        menu.open_page(PageId::Sectors(0), &m, None);
        assert_eq!(menu.list_page(), 0);
        let next = menu
            .layout(&m)
            .widgets
            .iter()
            .position(|w| w.role == Role::PageNext)
            .unwrap();
        let at = center(&menu, &m, next);
        assert_eq!(click(&mut menu, &m, at), MenuAction::None);
        assert_eq!(menu.list_page(), 1);
        // Focus stays on the paging button so the next click turns another page.
        assert_eq!(focused_role(&menu, &m), Role::PageNext);
        let at = center(&menu, &m, menu.focus());
        click(&mut menu, &m, at);
        assert_eq!(menu.list_page(), 2);
        // On the last page Next is disabled.
        let at = center(&menu, &m, menu.focus());
        click(&mut menu, &m, at);
        assert_eq!(menu.list_page(), 2);

        let prev = menu
            .layout(&m)
            .widgets
            .iter()
            .position(|w| w.role == Role::PagePrev)
            .unwrap();
        let at = center(&menu, &m, prev);
        click(&mut menu, &m, at);
        assert_eq!(menu.list_page(), 1);
        assert_eq!(focused_role(&menu, &m), Role::PagePrev);
    }

    #[test]
    fn paging_buttons_work_on_chapters_satisfaction_and_library_too() {
        let mut m = big_library_model(20);
        m.satisfaction_levels = (0..12).map(|i| format!("Q{i}")).collect();
        let template = m.worlds[0].clone();
        m.worlds.extend((0..9).map(|_| template.clone()));
        for page in [PageId::Chapters, PageId::Satisfaction, PageId::Library] {
            let mut menu = Menu::new();
            menu.open_page(page, &m, None);
            let next = menu
                .layout(&m)
                .widgets
                .iter()
                .position(|w| w.role == Role::PageNext)
                .unwrap_or_else(|| panic!("{page:?} has a next button"));
            let at = center(&menu, &m, next);
            click(&mut menu, &m, at);
            assert_eq!(menu.list_page(), 1, "{page:?}");
        }
    }

    #[test]
    fn a_disabled_paging_button_does_nothing() {
        let m = model();
        let mut menu = Menu::new();
        menu.open_page(PageId::Sectors(0), &m, None);
        let prev = menu
            .layout(&m)
            .widgets
            .iter()
            .position(|w| w.role == Role::PagePrev)
            .unwrap();
        let at = center(&menu, &m, prev);
        click(&mut menu, &m, at);
        assert_eq!(menu.list_page(), 0);
    }

    #[test]
    fn the_wheel_keys_page_the_library_like_the_other_lists() {
        let m = big_library_model(20);
        let mut menu = Menu::new();
        open_library(&mut menu, &m);
        let page_down = MenuInput {
            page_down: true,
            ..input()
        };
        menu.update(&page_down, &m);
        assert_eq!(menu.list_page(), 1);
        let page_up = MenuInput {
            page_up: true,
            ..input()
        };
        menu.update(&page_up, &m);
        assert_eq!(menu.list_page(), 0);
    }

    #[test]
    fn keyboard_focus_skips_the_pointer_only_paging_buttons() {
        let m = model();
        let mut menu = Menu::new();
        menu.open_page(PageId::Sectors(0), &m, None);
        let focusable = menu.layout(&m).focusable_count();
        for _ in 0..(3 * focusable) {
            menu.update(&down(), &m);
            assert!(!focused_role(&menu, &m).is_page_button());
        }
        for _ in 0..(3 * focusable) {
            menu.update(&up(), &m);
            assert!(!focused_role(&menu, &m).is_page_button());
        }
    }

    #[test]
    fn up_from_a_hovered_paging_button_comes_back_to_the_list() {
        let m = model();
        let mut menu = Menu::new();
        menu.open_page(PageId::Sectors(0), &m, None);
        let next = menu
            .layout(&m)
            .widgets
            .iter()
            .position(|w| w.role == Role::PageNext)
            .unwrap();
        let hover = MenuInput {
            pointer: Some(center(&menu, &m, next)),
            ..input()
        };
        menu.update(&hover, &m);
        assert_eq!(focused_role(&menu, &m), Role::PageNext);
        menu.update(&down(), &m);
        assert!(!focused_role(&menu, &m).is_page_button());
    }

    #[test]
    fn down_and_up_cross_library_pages() {
        let m = big_library_model(20);
        let mut menu = Menu::new();
        open_library(&mut menu, &m);
        // New, then rows of (play, delete) pairs: 8 rows on the first page.
        menu.update(&down(), &m);
        for _ in 0..(2 * 8 - 1) {
            menu.update(&down(), &m);
        }
        assert_eq!(focused_role(&menu, &m), Role::Delete(7));
        assert_eq!(menu.list_page(), 0);
        menu.update(&down(), &m);
        assert_eq!(menu.list_page(), 1);
        assert_eq!(focused_role(&menu, &m), Role::Item(8));
        menu.update(&up(), &m);
        assert_eq!(menu.list_page(), 0);
        assert_eq!(focused_role(&menu, &m), Role::Delete(7));
    }

    #[test]
    fn up_from_the_first_library_row_goes_to_new_not_to_a_page() {
        let m = big_library_model(20);
        let mut menu = Menu::new();
        open_library(&mut menu, &m);
        menu.update(&down(), &m);
        assert_eq!(focused_role(&menu, &m), Role::Item(0));
        menu.update(&up(), &m);
        assert_eq!(focused_role(&menu, &m), Role::Extra);
        assert_eq!(menu.list_page(), 0);
    }
}
