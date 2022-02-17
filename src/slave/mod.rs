/* mod.rs
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

pub mod video;
pub mod param_tuner;
pub mod slave_config;
pub mod slave_video;
pub mod firmware_update;

use std::{cell::RefCell, collections::{HashMap, VecDeque}, rc::Rc, sync::{Arc, Mutex}, fmt::Debug, time::{Duration, SystemTime}, ops::Deref, io::Error as IOError};
use async_std::{net::TcpStream, prelude::*, task::{JoinHandle, self}};

use glib::{PRIORITY_DEFAULT, Sender, WeakRef, DateTime, MainContext};
use glib_macros::clone;
use gtk::{prelude::*, Align, Box as GtkBox, Button, CenterBox, CheckButton, Frame, Grid, Image, Label, ListBox, MenuButton, Orientation, Overlay, Popover, Revealer, Switch, ToggleButton, Widget, Separator, PackType, Inhibit};
use adw::{ApplicationWindow, ToastOverlay, Toast, Flap};
use relm4::{WidgetPlus, factory::{FactoryPrototype, FactoryVec, positions::GridPosition}, send, MicroWidgets, MicroModel, MicroComponent};
use relm4_macros::micro_widget;

use serde::{Serialize, Deserialize};
use derivative::*;

use crate::{input::{InputSource, InputSourceEvent, InputSystem}, slave::param_tuner::SlaveParameterTunerMsg};
use crate::preferences::PreferencesModel;
use crate::ui::generic::error_message;
use crate::AppMsg;
use self::{param_tuner::SlaveParameterTunerModel, slave_config::{SlaveConfigModel, SlaveConfigMsg}, slave_video::{SlaveVideoModel, SlaveVideoMsg}, firmware_update::SlaveFirmwareUpdaterModel};

#[tracker::track(pub)]
#[derive(Debug, Derivative)]
#[derivative(Default)]
pub struct SlaveModel {
    #[no_eq]
    #[derivative(Default(value="MyComponent::new(Default::default(), MainContext::channel(PRIORITY_DEFAULT).0)"))]
    pub config: MyComponent<SlaveConfigModel>,
    #[no_eq]
    #[derivative(Default(value="MyComponent::new(Default::default(), MainContext::channel(PRIORITY_DEFAULT).0)"))]
    pub video: MyComponent<SlaveVideoModel>,
    #[derivative(Default(value="Some(false)"))]
    pub connected: Option<bool>,
    #[derivative(Default(value="Some(false)"))]
    pub polling: Option<bool>,
    #[derivative(Default(value="Some(false)"))]
    pub recording: Option<bool>,
    pub sync_recording: bool,
    #[no_eq]
    pub preferences: Rc<RefCell<PreferencesModel>>,
    pub input_source: Option<InputSource>,
    #[no_eq]
    pub input_system: Rc<InputSystem>,
    #[no_eq]
    #[derivative(Default(value="MainContext::channel(PRIORITY_DEFAULT).0"))]
    pub input_event_sender: Sender<InputSourceEvent>,
    pub slave_info_displayed: bool,
    #[no_eq]
    pub status: Arc<Mutex<HashMap<SlaveStatusClass, i16>>>,
    #[no_eq]
    pub tcp_msg_sender: Option<async_std::channel::Sender<SlaveTcpMsg>>,
    #[no_eq]
    pub tcp_stream: Option<async_std::sync::Arc<TcpStream>>,
    #[no_eq]
    pub toast_messages: Rc<RefCell<VecDeque<String>>>,
    #[no_eq]
    #[derivative(Default(value="FactoryVec::new()"))]
    pub infos: FactoryVec<SlaveInfoModel>,
    pub config_presented: bool,
}

#[tracker::track(pub)]
#[derive(Debug, Derivative)]
#[derivative(Default)]
pub struct SlaveInfoModel {
    key: String,
    value: String,
}

#[relm4::factory_prototype(pub)]
impl FactoryPrototype for SlaveInfoModel {
    type Factory = FactoryVec<Self>;
    type Widgets = SlaveInfoWidgets;
    type View = GtkBox;
    type Msg = SlaveMsg;

    view! {
        entry = CenterBox {
            set_orientation: Orientation::Horizontal,
            set_hexpand: true,
            set_start_widget = Some(&Label) {
                set_valign: Align::Start,
                set_markup: track!(self.changed(SlaveInfoModel::key()), &format!("<b>{}</b>", self.get_key())),
            },
            set_end_widget = Some(&Label) {
                set_valign: Align::Start,
                set_label: track!(self.changed(SlaveInfoModel::value()), self.get_value()),
            }
        }
    }

    fn position(&self, _index: &usize) {
        
    }
}

#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub enum SlaveStatusClass {
    MotionX, MotionY, MotionZ, MotionRotate,
    DepthLocked, DirectionLocked,
}

impl SlaveStatusClass {
    pub fn from_button(button: u8) -> Option<SlaveStatusClass> {
        match button {
            7 => Some(SlaveStatusClass::DepthLocked),
            8 => Some(SlaveStatusClass::DirectionLocked),
            _ => None,
        }
    }
    pub fn from_axis(axis: u8) -> Option<SlaveStatusClass> {
        match axis {
            0 => Some(SlaveStatusClass::MotionX),
            1 => Some(SlaveStatusClass::MotionY),
            2 => Some(SlaveStatusClass::MotionRotate),
            3 => Some(SlaveStatusClass::MotionZ),
            _ => None
        }
    }
}

const JOYSTICK_DISPLAY_THRESHOLD: i16 = 500;

impl SlaveModel {
    pub fn new(config: SlaveConfigModel, preferences: Rc<RefCell<PreferencesModel>>, component_sender: &Sender<SlaveMsg>, input_event_sender: Sender<InputSourceEvent>) -> Self {
        Self {
            config: MyComponent::new(config, component_sender.clone()),
            video: MyComponent::new(SlaveVideoModel::new(preferences.clone()), component_sender.clone()),
            preferences,
            input_event_sender,
            status: Arc::new(Mutex::new(HashMap::new())),
            ..Default::default()
        }
    }
    
    pub fn get_target_status_or_insert_0(&mut self, status_class: &SlaveStatusClass) -> i16 {
        let mut status = self.status.lock().unwrap();
        *status.entry(status_class.clone()).or_insert(0)
    }

    pub fn get_target_status(&self, status_class: &SlaveStatusClass) -> i16 {
        let status = self.status.lock().unwrap();
        *status.get(status_class).unwrap_or(&0)
    }
    pub fn set_target_status(&mut self, status_class: &SlaveStatusClass, new_status: i16) {
        let mut status = self.get_mut_status().lock().unwrap();
        *status.entry(status_class.clone()).or_insert(0) = new_status;
    }
}

pub fn input_sources_list_box(input_source: &Option<InputSource>, input_system: &InputSystem, sender: &Sender<SlaveMsg>) -> Widget {
    let sources = input_system.get_sources().unwrap();
    if sources.is_empty() {
        return Label::builder()
            .label("无可用设备")
            .margin_top(4)
            .margin_bottom(4)
            .margin_start(4)
            .margin_end(4)
            .build().upcast();
    }
    let list_box = ListBox::builder().build();
    let mut radio_button_group: Option<CheckButton> = None;
    for (source, name) in sources {
        let radio_button = CheckButton::builder().label(&name).build();
        let sender = sender.clone();
        radio_button.set_active(match input_source {
            Some(current_souce) => current_souce.eq(&source),
            None => false,
        });
        
        radio_button.connect_toggled(move |button| {
            sender.send(SlaveMsg::SetInputSource(if button.is_active() { Some(source.clone()) } else { None } )).unwrap();
        });
        {
            let radio_button = radio_button.clone();
            match &radio_button_group {
                Some(button) => radio_button.set_group(Some(button)),
                None => radio_button_group = Some(radio_button),
            }
        }
        list_box.append(&radio_button);
    }
    list_box.upcast()
}

#[micro_widget(pub)]
impl MicroWidgets<SlaveModel> for SlaveWidgets {
    view! {
        toast_overlay = ToastOverlay {
            add_toast?: watch!(model.get_toast_messages().borrow_mut().pop_front().map(|x| Toast::new(&x)).as_ref()),
            set_child = Some(&GtkBox) {
                set_orientation: Orientation::Vertical,
                append = &CenterBox {
                    set_css_classes: &["toolbar"],
                    set_orientation: Orientation::Horizontal,
                    set_start_widget = Some(&GtkBox) {
                        set_hexpand: true,
                        set_halign: Align::Start,
                        set_spacing: 5,
                        append = &Button {
                            set_icon_name: "network-transmit-symbolic",
                            set_sensitive: track!(model.changed(SlaveModel::connected()), model.connected != None),
                            set_css_classes?: watch!(model.connected.map(|x| if x { vec!["circular", "suggested-action"] } else { vec!["circular"] }).as_ref()),
                            set_tooltip_text: track!(model.changed(SlaveModel::connected()), model.connected.map(|x| if x { "断开连接" } else { "连接" })),
                            connect_clicked(sender) => move |_button| {
                                send!(sender, SlaveMsg::ToggleConnect);
                            },
                        },
                        append = &Button {
                            set_icon_name: "video-display-symbolic",
                            set_sensitive: track!(model.changed(SlaveModel::recording()) || model.changed(SlaveModel::sync_recording()) || model.changed(SlaveModel::polling()), model.get_recording().is_some() && model.get_polling().is_some() && !model.sync_recording),
                            set_css_classes?: watch!(model.polling.map(|x| if x { vec!["circular", "destructive-action"] } else { vec!["circular"] }).as_ref()),
                            set_tooltip_text: track!(model.changed(SlaveModel::polling()), model.polling.map(|x| if x { "停止拉流" } else { "启动拉流" })),
                            connect_clicked(sender) => move |_button| {
                                send!(sender, SlaveMsg::TogglePolling);
                            },
                        },
                        append = &Separator {},
                        append = &Button {
                            set_icon_name: "camera-photo-symbolic",
                            set_sensitive: watch!(model.video.model().get_pixbuf().is_some()),
                            set_css_classes: &["circular"],
                            set_tooltip_text: Some("画面截图"),
                            connect_clicked(sender) => move |_button| {
                                send!(sender, SlaveMsg::TakeScreenshot);
                            },
                        },
                        append = &Button {
                            set_icon_name: "camera-video-symbolic",
                            set_sensitive: track!(model.changed(SlaveModel::sync_recording()) || model.changed(SlaveModel::polling()) || model.changed(SlaveModel::recording()), !model.sync_recording && model.recording != None &&  model.polling == Some(true)),
                            set_css_classes?: watch!(model.recording.map(|x| if x { vec!["circular", "destructive-action"] } else { vec!["circular"] }).as_ref()),
                            set_tooltip_text: track!(model.changed(SlaveModel::recording()), model.polling.map(|x| if x { "停止录制" } else { "开始录制" })),
                            connect_clicked(sender) => move |_button| {
                                send!(sender, SlaveMsg::ToggleRecord);
                            },
                        },
                    },
                    set_center_widget = Some(&GtkBox) {
                        set_hexpand: true,
                        set_halign: Align::Center,
                        set_spacing: 5,
                        append = &Label {
                            set_text: track!(model.changed(SlaveModel::config()), format!("{}:{}", model.config.model().get_ip(), model.config.model().get_port()).as_str()),
                        },
                        append = &MenuButton {
                            set_icon_name: "input-gaming-symbolic",
                            set_css_classes: &["circular"],
                            set_tooltip_text: Some("切换当前机位使用的输入设备"),
                            set_popover = Some(&Popover) {
                                set_child = Some(&GtkBox) {
                                    set_spacing: 5,
                                    set_orientation: Orientation::Vertical, 
                                    append = &CenterBox {
                                        set_center_widget = Some(&Label) {
                                            set_margin_start: 10,
                                            set_margin_end: 10,
                                            set_markup: "<b>输入设备</b>"
                                        },
                                        set_end_widget = Some(&Button) {
                                            set_icon_name: "view-refresh-symbolic",
                                            set_css_classes: &["circular"],
                                            set_tooltip_text: Some("刷新输入设备"),
                                            connect_clicked(sender) => move |_button| {
                                                send!(sender, SlaveMsg::UpdateInputSources);
                                            },
                                        },
                                    },
                                    append = &Frame {
                                        set_child: track!(model.changed(SlaveModel::input_system()), Some(&input_sources_list_box(&model.input_source, &model.input_system ,&sender))),
                                    },
                                    
                                },
                            },
                        },
                    },
                    set_end_widget = Some(&GtkBox) {
                        set_hexpand: true,
                        set_halign: Align::End,
                        set_spacing: 5,
                        set_margin_end: 5,
                        append = &Button {
                            set_icon_name: "software-update-available-symbolic",
                            set_css_classes: &["circular"],
                            set_tooltip_text: Some("固件更新"),
                            connect_clicked(sender) => move |_button| {
                                send!(sender, SlaveMsg::OpenFirmwareUpater);
                            },
                        },
                        append = &Button {
                            set_icon_name: "preferences-other-symbolic",
                            set_css_classes: &["circular"],
                            set_tooltip_text: Some("参数调校"),
                            connect_clicked(sender) => move |_button| {
                                send!(sender, SlaveMsg::OpenParameterTuner);
                            },
                        },
                        append = &Separator {},
                        append = &ToggleButton {
                            set_icon_name: "emblem-system-symbolic",
                            set_css_classes: &["circular"],
                            set_tooltip_text: Some("机位设置"),
                            set_active: track!(model.changed(SlaveModel::config_presented()), *model.get_config_presented()),
                            connect_active_notify(sender) => move |button| {
                                send!(sender, SlaveMsg::SetConfigPresented(button.is_active()));
                            },
                        },
                        append = &ToggleButton {
                            set_icon_name: "window-close-symbolic",
                            set_css_classes: &["circular"],
                            set_tooltip_text: Some("移除机位"),
                            set_visible: false,
                            connect_active_notify(sender) => move |_button| {
                                send!(sender, SlaveMsg::DestroySlave);
                            },
                        },
                    },
                },
                append = &Flap {
                    set_flap: Some(model.config.root_widget()),
                    set_reveal_flap: track!(model.changed(SlaveModel::config_presented()), *model.get_config_presented()),
                    set_locked: true,
                    set_flap_position: PackType::End,
                    set_separator = Some(&Separator) {},
                    set_content = Some(&Overlay) {
                        set_child: Some(model.video.root_widget()),
                        add_overlay = &GtkBox {
                            set_valign: Align::Start,
                            set_halign: Align::End,
                            set_hexpand: true,
                            set_margin_all: 20, 
                            append = &Frame {
                                add_css_class: "card",
                                set_child = Some(&GtkBox) {
                                    set_orientation: Orientation::Vertical,
                                    set_margin_all: 5,
                                    set_width_request: 50,
                                    set_spacing: 5,
                                    append = &Button {
                                        set_child = Some(&CenterBox) {
                                            set_center_widget = Some(&Label) {
                                                set_margin_start: 10,
                                                set_margin_end: 10,
                                                set_text: "状态信息",
                                            },
                                            set_end_widget = Some(&Image) {
                                                set_icon_name: watch!(Some(if model.slave_info_displayed { "go-down-symbolic" } else { "go-next-symbolic" })),
                                            },
                                        },
                                        connect_clicked(sender) => move |_button| {
                                            send!(sender, SlaveMsg::ToggleDisplayInfo);
                                        },
                                    },
                                    append = &Revealer {
                                        set_reveal_child: watch!(model.slave_info_displayed),
                                        set_child = Some(&GtkBox) {
                                            set_spacing: 5,
                                            set_margin_all: 5,
                                            set_orientation: Orientation::Vertical,
                                            set_halign: Align::Center,
                                            append = &GtkBox {
                                                set_hexpand: true,
                                                set_halign: Align::Center,
                                                append = &Grid {
                                                    set_margin_all: 2,
                                                    set_row_spacing: 2,
                                                    set_column_spacing: 2,
                                                    attach(0, 0, 1, 1) = &ToggleButton {
                                                        set_icon_name: "object-rotate-left-symbolic",
                                                        set_can_focus: false,
                                                        set_can_target: false,
                                                        set_active: track!(model.changed(SlaveModel::status()), model.get_target_status(&SlaveStatusClass::MotionRotate) < -JOYSTICK_DISPLAY_THRESHOLD),
                                                    },
                                                    attach(2, 0, 1, 1) = &ToggleButton {
                                                        set_icon_name: "object-rotate-right-symbolic",
                                                        set_can_focus: false,
                                                        set_can_target: false,
                                                        set_active: track!(model.changed(SlaveModel::status()), model.get_target_status(&SlaveStatusClass::MotionRotate) > JOYSTICK_DISPLAY_THRESHOLD),
                                                    },
                                                    attach(0, 2, 1, 1) = &ToggleButton {
                                                        set_icon_name: "go-bottom-symbolic",
                                                        set_can_focus: false,
                                                        set_can_target: false,
                                                        set_active: track!(model.changed(SlaveModel::status()), model.get_target_status(&SlaveStatusClass::MotionZ) < -JOYSTICK_DISPLAY_THRESHOLD),
                                                    },
                                                    attach(2, 2, 1, 1) = &ToggleButton {
                                                        set_icon_name: "go-top-symbolic",
                                                        set_can_focus: false,
                                                        set_can_target: false,
                                                        set_active: track!(model.changed(SlaveModel::status()), model.get_target_status(&SlaveStatusClass::MotionZ) > JOYSTICK_DISPLAY_THRESHOLD),
                                                    },
                                                    attach(1, 0, 1, 1) = &ToggleButton {
                                                        set_icon_name: "go-up-symbolic",
                                                        set_can_focus: false,
                                                        set_can_target: false,
                                                        set_active: track!(model.changed(SlaveModel::status()), model.get_target_status(&SlaveStatusClass::MotionY) > JOYSTICK_DISPLAY_THRESHOLD),
                                                    },
                                                    attach(0, 1, 1, 1) = &ToggleButton {
                                                        set_icon_name: "go-previous-symbolic",
                                                        set_can_focus: false,
                                                        set_can_target: false,
                                                        set_active: track!(model.changed(SlaveModel::status()), model.get_target_status(&SlaveStatusClass::MotionX) < -JOYSTICK_DISPLAY_THRESHOLD),
                                                    },
                                                    attach(2, 1, 1, 1) = &ToggleButton {
                                                        set_icon_name: "go-next-symbolic",
                                                        set_can_focus: false,
                                                        set_can_target: false,
                                                        set_active: track!(model.changed(SlaveModel::status()), model.get_target_status(&SlaveStatusClass::MotionX) > JOYSTICK_DISPLAY_THRESHOLD),
                                                    },
                                                    attach(1, 2, 1, 1) = &ToggleButton {
                                                        set_icon_name: "go-down-symbolic",
                                                        set_can_focus: false,
                                                        set_can_target: false,
                                                        set_active: track!(model.changed(SlaveModel::status()), model.get_target_status(&SlaveStatusClass::MotionY) < -JOYSTICK_DISPLAY_THRESHOLD),
                                                    },
                                                },
                                            },
                                            append = &GtkBox {
                                                set_orientation: Orientation::Vertical,
                                                set_spacing: 5,
                                                set_hexpand: true,
                                                factory!(model.infos),
                                            },
                                            append = &CenterBox {
                                                set_hexpand: true,
                                                set_start_widget = Some(&Label) {
                                                    set_markup: "<b>深度锁定</b>",
                                                },
                                                set_end_widget = Some(&Switch) {
                                                    set_active: track!(model.changed(SlaveModel::status()), model.get_target_status(&SlaveStatusClass::DepthLocked) != 0),
                                                    connect_state_set(sender) => move |_switch, state| {
                                                        send!(sender, SlaveMsg::SetSlaveStatus(SlaveStatusClass::DepthLocked, if state { 1 } else { 0 }));
                                                        Inhibit(false)
                                                    },
                                                },
                                            },
                                            append = &CenterBox {
                                                set_hexpand: true,
                                                set_start_widget = Some(&Label) {
                                                    set_markup: "<b>方向锁定</b>",
                                                },
                                                set_end_widget = Some(&Switch) {
                                                    set_active: track!(model.changed(SlaveModel::status()), model.get_target_status(&SlaveStatusClass::DirectionLocked) != 0),
                                                    connect_state_set(sender) => move |_switch, state| {
                                                        send!(sender, SlaveMsg::SetSlaveStatus(SlaveStatusClass::DirectionLocked, if state { 1 } else { 0 }));
                                                        Inhibit(false)
                                                    },
                                                },
                                            },
                                        },
                                    },
                                },
                            },
                        },
                    },
                    connect_reveal_flap_notify(sender) => move |flap| {
                        send!(sender, SlaveMsg::SetConfigPresented(flap.reveals_flap()));
                    },
                },
            },
        }
    }
}

impl std::fmt::Debug for SlaveWidgets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.toast_overlay.fmt(f)
    }
}

pub enum SlaveMsg {
    ConfigUpdated,
    ToggleRecord,
    ToggleConnect,
    TogglePolling,
    PollingChanged(bool),
    RecordingChanged(bool),
    TakeScreenshot,
    SetInputSource(Option<InputSource>),
    SetSlaveStatus(SlaveStatusClass, i16),
    UpdateInputSources,
    ToggleDisplayInfo,
    InputReceived(InputSourceEvent),
    OpenFirmwareUpater,
    OpenParameterTuner,
    DestroySlave,
    ErrorMessage(String),
    TcpError(String),
    TcpConnectionChanged(Option<async_std::sync::Arc<TcpStream>>),
    ShowToastMessage(String),
    TcpMessage(SlaveTcpMsg),
    InformationsReceived(HashMap<String, String>),
    SetConfigPresented(bool),
}

pub enum SlaveTcpMsg {
    ConnectionLost(IOError),
    Disconnect,
    SendString(String),
    ControlUpdated(ControlPacket),
    Block(JoinHandle<Result<(), IOError>>),
}

async fn tcp_main_handler(input_rate: u16,
                          tcp_stream: Arc<TcpStream>,
                          tcp_sender: async_std::channel::Sender<SlaveTcpMsg>,
                          tcp_receiver: async_std::channel::Receiver<SlaveTcpMsg>,
                          slave_sender: Sender<SlaveMsg>) -> Result<(), IOError> {
    fn current_millis() -> u128 {
        SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_millis()
    }
    send!(slave_sender, SlaveMsg::TcpConnectionChanged(Some(tcp_stream.clone())));
    
    let mut tcp_stream = &*tcp_stream;
    let idle = async_std::sync::Arc::new(async_std::sync::Mutex::new(true));
    let last_action_timestamp = async_std::sync::Arc::new(async_std::sync::Mutex::new(current_millis()));
    let control_packet = async_std::sync::Arc::new(async_std::sync::Mutex::new(None as Option<ControlPacket>));
    
    const IDLE_TIME_MILLIS: u128 = 5000;

    let connection_test_task = task::spawn(clone!(@strong idle, @strong tcp_sender, @strong tcp_stream, @strong last_action_timestamp => async move {
        let mut tcp_stream = &tcp_stream;
        tcp_stream.flush().await.unwrap();
        loop {
            if tcp_sender.is_closed() {
                return;
            }
            if *idle.lock().await {
                if current_millis() - *last_action_timestamp.lock().await >= IDLE_TIME_MILLIS {
                    if let Err(err) = tcp_stream.write_all("{}".as_bytes()).await {
                        tcp_sender.send(SlaveTcpMsg::ConnectionLost(err)).await.unwrap_or_default();
                        break;
                    }
                }
            }
            task::sleep(Duration::from_millis(500)).await;
        }
    }));

    let receive_task = task::spawn(clone!(@strong tcp_sender, @strong idle, @strong slave_sender, @strong tcp_stream => async move {
        let mut tcp_stream = tcp_stream.clone();
        let mut buf = [0u8; 1024];
        loop {
            if *idle.lock().await {
                buf.fill(0);
                if let Err(err) = tcp_stream.read(&mut buf).await {
                    tcp_sender.send(SlaveTcpMsg::ConnectionLost(err)).await.unwrap_or_default();
                    break;
                } else {
                    let json_string = match std::str::from_utf8(buf.split(|x| x.eq(&0)).next().unwrap()) {
                        Ok(string) => string,
                        Err(_) => continue,
                    };
                    if json_string.is_empty() {
                        tcp_sender.send(SlaveTcpMsg::ConnectionLost(IOError::new(std::io::ErrorKind::ConnectionAborted, "下位机主动断开连接（EOF）"))).await.unwrap_or_default();
                        break;
                    }
                    let msg = serde_json::from_str::<SlaveInfoPacket>(&json_string);
                    match msg {
                        Ok(packet) => {
                            send!(slave_sender, SlaveMsg::InformationsReceived(packet.info));
                        },
                        Err(err) => eprintln!("无法识别来自于下位机的JSON数据包（{}）：“{}”", err.to_string(), json_string),
                    }
                }
            }
        }
    }));
    
    let control_send_task = task::spawn(clone!(@strong idle, @strong tcp_sender, @strong tcp_stream, @strong control_packet => async move {
        let mut tcp_stream = &tcp_stream;
        loop {
            if tcp_sender.is_closed() {
                return;
            }
            if *idle.lock().await {
                let mut control_mutex = control_packet.lock().await;
                if let Some(control) = control_mutex.as_ref() {
                    match tcp_stream.write_all(control.to_string().as_bytes()).await {
                        Ok(_) => *control_mutex = None,
                        Err(err) => {
                            tcp_sender.send(SlaveTcpMsg::ConnectionLost(err)).await.unwrap_or_default();
                            break;
                        }
                    }
                }
            }
            task::sleep(Duration::from_millis(1000 / input_rate as u64)).await;
        }
    }));
    
    loop {
        match tcp_receiver.recv().await {
            Ok(msg) if *idle.lock().await => {
                match msg {
                    SlaveTcpMsg::Disconnect => {
                        if tcp_stream.shutdown(std::net::Shutdown::Both).is_ok() {
                            connection_test_task.cancel().await;
                            control_send_task.cancel().await;
                            receive_task.cancel().await;
                            send!(slave_sender, SlaveMsg::TcpConnectionChanged(None));
                        }
                        tcp_receiver.close();
                        break;
                    },
                    SlaveTcpMsg::ConnectionLost(err) => {
                        tcp_stream.shutdown(std::net::Shutdown::Both).unwrap_or_default();
                        connection_test_task.cancel().await;
                        control_send_task.cancel().await;
                        receive_task.cancel().await;
                        send!(slave_sender, SlaveMsg::TcpError(err.to_string()));
                        tcp_receiver.close();
                        return Err(err);
                    }
                    SlaveTcpMsg::SendString(string) => {
                        tcp_stream.write_all(string.as_bytes()).await?;
                        *last_action_timestamp.lock().await = current_millis();
                    },
                    SlaveTcpMsg::ControlUpdated(control) => {
                        *control_packet.lock().await = Some(control);
                        *last_action_timestamp.lock().await = current_millis();
                    },
                    SlaveTcpMsg::Block(blocker) => {
                        *idle.lock().await = false;
                        task::spawn(clone!(@strong idle => async move {
                            if let Err(err) = blocker.await {
                                eprintln!("模块异常退出：{}", err);
                            }
                            *idle.lock().await = true;
                        }));
                    },
                }
            },
            _ => (),
        }
    }
    Ok(())
}

impl MicroModel for SlaveModel {
    type Msg = SlaveMsg;
    type Widgets = SlaveWidgets;
    type Data = (Sender<AppMsg>, WeakRef<ApplicationWindow>);
    fn update(&mut self, msg: SlaveMsg, (parent_sender, window): &Self::Data, sender: Sender<SlaveMsg>) {
        self.reset();
        match msg {
            SlaveMsg::ConfigUpdated => {
                let config = self.get_mut_config().model().clone();
                send!(self.video.sender(), SlaveVideoMsg::ConfigUpdated(config));
            },
            SlaveMsg::ToggleConnect => {
                match self.get_connected() {
                    Some(true) => { // 断开连接
                        self.set_connected(None);
                        self.config.send(SlaveConfigMsg::SetConnected(None)).unwrap();
                        let sender = self.get_tcp_msg_sender().clone().unwrap();
                        task::spawn(async move {
                            sender.send(SlaveTcpMsg::Disconnect).await.expect("TCP should be running");
                        });
                    },
                    Some(false) => { // 连接
                        self.set_connected(None);
                        self.config.send(SlaveConfigMsg::SetConnected(None)).unwrap();
                        let (tcp_sender, tcp_receiver) = async_std::channel::bounded::<SlaveTcpMsg>(128);
                        self.set_tcp_msg_sender(Some(tcp_sender.clone()));
                        let sender = sender.clone();
                        let control_sending_rate = *self.preferences.borrow().get_default_input_sending_rate();
                        let ip = *self.config.model().get_ip();
                        let port = *self.config.model().get_port();
                        async_std::task::spawn(async move {
                            match TcpStream::connect(format!("{}:{}", ip, port)).await.map(|x| async_std::sync::Arc::new(x)) {
                                Ok(tcp_stream) => {
                                    tcp_main_handler(control_sending_rate, tcp_stream.clone(), tcp_sender, tcp_receiver, sender.clone()).await.unwrap_or_default();
                                },
                                Err(err) => send!(sender, SlaveMsg::TcpError(err.to_string())),
                            }
                        });
                    },
                    None => (),
                }
            },
            SlaveMsg::TogglePolling => {
                match self.get_polling() {
                    Some(true) =>{
                        self.video.send(SlaveVideoMsg::StopPipeline).unwrap();
                        self.set_polling(None);
                        self.config.send(SlaveConfigMsg::SetPolling(None)).unwrap();
                    },
                    Some(false) => {
                        self.video.send(SlaveVideoMsg::StartPipeline).unwrap();
                        self.set_polling(None);
                        self.config.send(SlaveConfigMsg::SetPolling(None)).unwrap();
                    },
                    None => (),
                }
            },
            SlaveMsg::SetInputSource(source) => {
                self.set_input_source(source);
            },
            SlaveMsg::UpdateInputSources => {
                self.get_mut_input_system();
            },
            SlaveMsg::ToggleDisplayInfo => {
                self.set_slave_info_displayed(!*self.get_slave_info_displayed());
            },
            SlaveMsg::InputReceived(event) => {
                match event {
                    InputSourceEvent::ButtonChanged(button, pressed) => {
                        if let Some(status_class) = SlaveStatusClass::from_button(button) {
                            if pressed {
                                self.set_target_status(&status_class, !(self.get_target_status(&status_class) != 0) as i16);
                            }
                        }
                    },
                    InputSourceEvent::AxisChanged(axis, value) => {
                        if let Some(status_class) = SlaveStatusClass::from_axis(axis) {
                            self.set_target_status(&status_class, value.saturating_mul(if axis == 1 || axis == 3 { -1 } else { 1 }));
                        }
                    },
                }
                if let Some(sender) = self.get_tcp_msg_sender() {
                    match sender.try_send(SlaveTcpMsg::ControlUpdated(ControlPacket::from_status_map(&self.get_status().lock().unwrap()))) {
                        Ok(_) => (),
                        Err(err) => println!("Cannot send control input: {}", err.to_string()),
                    }
                }
            },
            SlaveMsg::OpenFirmwareUpater => {
                match self.get_tcp_stream() {
                    Some(tcp_stream) => {
                        let component = MicroComponent::new(SlaveFirmwareUpdaterModel::new(Deref::deref(tcp_stream).clone()), sender.clone());
                        component.root_widget().set_transient_for(Some(&window.upgrade().unwrap()));
                    },
                    None => {
                        error_message("错误", "请确保下位机处于连接状态。", window.upgrade().as_ref());
                    },
                }
            },
            SlaveMsg::OpenParameterTuner => {
                match self.get_tcp_stream() {
                    Some(tcp_stream) => {
                        let component = MicroComponent::new(SlaveParameterTunerModel::new(*self.preferences.borrow().get_param_tuner_graph_view_point_num_limit()), sender.clone());
                        component.root_widget().set_transient_for(Some(&window.upgrade().unwrap()));
                        send!(component.sender(), SlaveParameterTunerMsg::StartDebug(Deref::deref(tcp_stream).clone()));
                    },
                    None => {
                        error_message("错误", "请确保下位机处于连接状态。", window.upgrade().as_ref());
                    },
                }
            },
            SlaveMsg::DestroySlave => {
                if let Some(polling) = self.get_polling() {
                    if *polling {
                        send!(self.video.sender(), SlaveVideoMsg::StopPipeline);
                    }
                }
                if let Some(connected) = self.get_connected() {
                    if *connected {
                        send!(sender, SlaveMsg::ToggleConnect);
                    }
                }
                send!(parent_sender, AppMsg::DestroySlave(self as *const Self));
            },
            SlaveMsg::ErrorMessage(msg) => {
                error_message("错误", &msg, window.upgrade().as_ref());
            },
            SlaveMsg::TcpError(msg) => {
                send!(sender, SlaveMsg::ShowToastMessage(format!("下位机通讯错误：{}", msg)));
                send!(sender, SlaveMsg::TcpConnectionChanged(None));
            },
            SlaveMsg::TcpConnectionChanged(tcp_stream) => {
                self.set_connected(Some(tcp_stream.is_some()));
                self.config.send(SlaveConfigMsg::SetConnected(Some(tcp_stream.is_some()))).unwrap();
                if tcp_stream.is_none() {
                    self.set_tcp_msg_sender(None);
                }
                self.set_tcp_stream(tcp_stream);
            },
            SlaveMsg::ShowToastMessage(msg) => {
                self.get_mut_toast_messages().borrow_mut().push_back(msg);
            },
	    SlaveMsg::ToggleRecord => {
                let video = &self.video;
                if video.model().get_record_handle().is_none() {
                    let mut pathbuf = self.preferences.borrow().get_video_save_path().clone();
                    pathbuf.push(format!("{}.mkv", DateTime::now_local().unwrap().format_iso8601().unwrap().replace(":", "-")));
                    send!(video.sender(), SlaveVideoMsg::StartRecord(pathbuf));
                } else {
                    send!(video.sender(), SlaveVideoMsg::StopRecord(None));
                }
                self.set_recording(None);
            },
            SlaveMsg::PollingChanged(polling) => {
                self.set_polling(Some(polling));
                send!(self.config.sender(), SlaveConfigMsg::SetPolling(Some(polling)));
                // send!(sender, SlaveMsg::InformationsReceived([("航向角".to_string(), "37°".to_string()), ("温度".to_string(), "25℃".to_string())].into_iter().collect())) // Debug
            },
            SlaveMsg::RecordingChanged(recording) => {
                if recording {
                    if *self.get_recording() == Some(false) {
                        self.set_sync_recording(true);
                    }
                } else {
                    self.set_sync_recording(false);
                }
                self.set_recording(Some(recording));
            },
            SlaveMsg::TakeScreenshot => {
                let mut pathbuf = self.preferences.borrow().get_image_save_path().clone();
                let format = self.preferences.borrow().get_image_save_format().clone();
                pathbuf.push(format!("{}.{}", DateTime::now_local().unwrap().format_iso8601().unwrap().replace(":", "-"), format.extension()));
                send!(self.video.sender(), SlaveVideoMsg::SaveScreenshot(pathbuf));
            },
            SlaveMsg::TcpMessage(msg) => {
                if let Some(sender) = self.get_tcp_msg_sender().as_ref() {
                    sender.try_send(msg).unwrap_or_default();
                }
            },
            SlaveMsg::InformationsReceived(info_map) => {
                let infos = self.get_mut_infos();
                infos.clear();
                for (key, value) in info_map.into_iter() {
                    infos.push(SlaveInfoModel { key, value, ..Default::default() });
                }
            },
            SlaveMsg::SetConfigPresented(presented) => self.set_config_presented(presented),
            SlaveMsg::SetSlaveStatus(which, value) => {
                self.set_target_status(&which, value);
                if let Some(sender) = self.get_tcp_msg_sender() {
                    match sender.try_send(SlaveTcpMsg::ControlUpdated(ControlPacket::from_status_map(&self.get_status().lock().unwrap()))) {
                        Ok(_) => (),
                        Err(err) => println!("Cannot send updated status: {}", err.to_string()),
                    }
                }
            },
        }
    }
}

pub struct MyComponent<T: MicroModel> {
    pub component: MicroComponent<T>,
}

impl <Model> MyComponent<Model>
    where
Model::Widgets: MicroWidgets<Model> + 'static,
Model::Msg: 'static,
Model::Data: 'static,
Model: MicroModel + 'static,  {
    fn model(&self) -> std::cell::Ref<'_, Model> {
        self.component.model().unwrap()
    }
    #[allow(dead_code)]
    fn model_mut(&self) -> std::cell::RefMut<'_, Model> {
        self.component.model_mut().unwrap()
    }
    #[allow(dead_code)]
    fn widgets(&self) -> std::cell::RefMut<'_, Model::Widgets> {
        self.component.widgets().unwrap()
    }
}

impl <T: MicroModel> std::ops::Deref for MyComponent<T> {
    type Target = MicroComponent<T>;
    fn deref(&self) -> &MicroComponent<T> {
        &self.component
    }
}


impl <T: MicroModel> Debug for MyComponent<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MyComponent").finish()
    }
}

impl <Model> Default for MyComponent<Model>
where
    Model::Widgets: MicroWidgets<Model> + 'static,
    Model::Msg: 'static,
    Model::Data: Default + 'static,
    Model: MicroModel + Default + 'static, {
    fn default() -> Self {
        MyComponent { component: MicroComponent::new(Model::default(), Model::Data::default()) }
    }
}

impl <Model> MyComponent<Model>
where
    Model::Widgets: MicroWidgets<Model> + 'static,
    Model::Msg: 'static,
    Model::Data: 'static,
    Model: MicroModel + 'static, {
    pub fn new(model: Model, data: Model::Data) -> MyComponent<Model> {
        MyComponent { component: MicroComponent::new(model, data) }
    }
}

impl FactoryPrototype for MyComponent<SlaveModel> {
    type Factory = FactoryVec<Self>;
    type Widgets = ToastOverlay;
    type Root = ToastOverlay;
    type View = Grid;
    type Msg = AppMsg;

    fn init_view(
        &self,
        _index: &usize,
        _sender: Sender<AppMsg>,
    ) -> ToastOverlay {
        self.component.root_widget().clone()
    }

    fn position(
        &self,
        index: &usize,
    ) -> GridPosition {
        let index = *index as i32;
        let row = index / 3;
        let column = index % 3;
        GridPosition {
            column,
            row,
            width: 1,
            height: 1,
        }
    }

    fn view(
        &self,
        _index: &usize,
        _widgets: &ToastOverlay,
    ) {
        self.component.update_view().unwrap();
    }

    fn root_widget(widgets: &ToastOverlay) -> &ToastOverlay {
        widgets
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ControlPacket {
    x: f32,
    y: f32,
    z: f32,
    rot: f32,
    depth_locked: bool,
    direction_locked: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct SlaveInfoPacket {
    info: HashMap<String, String>,
}

impl ControlPacket {
    pub fn from_status_map(status_map: &HashMap<SlaveStatusClass, i16>) -> ControlPacket {
        fn map_value(value: &i16) -> f32 {
            match *value {
                0 => 0.0,
                1..=i16::MAX => *value as f32 / i16::MAX as f32,
                i16::MIN..=-1 =>  *value as f32 / i16::MIN as f32 * -1.0,
            }
        }
        ControlPacket {
            x                : map_value(status_map.get(&SlaveStatusClass::MotionX).unwrap_or(&0)),
            y                : map_value(status_map.get(&SlaveStatusClass::MotionY).unwrap_or(&0)),
            z                : map_value(status_map.get(&SlaveStatusClass::MotionZ).unwrap_or(&0)),
            rot              : map_value(status_map.get(&SlaveStatusClass::MotionRotate).unwrap_or(&0)),
            depth_locked     : status_map.get(&SlaveStatusClass::DepthLocked).map(|x| *x >= 1).unwrap_or(false),
            direction_locked : status_map.get(&SlaveStatusClass::DirectionLocked).map(|x| *x >= 1).unwrap_or(false),
        }
    }
}

impl ToString for ControlPacket {
    fn to_string(&self) -> String {
        serde_json::to_string_pretty(self).unwrap()
    }
}
