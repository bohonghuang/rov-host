use std::{borrow::Borrow, cell::RefCell, collections::HashMap, fmt::Debug, rc::Rc, sync::{Arc, Mutex}, time::Duration};

use fragile::Fragile;
use glib::{Continue, MainContext, PRIORITY_HIGH, Sender};
use sdl2::{JoystickSubsystem, Sdl, event::Event, joystick::Joystick};

use lazy_static::{__Deref, lazy_static};

#[derive(Debug, PartialEq, Clone, Eq)]
pub enum InputSource {
    Joystick(u32),
}

pub enum InputSystemMessage {
    RetrieveJoystickList, Connect(u32)
}

#[derive(Debug, Clone)]
pub enum InputSourceEvent {
    ButtonChanged(u8, bool),
    AxisChanged(u8, i16),
}

pub struct InputEvent(pub InputSource, pub InputSourceEvent);

lazy_static! {
    pub static ref SDL: Result<Fragile<Sdl>, String> = sdl2::init().map(Fragile::new);
}

pub struct InputSystem {
    pub sdl: Sdl,
    pub subsystem: JoystickSubsystem,
    pub event_sender: Rc<RefCell<Option<Sender<InputEvent>>>>,
    pub messsage_sender: Sender<InputSystemMessage>,
    running: Arc<Mutex<bool>>,
    // joysticks: Rc<RefCell<HashMap<u32, (Joystick, (u16, u16))>>>
}

impl InputSystem {
    pub fn get_sources(&self) -> Result<Vec<(InputSource, String)>, String>{
        let num = self.subsystem.num_joysticks()?;
        Ok((0..num).map(|index| (InputSource::Joystick(index), self.subsystem.name_for_index(index).unwrap_or("未知设备".to_string()))).collect())
    }
}

impl Debug for InputSystem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InputSystem").field("sdl", &String::from("SDL")).field("subsystem", &self.subsystem).field("event_sender", &self.event_sender).field("messsage_sender", &self.messsage_sender).finish()
    }
}

impl Default for InputSystem {
    fn default() -> Self {
        let sdl_fragile = SDL.deref().clone().unwrap();
        let sdl = sdl_fragile.get();
        // let sdl = sdl2::init().unwrap();
        let subsystem = sdl.joystick().unwrap();
        InputSystem::new(&sdl, &subsystem)
    }
}

impl InputSystem {
    pub fn new(sdl: &Sdl, subsystem: &JoystickSubsystem) -> Self {
        let (sys_sender, sys_receriver) = MainContext::channel(PRIORITY_HIGH);
        sys_receriver.attach(None, |msg| {
            match msg {
                InputSystemMessage::RetrieveJoystickList =>(),
                InputSystemMessage::Connect(id) => (),
            }
            Continue(true)
        });
        let event_sender: Rc<RefCell<Option<Sender<InputEvent>>>> = Rc::new(RefCell::new(None));

        Self {
            sdl: sdl.clone(),
            subsystem: subsystem.clone(),
            event_sender,
            messsage_sender: sys_sender,
            running: Arc::new(Mutex::new(false)),
            // joysticks: Rc::new(HashMap::new()),
        }
    }

    pub fn run(&self) {
        if *self.running.lock().unwrap() {
            return
        }
        let available = self.subsystem
            .num_joysticks()
            .map_err(|e| format!("can't enumerate joysticks: {}", e)).unwrap();
        let mut joysticks: HashMap<u32, (Joystick, (u16, u16))> = (0..available)
            .filter_map(|id| match self.subsystem.open(id) {
                Ok(c) => {
                    println!("Success: opened \"{}\"", c.name());
                    Some((id, (c, (0, 0))))
                }
                Err(e) => {
                    println!("failed: {:?}", e);
                    None
                }
            }).collect();
        
        let sdl = self.sdl.clone();
        let sender = self.event_sender.clone();
        let running = self.running.clone();
        *self.running.lock().unwrap() = true;
        glib::timeout_add_local(Duration::from_millis(16), move || {
            let mut event_pump = sdl.event_pump().expect("无法读取手柄事件");
            if let Some(sender) = sender.as_ref().borrow().as_ref() {
                for event in event_pump.poll_iter() {
                    match event {
                        Event::JoyAxisMotion {
                            axis_idx,
                            value: val,
                            which,
                            ..
                        } => {
                            let dead_zone = 2;
                            if val > dead_zone || val < -dead_zone {
                                println!("Axis {} moved to {}", axis_idx, val);
                            } else {
                                println!("Axis {} moved to {}", axis_idx, 0);
                            }
                            sender.send(InputEvent(InputSource::Joystick(which), InputSourceEvent::AxisChanged(axis_idx, val))).unwrap();
                        }
                        Event::JoyButtonDown { button_idx, which, .. } => {
                            println!("Button {} down", button_idx);
                            let (joystick, (lo_freq, hi_freq)) = joysticks.get_mut(&which).unwrap();
                            if button_idx == 0 {
                                *lo_freq = 65535;
                            } else if button_idx == 1 {
                                *hi_freq = 65535;
                            }
                            if button_idx < 2 {
                                match joystick.set_rumble(*lo_freq, *hi_freq, 15000) {
                                    Ok(()) => (), // println!("Set rumble to ({}, {})", lo_freq, hi_freq),
                                    Err(e) => println!(
                                        "Error setting rumble to ({}, {}): {:?}",
                                        lo_freq, hi_freq, e
                                    ),
                                }
                            }
                            sender.send(InputEvent(InputSource::Joystick(which), InputSourceEvent::ButtonChanged(button_idx, true))).unwrap();
                        }
                        Event::JoyButtonUp { button_idx, which, .. } => {
                            println!("Button {} up", button_idx);
                            let (joystick, (lo_freq, hi_freq)) = joysticks.get_mut(&which).unwrap();
                            if button_idx == 0 {
                                *lo_freq = 65535;
                            } else if button_idx == 1 {
                                *hi_freq = 65535;
                            }
                            if button_idx < 2 {
                                match joystick.set_rumble(*lo_freq, *hi_freq, 15000) {
                                    Ok(()) => (), // println!("Set rumble to ({}, {})", lo_freq, hi_freq),
                                    Err(e) => println!(
                                        "Error setting rumble to ({}, {}): {:?}",
                                        lo_freq, hi_freq, e
                                    ),
                                }
                            }
                            sender.send(InputEvent(InputSource::Joystick(which), InputSourceEvent::ButtonChanged(button_idx, false))).unwrap();
                        }
                        Event::JoyHatMotion { hat_idx, state, .. } => {
                            println!("Hat {} moved to {:?}", hat_idx, state)
                        }
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
