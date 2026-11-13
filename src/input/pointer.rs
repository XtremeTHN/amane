use std::path::PathBuf;

use crate::{Button, Cursor, Scroll};

use super::Target;

#[derive(Default)]
pub struct Pointer {
    // the hit areas of the last drawn frame
    targets: Vec<Target>,

    x: f32,
    y: f32,

    // positions in targets, which stay the same across redraws of an unchanged layout
    hovered: Vec<usize>,
    pressed: Option<usize>,
    dragged: Option<usize>,
}

impl Pointer {
    // the hit areas of a new frame replace the last frame's
    pub fn set_targets(&mut self, targets: Vec<Target>) {
        self.targets = targets;
    }

    // the innermost target under the pointer that has the handler asked for
    fn find_topmost(&self, has_handler: fn(&Target) -> bool) -> Option<usize> {
        let mut topmost = None;

        // children are collected after their parents, so the last match is the innermost
        for (index, target) in self.targets.iter().enumerate() {
            if target.contains(self.x, self.y) && has_handler(target) {
                topmost = Some(index);
            }
        }

        topmost
    }

    // hover

    pub fn move_to(&mut self, x: f32, y: f32) -> bool {
        self.x = x;
        self.y = y;

        let mut inside = Vec::new();

        for (index, target) in self.targets.iter().enumerate() {
            if target.handlers.hover.is_some() && target.contains(x, y) {
                inside.push(index);
            }
        }

        self.update_hovered(inside)
    }

    pub fn leave(&mut self) -> bool {
        // a button let go outside the window is not a click
        self.pressed = None;

        // the release will not reach this window, so the drag ends here
        self.dragged = None;

        self.update_hovered(Vec::new())
    }

    // tells the targets the pointer entered or left, and returns whether any were told
    fn update_hovered(&mut self, inside: Vec<usize>) -> bool {
        let mut changed = false;

        for &index in &self.hovered {
            if !inside.contains(&index) {
                changed |= self.notify(index, false);
            }
        }

        for &index in &inside {
            if !self.hovered.contains(&index) {
                changed |= self.notify(index, true);
            }
        }

        self.hovered = inside;

        changed
    }

    fn notify(&self, index: usize, inside: bool) -> bool {
        // a redraw since the pointer entered may have removed this target
        let Some(target) = self.targets.get(index) else {
            return false;
        };

        let Some(hover) = &target.handlers.hover else {
            return false;
        };

        hover(inside);

        true
    }

    // motion

    // tells the innermost target under the pointer where the pointer is on it
    pub fn report_motion(&self) -> bool {
        let Some(index) = self.find_topmost(|target| target.handlers.motion.is_some()) else {
            return false;
        };

        let target = &self.targets[index];

        let Some(motion) = &target.handlers.motion else {
            return false;
        };

        let point = target.point(self.x, self.y);

        motion(point);

        true
    }

    // the look asked for by the innermost target under the pointer
    pub fn cursor(&self) -> Cursor {
        let Some(index) = self.find_topmost(|target| target.handlers.cursor.is_some()) else {
            return Cursor::Default;
        };

        self.targets[index]
            .handlers
            .cursor
            .unwrap_or(Cursor::Default)
    }

    // click

    pub fn press(&mut self, button: Button) -> bool {
        self.pressed = self.find_topmost(|target| {
            target.handlers.click.is_some() || target.handlers.release.is_some()
        });

        let Some(index) = self.find_topmost(|target| target.handlers.press.is_some()) else {
            return false;
        };

        let Some(handler) = &self.targets[index].handlers.press else {
            return false;
        };

        handler(button);
        true
    }

    // release belongs to the pressed target; clicks additionally require releasing over it
    pub fn release(&mut self, button: Button) -> bool {
        let Some(pressed) = self.pressed.take() else {
            return false;
        };

        let Some(target) = self.targets.get(pressed) else {
            return false;
        };

        let mut handled = false;

        if let Some(release) = &target.handlers.release {
            release(button);
            handled = true;
        }

        let released = self.find_topmost(|target| target.handlers.click.is_some());
        if released == Some(pressed)
            && let Some(click) = &target.handlers.click
        {
            click(button);
            handled = true;
        }

        handled
    }

