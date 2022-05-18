/* input.rs
 *
 * Copyright 2021-2022 Bohong Huang
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program. If not, see <http://www.gnu.org/licenses/>.
 */

use std::{cell::RefCell, collections::HashMap, fmt::Debug, rc::Rc, sync::{Arc, Mutex}, time::Duration, ops::Deref};

use glib::{Continue, Sender};

use sdl2::{Sdl, event::Event, GameControllerSubsystem};
use fragile::Fragile;

use lazy_static::lazy_static;

pub type Button = sdl2::controller::Button;
pub type Axis = sdl2::controller::Axis;
pub type GameController = sdl2::controller::GameController;

#[derive(Hash, Debug, PartialEq, Clone, Eq)]
pub enum InputSource {
    GameController(u32),
}

pub enum InputSystemMessage {
    RetrieveJoystickList, Connect(u32)
}

#[derive(Debug, Clone)]
pub enum InputSourceEvent {
    ButtonChanged(Button, bool),
    AxisChanged(Axis, i16),
}

pub struct InputEvent(pub InputSource, pub InputSourceEvent);

lazy_static! {
    pub static ref SDL: Result<Fragile<Sdl>, String> = sdl2::init().map(Fragile::new);
}

pub struct InputSystem {
    pub sdl: Sdl,
    pub game_controller_subsystem: GameControllerSubsystem,
    pub game_controllers: Arc<Mutex<HashMap<u32, GameController>>>, // GameController 在 drop 时会自动断开连接，因此容器来保存
    pub event_sender: Rc<RefCell<Option<Sender<InputEvent>>>>,
    running: Arc<Mutex<bool>>,
}

impl InputSystem {
    pub fn get_sources(&self) -> Result<Vec<(InputSource, String)>, String> {
        let num = self.game_controller_subsystem.num_joysticks()?;
        Ok((0..num).map(|index| (InputSource::GameController(index), self.game_controller_subsystem.name_for_index(index).unwrap_or("未知设备".to_string()))).collect())
    }
}

impl Debug for InputSystem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InputSystem")
            .field("game_controller_subsystem", &self.game_controller_subsystem)
            .field("event_sender", &self.event_sender)
            .field("running", &self.running)
            .finish()
    }
}

impl Default for InputSystem {
    fn default() -> Self {
        let sdl_fragile = Deref::deref(&SDL).clone().unwrap();
        let sdl = sdl_fragile.get();
        let game_controller_subsystem = sdl.game_controller().unwrap();
        InputSystem::new(&sdl, &game_controller_subsystem)
    }
}

impl InputSystem {
    pub fn new(sdl: &Sdl, game_controller_subsystem: &GameControllerSubsystem) -> Self {
        let event_sender: Rc<RefCell<Option<Sender<InputEvent>>>> = Rc::new(RefCell::new(None));

        Self {
            sdl: sdl.clone(),
            game_controller_subsystem: game_controller_subsystem.clone(),
            game_controllers: Arc::new(Mutex::new(HashMap::new())),
            event_sender,
            running: Arc::new(Mutex::new(false)),
        }
    }

    pub fn run(&self) {
        if *self.running.lock().unwrap() {
            return
        }
        
        let available = self.game_controller_subsystem
            .num_joysticks()
            .map_err(|e| format!("Can't enumerate joysticks: {}", e)).unwrap();
        for (id, game_controller) in (0..available).filter_map(|id| self.game_controller_subsystem.open(id).ok().map(|c| (id, c))) {
            self.game_controllers.lock().unwrap().insert(id, game_controller);
        }
        
        let sdl = self.sdl.clone();
        let sender = self.event_sender.clone();
        let running = self.running.clone();
        *self.running.lock().unwrap() = true;
        let game_controller_subsystem = self.game_controller_subsystem.clone();
        let game_controllers = self.game_controllers.clone();
        glib::timeout_add_local(Duration::from_millis(16), move || {
            let mut event_pump = sdl.event_pump().expect("Cannot get event pump from SDL");
            if let Some(sender) = sender.as_ref().borrow().as_ref() {
                for event in event_pump.poll_iter() {
                    match event {
                        Event::ControllerAxisMotion { axis, which, value, .. } => sender.send(InputEvent(InputSource::GameController(which), InputSourceEvent::AxisChanged(axis, value))).unwrap(),
                        Event::ControllerButtonDown { button, which, .. } => sender.send(InputEvent(InputSource::GameController(which), InputSourceEvent::ButtonChanged(button, true))).unwrap(),
                        Event::ControllerButtonUp { button, which, .. } => sender.send(InputEvent(InputSource::GameController(which), InputSourceEvent::ButtonChanged(button, false))).unwrap(),
                        Event::ControllerDeviceAdded { which, .. } => {
                            if let Ok(game_controller) = game_controller_subsystem.open(which) {
                                game_controllers.lock().unwrap().insert(which, game_controller);
                            }
                        },
                        Event::ControllerDeviceRemoved { which, .. } => {
                            game_controllers.lock().unwrap().remove(&which);
                        },
                        Event::Quit { .. } => break,
                        _ => (),
                    }
                }
            } else {
                event_pump.poll_iter().last();
            }
            Continue(*running.clone().lock().unwrap())
        });
    }

    pub fn stop(&self) {
        *self.running.lock().unwrap() = false;
    }
}
