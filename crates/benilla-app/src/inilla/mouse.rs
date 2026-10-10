//! **The virtual mouse**: touch gestures and the controller's cursor mode speak mouse to the game,
//! because the game — the stock FrameXML interface, the world click-to-target, mouse-look — is
//! written for one. Operations are turned into the very messages winit's mouse would have produced
//! (`CursorMoved`, `MouseButtonInput`, `MouseMotion`, `MouseWheel`) and the window's cursor
//! position is set, all before Bevy's input systems run, so every reader in benilla sees an
//! ordinary mouse.
//!
//! **One frame between a move and a button.** benilla computes what is under the cursor (the UI
//! hover, the world pick) from the cursor position, and a click acts on that answer. A move and a
//! press in the same frame could act on the previous frame's answer, so a press waits one frame
//! after a move, and a release one frame after its press: a tap is `move · press · release` over
//! three frames (~50 ms), which also gives every "pressed this frame" reader its own frame.

use std::collections::VecDeque;

use bevy::input::mouse::{MouseButtonInput, MouseMotion, MouseScrollUnit, MouseWheel};
use bevy::input::ButtonState;
use bevy::math::DVec2;
use bevy::prelude::*;
use bevy::window::{CursorLeft, CursorMoved, PrimaryWindow};

use inilla_core::touch::{MouseButton as KButton, MouseOp};

#[derive(Resource, Default)]
pub(crate) struct VirtualMouse {
    queue: VecDeque<MouseOp>,
    /// Where the cursor is (logical points), when there is one.
    pub(crate) pos: Option<Vec2>,
    pub(crate) left: bool,
    pub(crate) right: bool,
}

impl VirtualMouse {
    pub(crate) fn push(&mut self, op: MouseOp) {
        self.queue.push_back(op);
    }

    pub(crate) fn extend(&mut self, ops: impl IntoIterator<Item = MouseOp>) {
        self.queue.extend(ops);
    }

    /// Release anything held and forget the queue (focus lost, HUD off).
    pub(crate) fn reset(&mut self) {
        self.queue.clear();
        if self.left {
            self.queue.push_back(MouseOp::Up(KButton::Left));
        }
        if self.right {
            self.queue.push_back(MouseOp::Up(KButton::Right));
        }
    }
}

fn button(b: KButton) -> MouseButton {
    match b {
        KButton::Left => MouseButton::Left,
        KButton::Right => MouseButton::Right,
    }
}

/// Drain as much of the queue as this frame may carry. Runs in `PreUpdate` before
/// `InputSystems`.
pub(crate) fn apply(
    mut vm: ResMut<VirtualMouse>,
    mut windows: Query<(Entity, &mut Window), With<PrimaryWindow>>,
    mut buttons: MessageWriter<MouseButtonInput>,
    mut motion: MessageWriter<MouseMotion>,
    mut wheel: MessageWriter<MouseWheel>,
    mut moved: MessageWriter<CursorMoved>,
    mut left_window: MessageWriter<CursorLeft>,
) {
    let Ok((window_entity, mut window)) = windows.single_mut() else {
        return;
    };
    let scale = window.resolution.scale_factor() as f64;
    let mut moved_this_frame = false;
    let mut clicked_this_frame = false;
    while let Some(op) = vm.queue.front().copied() {
        match op {
            MouseOp::MoveTo(p) => {
                if clicked_this_frame {
                    break;
                }
                let p = Vec2::new(p[0], p[1]);
                let delta = vm.pos.map(|old| p - old);
                if vm.pos != Some(p) {
                    window.set_physical_cursor_position(Some(DVec2::new(
                        f64::from(p.x) * scale,
                        f64::from(p.y) * scale,
                    )));
                    moved.write(CursorMoved {
                        window: window_entity,
                        position: p,
                        delta,
                    });
                    vm.pos = Some(p);
                    moved_this_frame = true;
                }
            }
            MouseOp::Down(b) | MouseOp::Up(b) => {
                if moved_this_frame || clicked_this_frame {
                    break;
                }
                let down = matches!(op, MouseOp::Down(_));
                let held = match b {
                    KButton::Left => &mut vm.left,
                    KButton::Right => &mut vm.right,
                };
                if *held != down {
                    *held = down;
                    buttons.write(MouseButtonInput {
                        button: button(b),
                        state: if down {
                            ButtonState::Pressed
                        } else {
                            ButtonState::Released
                        },
                        window: window_entity,
                    });
                    clicked_this_frame = true;
                }
            }
            MouseOp::Motion(d) => {
                if clicked_this_frame && !(vm.left || vm.right) {
                    break;
                }
                motion.write(MouseMotion {
                    delta: Vec2::new(d[0], d[1]),
                });
            }
            MouseOp::Wheel(n) => {
                wheel.write(MouseWheel {
                    unit: MouseScrollUnit::Line,
                    x: 0.0,
                    y: n,
                    window: window_entity,
                });
            }
            MouseOp::Leave => {
                if vm.left || vm.right || clicked_this_frame {
                    // Never leave with a button held: let the release land first.
                    if vm.left || vm.right {
                        vm.queue.pop_front();
                        vm.queue.push_front(MouseOp::Leave);
                        let pending_up = if vm.left {
                            KButton::Left
                        } else {
                            KButton::Right
                        };
                        vm.queue.push_front(MouseOp::Up(pending_up));
                    }
                    break;
                }
                if vm.pos.is_some() {
                    window.set_physical_cursor_position(None);
                    left_window.write(CursorLeft {
                        window: window_entity,
                    });
                    vm.pos = None;
                }
            }
        }
        vm.queue.pop_front();
    }
}