    // drag

    pub fn start_drag(&mut self) -> bool {
        self.dragged = self.find_topmost(|target| target.handlers.drag.is_some());

        self.drag()
    }

    pub fn stop_drag(&mut self) {
        self.dragged = None;
    }

    // keeps reporting to the target pressed on, even after the pointer leaves it
    pub fn drag(&self) -> bool {
        let Some(index) = self.dragged else {
            return false;
        };

        // a redraw since the press may have removed this target
        let Some(target) = self.targets.get(index) else {
            return false;
        };

        let Some(drag) = &target.handlers.drag else {
            return false;
        };

        let point = target.point(self.x, self.y);

        drag(point);

        true
    }

    // scroll

    pub fn scroll(&self, scroll: Scroll) -> bool {
        let Some(topmost) = self.find_topmost(|target| target.handlers.scroll.is_some()) else {
            return false;
        };

        let Some(handler) = &self.targets[topmost].handlers.scroll else {
            return false;
        };

        handler(scroll);

        true
    }

    // drop

    // returns whether a handler ran
    pub fn drop_files(&mut self, x: f32, y: f32, files: Vec<PathBuf>) -> bool {
        self.x = x;
        self.y = y;

        let Some(index) = self.find_topmost(|target| target.handlers.drop.is_some()) else {
            return false;
        };

        let Some(drop) = &self.targets[index].handlers.drop else {
            return false;
        };

        drop(files);

        true
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, rc::Rc};

    use crate::Button;
    use crate::graphics::Area;
    use crate::input::{Handlers, Target};

    use super::Pointer;

    #[test]
    fn press_handler_runs_on_press_not_release() {
        let presses = Rc::new(Cell::new(0));
        let press_state = Rc::clone(&presses);
        let target = Target::new(
            Area::new(0.0, 0.0, 10.0, 10.0),
            Handlers {
                press: Some(Rc::new(move |_| press_state.set(press_state.get() + 1))),
                ..Default::default()
            },
        );

        let mut pointer = Pointer::default();
        pointer.set_targets(vec![target]);
        pointer.move_to(5.0, 5.0);

        assert!(pointer.press(Button::Left));
        assert_eq!(presses.get(), 1);
        assert!(!pointer.release(Button::Left));
        assert_eq!(presses.get(), 1);
    }

    #[test]
    fn release_handler_runs_outside_target_without_counting_a_click() {
        let released = Rc::new(Cell::new(None));
        let clicked = Rc::new(Cell::new(false));
        let release_state = Rc::clone(&released);
        let click_state = Rc::clone(&clicked);
        let target = Target::new(
            Area::new(0.0, 0.0, 10.0, 10.0),
            Handlers {
                click: Some(Rc::new(move |_| click_state.set(true))),
                release: Some(Rc::new(move |button| release_state.set(Some(button)))),
                ..Default::default()
            },
        );

        let mut pointer = Pointer::default();
        pointer.set_targets(vec![target]);
        pointer.move_to(5.0, 5.0);
        pointer.press(Button::Left);
        pointer.move_to(15.0, 5.0);

        assert!(pointer.release(Button::Left));
        assert_eq!(released.get(), Some(Button::Left));
        assert!(!clicked.get());
    }

    #[test]
    fn click_still_runs_when_released_over_pressed_target() {
        let clicked = Rc::new(Cell::new(false));
        let click_state = Rc::clone(&clicked);
        let target = Target::new(
            Area::new(0.0, 0.0, 10.0, 10.0),
            Handlers {
                click: Some(Rc::new(move |_| click_state.set(true))),
                ..Default::default()
            },
        );

        let mut pointer = Pointer::default();
        pointer.set_targets(vec![target]);
        pointer.move_to(5.0, 5.0);
        pointer.press(Button::Left);

        assert!(pointer.release(Button::Left));
        assert!(clicked.get());
    }
}
