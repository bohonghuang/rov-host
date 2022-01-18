use std::{cell::{Cell, RefCell}, collections::HashMap, net::Ipv4Addr, path::PathBuf, rc::Rc, str::FromStr, sync::{Arc, Mutex}, fmt::Debug};

use fragile::Fragile;
use glib::{MainContext, Object, PRIORITY_DEFAULT, Sender, Type, clone};

use gstreamer as gst;
use gst::{Pipeline, prelude::*};
use gtk::{AboutDialog, Align, ApplicationWindow, Box as GtkBox, Button, CenterBox, CheckButton, Dialog, DialogFlags, Entry, Frame, Grid, Image, Inhibit, Label, ListBox, MenuButton, Orientation, Overlay, Popover, ResponseType, Revealer, RevealerTransitionType, ScrolledWindow, SelectionModel, Separator, SingleSelection, SpinButton, Stack, StringList, Switch, ToggleButton, Viewport, gdk_pixbuf::Pixbuf, gio::{Menu, MenuItem, MenuModel}, prelude::*, Picture};

use adw::{ActionRow, ComboRow, HeaderBar, PreferencesGroup, PreferencesPage, PreferencesWindow, StatusPage, Window, prelude::*};

use relm4::{AppUpdate, WidgetPlus, ComponentUpdate, Components, Model, RelmApp, RelmComponent, Widgets, actions::{ActionGroupName, ActionName, RelmAction, RelmActionGroup}, factory::{DynamicIndex, FactoryPrototype, FactoryVec, FactoryVecDeque, positions::GridPosition}, new_action_group, send, MicroWidgets, MicroModel, MicroComponent};
use relm4_macros::{widget, micro_widget};

use strum::IntoEnumIterator;
use strum_macros::{EnumIter, EnumString as EnumFromString, Display as EnumToString};

use lazy_static::lazy_static;

use crate::{AppModel, input::{InputEvent, InputSource, InputSourceEvent, InputSystem}, preferences::{PreferencesMsg, PreferencesModel}, video::{self, MatExt}};
use crate::AppMsg;
use crate::prelude::ObjectExt;

use derivative::*;

#[tracker::track(pub)]
#[derive(Debug, Derivative)]
#[derivative(Default)]
pub struct SlaveModel {
    #[no_eq]
    #[derivative(Default(value="MyComponent::new(Default::default(), MainContext::channel(PRIORITY_DEFAULT).0)"))]
    pub config: MyComponent<SlaveConfigModel>,
    #[no_eq]
    pub video: MyComponent<SlaveVideoModel>,
    #[derivative(Default(value="Some(false)"))]
    pub connected: Option<bool>,
    #[derivative(Default(value="Some(false)"))]
    pub polling: Option<bool>,
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
    pub fn new(config: MyComponent<SlaveConfigModel>, preferences: Rc<RefCell<PreferencesModel>>, input_event_sender: Sender<InputSourceEvent>) -> Self {
        Self {
            config,
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
        let mut status = self.status.lock().unwrap();
        *status.entry(status_class.clone()).or_insert(0) = new_status;
    }
    
}
#[derive(EnumIter, EnumToString, EnumFromString, PartialEq, Clone, Debug)]
pub enum VideoAlgorithm {
    Algorithm1, Algorithm2, Algorithm3, Algorithm4
}

pub fn input_sources_list_box(input_source: &Option<InputSource>, input_system: &InputSystem, sender: &Sender<SlaveMsg>) -> ListBox {
    let sources = input_system.get_sources().unwrap();
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
    list_box
}

#[micro_widget(pub)]
impl MicroWidgets<SlaveModel> for SlaveWidgets {
    view! {
        vbox = GtkBox {
            put_data: args!("sender", sender.clone()),
            set_orientation: Orientation::Vertical,
            append = &CenterBox {
                set_css_classes: &["toolbar"],
                set_orientation: Orientation::Horizontal,
                set_start_widget = Some(&GtkBox) {
                    set_hexpand: true,
                    set_halign: Align::Start,
                    set_spacing: 5,
                    append = &Button {
                        set_icon_name?: watch!(model.connected.map(|x| if x { "network-offline-symbolic" } else { "network-transmit-symbolic" })),
                        set_sensitive: track!(model.changed(SlaveModel::connected()), model.connected !=None),
                        set_css_classes: &["circular"],
                        set_tooltip_text: Some("连接/断开连接"),
                        connect_clicked(sender) => move |button| {
                            send!(sender, SlaveMsg::ToggleConnect);
                        },
                    },
                    append = &Button {
                        set_icon_name?: watch!(model.polling.map(|x| if x { "media-playback-pause-symbolic" } else { "media-playback-start-symbolic" })),
                        set_sensitive: track!(model.changed(SlaveModel::polling()), model.polling !=None),
                        set_css_classes: &["circular"],
                        set_tooltip_text: Some("启动/停止视频"),
                        connect_clicked(sender) => move |button| {
                            send!(sender, SlaveMsg::TogglePolling);
                        },
                    },
                },
                set_center_widget = Some(&GtkBox) {
                    set_hexpand: true,
                    set_halign: Align::Center,
                    set_spacing: 1,
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
                                        set_text: "输入设备"
                                    },
                                    set_end_widget = Some(&Button) {
                                        set_icon_name: "view-refresh-symbolic",
                                        set_css_classes: &["circular"],
                                        set_tooltip_text: Some("刷新输入设备"),
                                        connect_clicked(sender) => move |button| {
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
                    set_spacing: 1,
                    append = &ToggleButton {
                        set_icon_name: "emblem-system-symbolic",
                        set_css_classes: &["circular"],
                        set_tooltip_text: Some("机位设置"),
                        put_data: args!("sender", model.config.sender().clone()),
                        connect_active_notify => move |button| {
                            let sender = button.get_data::<Sender<SlaveConfigMsg>>("sender").unwrap().clone();
                            send!(sender, SlaveConfigMsg::TogglePresented);
                        },
                    },
                },
            },
            append = &GtkBox {
                set_orientation: Orientation::Horizontal,
                append = &Overlay {
                    set_child: Some(model.video.root_widget()),
                    add_overlay = &GtkBox {
                        set_valign: Align::Start,
                        set_halign: Align::End,
                        set_hexpand: true,
                        set_margin_all: 20, 
                        append = &Frame {
                            set_css_classes: &["card"],
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
                                            set_text: "机位信息",
                                        },
                                        set_end_widget = Some(&Image) {
                                            set_icon_name: watch!(Some(if model.slave_info_displayed { "go-down-symbolic" } else { "go-next-symbolic" })),
                                        },
                                    },
                                    connect_clicked(sender) => move |button| {
                                        send!(sender, SlaveMsg::ToggleDisplayInfo);
                                    },
                                },
                                append = &Revealer {
                                    set_reveal_child: watch!(model.slave_info_displayed),
                                    set_child = Some(&GtkBox) {
                                        set_spacing: 2,
                                        set_margin_all: 5,
                                        set_orientation: Orientation::Vertical,
                                        set_halign: Align::Center,
                                        append = &Frame {
                                            set_hexpand: true,
                                            set_halign: Align::Center,
                                            set_child = Some(&Grid) {
                                                set_margin_all: 2,
                                                set_row_spacing: 2,
                                                set_column_spacing: 2,
                                                attach(0, 0, 1, 1) = &ToggleButton {
                                                    set_icon_name: "object-rotate-left-symbolic",
                                                    set_active: watch!(model.get_target_status(&SlaveStatusClass::MotionRotate) < -JOYSTICK_DISPLAY_THRESHOLD),
                                                },
                                                attach(2, 0, 1, 1) = &ToggleButton {
                                                    set_icon_name: "object-rotate-right-symbolic",
                                                    set_active: watch!(model.get_target_status(&SlaveStatusClass::MotionRotate) > JOYSTICK_DISPLAY_THRESHOLD),
                                                },
                                                attach(0, 2, 1, 1) = &ToggleButton {
                                                    set_icon_name: "go-bottom-symbolic",
                                                    set_active: watch!(model.get_target_status(&SlaveStatusClass::MotionZ) < -JOYSTICK_DISPLAY_THRESHOLD),
                                                },
                                                attach(2, 2, 1, 1) = &ToggleButton {
                                                    set_icon_name: "go-top-symbolic",
                                                    set_active: watch!(model.get_target_status(&SlaveStatusClass::MotionZ) > JOYSTICK_DISPLAY_THRESHOLD),
                                                },
                                                attach(1, 0, 1, 1) = &ToggleButton {
                                                    set_icon_name: "go-up-symbolic",
                                                    set_active: watch!(model.get_target_status(&SlaveStatusClass::MotionY) > JOYSTICK_DISPLAY_THRESHOLD),
                                                },
                                                attach(0, 1, 1, 1) = &ToggleButton {
                                                    set_icon_name: "go-previous-symbolic",
                                                    set_active: watch!(model.get_target_status(&SlaveStatusClass::MotionX) < -JOYSTICK_DISPLAY_THRESHOLD),
                                                },
                                                attach(2, 1, 1, 1) = &ToggleButton {
                                                    set_icon_name: "go-next-symbolic",
                                                    set_active: watch!(model.get_target_status(&SlaveStatusClass::MotionX) > JOYSTICK_DISPLAY_THRESHOLD),
                                                },
                                                attach(1, 2, 1, 1) = &ToggleButton {
                                                    set_icon_name: "go-down-symbolic",
                                                    set_active: watch!(model.get_target_status(&SlaveStatusClass::MotionY) < -JOYSTICK_DISPLAY_THRESHOLD),
                                                },
                                            },
                                        },
                                        append = &Label {
                                            set_halign: Align::Start,
                                            set_text: "机位信息 1",
                                        },
                                        append = &Label {
                                            set_halign: Align::Start,
                                            set_text: "机位信息 2",
                                        },
                                        append = &Label {
                                            set_halign: Align::Start,
                                            set_text: "机位信息 3",
                                        },
                                        append = &Label {
                                            set_halign: Align::Start,
                                            set_text: "机位信息 4",
                                        },
                                        append = &Label {
                                            set_halign: Align::Start,
                                            set_text: "机位信息 5",
                                        },
                                        append = &CenterBox {
                                            set_hexpand: true,
                                            set_start_widget = Some(&Label) {
                                                set_text: "深度锁定",
                                            },
                                            set_end_widget = Some(&Switch) {
                                                set_active: watch!(model.get_target_status(&SlaveStatusClass::DepthLocked) != 0),
                                            },
                                        },
                                        append = &CenterBox {
                                            set_hexpand: true,
                                            set_start_widget = Some(&Label) {
                                                set_text: "方向锁定",
                                            },
                                            set_end_widget = Some(&Switch) {
                                                set_active: watch!(model.get_target_status(&SlaveStatusClass::DirectionLocked) != 0),
                                            },
                                        },
                                    },
                                },
                            }
                        }
                    }
                }, 
                append: model.config.root_widget(),
            },
        }
    }
}

impl std::fmt::Debug for SlaveWidgets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.vbox.fmt(f)
    }
}

pub enum SlaveMsg {
    ConfigUpdated,
    ToggleConnect,
    TogglePolling,
    SetInputSource(Option<InputSource>),
    UpdateInputSources,
    ToggleDisplayInfo,
    InputReceived(InputSourceEvent),
}

impl MicroModel for SlaveModel {
    type Msg = SlaveMsg;
    type Widgets = SlaveWidgets;
    type Data = ();
    fn update(&mut self, msg: SlaveMsg, data: &(), sender: Sender<SlaveMsg>) {
        match msg {
            SlaveMsg::ConfigUpdated => {
                self.get_mut_config();
            },
            SlaveMsg::ToggleConnect => {
                self.set_connected(None);
            },
            SlaveMsg::TogglePolling => {
                match self.get_polling() {
                    Some(true) =>{
                        self.video.send(SlaveVideoMsg::SetPipeline(None)).unwrap();
                        self.set_polling(Some(false));
                    },
                    Some(false) => {
                        self.video.send(SlaveVideoMsg::SetPipeline(Some(video::create_pipeline(*self.get_config().model().get_video_port()).unwrap()))).unwrap();
                        self.set_polling(Some(true));
                    },
                    None => (),
                }
            },
            SlaveMsg::SetInputSource(source) => {
                self.set_input_source(source);
            },
            SlaveMsg::UpdateInputSources => {
                self.set_input_system(self.get_input_system().clone());
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
                self.set_status(self.get_status().clone());
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
    fn model_mut(&self) -> std::cell::RefMut<'_, Model> {
        self.component.model_mut().unwrap()
    }
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
    type Widgets = GtkBox;
    type Root = GtkBox;
    type View = Grid;
    type Msg = AppMsg;

    fn init_view(
        &self,
        index: &usize,
        sender: Sender<AppMsg>,
    ) -> GtkBox {
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
        index: &usize,
        widgets: &GtkBox,
    ) {
        self.component.update_view().unwrap();
    }

    fn root_widget(widgets: &GtkBox) -> &GtkBox {
        widgets
    }
}

#[tracker::track(pub)]
#[derive(Debug, Derivative, PartialEq, Clone)]
#[derivative(Default)]
pub struct SlaveConfigModel {
    pub presented: bool,
    #[derivative(Default(value="PreferencesModel::default().default_slave_ipv4_address"))]
    pub ip: Ipv4Addr,
    #[derivative(Default(value="PreferencesModel::default().default_slave_port"))]
    pub port: u16,
    #[derivative(Default(value="5600"))]
    pub video_port: u16,
    pub video_algorithms: Vec<VideoAlgorithm>,
}

impl SlaveConfigModel {
    pub fn new(ip: Ipv4Addr, port: u16, video_port: u16) -> Self {
        Self {
            ip, port, video_port,
            ..Default::default()
        }
    }
}

impl MicroModel for SlaveConfigModel {
    type Msg = SlaveConfigMsg;
    type Widgets = SlaveConfigWidgets;
    type Data = Sender<SlaveMsg>;
    fn update(&mut self, msg: SlaveConfigMsg, parent_sender: &Sender<SlaveMsg>, sender: Sender<SlaveConfigMsg>) {
         match msg {
             SlaveConfigMsg::SetIp(ip) => self.set_ip(ip),
             SlaveConfigMsg::SetPort(port) => self.set_port(port),
             SlaveConfigMsg::SetVideoPort(port) => self.set_video_port(port),
             SlaveConfigMsg::TogglePresented =>self.set_presented(!self.get_presented()),
         }
         send!(parent_sender, SlaveMsg::ConfigUpdated);
    }
}

impl std::fmt::Debug for SlaveConfigWidgets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.root_widget().fmt(f)
    }
}

pub enum SlaveConfigMsg {
    SetIp(Ipv4Addr),
    SetPort(u16),
    SetVideoPort(u16),
    TogglePresented,
}

#[micro_widget(pub)]
impl MicroWidgets<SlaveConfigModel> for SlaveConfigWidgets {
    view! {
        window = Revealer {
            set_reveal_child: watch!(model.presented), //track!(model.changed(SlaveConfigModel::window_presented()), model.window_presented),
            set_transition_type: RevealerTransitionType::SlideLeft,
            set_child = Some(&GtkBox) {
                set_orientation: Orientation::Horizontal,
                append = &Separator {
                    set_orientation: Orientation::Horizontal,
                },
                append = &ScrolledWindow {
                    set_width_request: 300,
                    set_child = Some(&Viewport) {
                        set_child = Some(&GtkBox) {
                            set_spacing: 20,
                            set_margin_all: 10,
                            set_orientation: Orientation::Vertical,
                            append = &PreferencesGroup {
                                set_title: "通讯",
                                set_description: Some("设置下位机的通讯选项"),
                                add = &ActionRow {
                                    set_title: "地址",
                                    set_subtitle: "下位机的内网地址",
                                    add_suffix = &Entry {
                                        set_text: model.ip.to_string().as_str(), //track!(model.changed(SlaveConfigModel::ip()), model.ip.to_string().as_str()),
                                        set_valign: Align::Center,
                                        connect_changed(sender) => move |entry| {
                                            match Ipv4Addr::from_str(&entry.text()) {
                                                Ok(addr) => send!(sender, SlaveConfigMsg::SetIp(addr)),
                                                Err(_) => (),
                                            }
                                        }
                                    },
                                },
                                add = &ActionRow {
                                    set_title: "端口",
                                    set_subtitle: "下位机的通讯端口",
                                    add_suffix = &SpinButton::with_range(0.0, 65535.0, 1.0) {
                                        set_value: track!(model.changed(SlaveConfigModel::port()), model.port as f64),
                                        set_digits: 0,
                                        set_valign: Align::Center,
                                        connect_changed(sender) => move |button| {
                                            send!(sender, SlaveConfigMsg::SetPort(button.value() as u16));
                                        }
                                    },
                                },
                            },
                            append = &PreferencesGroup {
                                set_title: "画面",
                                set_description: Some("上位机端对画面进行的处理选项"),
                                add = &ComboRow {
                                    set_title: "增强算法",
                                    set_subtitle: "对画面使用的增强算法",
                                    set_model: Some(&{
                                        let model = StringList::new(&[]);
                                        model.append("无");
                                        for value in VideoAlgorithm::iter() {
                                            model.append(&value.to_string());
                                        }
                                        model
                                    }),
                                    set_selected: track!(model.changed(PreferencesModel::video_encoder()), VideoAlgorithm::iter().position(|x| model.video_algorithms.first().map_or_else(|| false, |y| *y == x)).map_or_else(|| 0, |x| x + 1) as u32),
                                    connect_activated(sender) => move |row| {
                                        
                                    }
                                }
                            },
                            append = &PreferencesGroup {
                                set_title: "拉流",
                                set_description: Some("从下位机拉取视频流的选项"),
                                add = &ActionRow {
                                    set_title: "端口",
                                    set_subtitle: "拉取视频流的本地端口",
                                    add_suffix = &SpinButton::with_range(0.0, 65535.0, 1.0) {
                                        set_value: track!(model.changed(SlaveConfigModel::video_port()), model.video_port as f64),
                                        set_digits: 0,
                                        set_valign: Align::Center,
                                        connect_changed(sender) => move |button| {
                                            send!(sender, SlaveConfigMsg::SetVideoPort(button.value() as u16));
                                        }
                                    }
                                }
                            },
                        },
                    },
                },
            },
            // connect_close_request(sender) => move |window| {
            //     send!(sender, SlaveConfigMsg::SetWindowPresented(false));
            //     Inhibit(false)
            // },
        }
    }
}

#[tracker::track(pub)]
#[derive(Debug, Derivative, PartialEq)]
#[derivative(Default)]
pub struct SlaveVideoModel {
    #[no_eq]
    pub pixbuf: Option<Pixbuf>,
    #[no_eq]
    pub pipeline: Option<Pipeline>,
    pub config: Rc<RefCell<SlaveConfigModel>>,
    pub record_handle: Option<(gst::Pad, Vec<gst::Element>)>,
    pub preferences: Rc<RefCell<PreferencesModel>>, 
}

pub enum SlaveVideoMsg {
    SetPipeline(Option<Pipeline>),
    SetPixbuf(Option<Pixbuf>),
    StartRecord(String),
    StopRecord,
}

impl MicroModel for SlaveVideoModel {
    type Msg = SlaveVideoMsg;
    type Widgets = SlaveVideoWidgets;
    type Data = ();

    fn update(&mut self, msg: SlaveVideoMsg, data: &(), sender: Sender<SlaveVideoMsg>) {
        match msg {
            SlaveVideoMsg::SetPipeline(pipeline) => {
                match pipeline {
                    Some(pipeline) => {
                        if self.pipeline == None {
                            let sender = sender.clone();
                            let (mat_sender, mat_receiver) = MainContext::channel(glib::PRIORITY_DEFAULT);
                            video::attach_pipeline_callback(&pipeline, mat_sender).unwrap();
                            mat_receiver.attach(None, move |mat| {
                                sender.send(SlaveVideoMsg::SetPixbuf(Some(mat.as_pixbuf()))).unwrap();
                                Continue(true)
                            });
                            pipeline.set_state(gst::State::Playing);
                            self.pipeline = Some(pipeline);
                        }
                    },
                    None => {
                        if let Some(pipeline) = &self.pipeline {
                            pipeline.set_state(gst::State::Null);
                            self.pipeline = None;
                        }
                    },
                }
            },
            SlaveVideoMsg::SetPixbuf(pixbuf) => self.set_pixbuf(pixbuf),
            SlaveVideoMsg::StartRecord(file_name) => {
                if let Some(pipeline) = &self.pipeline {
                    let mut pathbuf = PathBuf::from_str(self.preferences.borrow().get_video_save_path()).unwrap();
                    pathbuf.push(format!("{}.mkv", file_name));
                    println!("{}", pathbuf.to_str().unwrap());
                    let elements = video::create_queue_to_file(pathbuf.to_str().unwrap()).unwrap();
                    let pad = video::connect_elements_to_pipeline(pipeline, &elements).unwrap();
                    pipeline.set_state(gst::State::Playing).unwrap(); // 添加元素后会自动暂停，需要手动重新开始播放
                    self.record_handle = Some((pad, Vec::from(elements)));
                }
            },
            SlaveVideoMsg::StopRecord => {
                if let Some(pipeline) = &self.pipeline {
                    if let Some((teepad, elements)) = &self.record_handle{
                        video::disconnect_elements_to_pipeline(pipeline, teepad, elements).unwrap();
                    }
                }
            },
        }
    }
}

impl std::fmt::Debug for SlaveVideoWidgets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.root_widget().fmt(f)
    }
}

#[micro_widget(pub)]
impl MicroWidgets<SlaveVideoModel> for SlaveVideoWidgets {
    view! {
        frame = GtkBox {
            append = &Stack {
                set_vexpand: true,
                set_hexpand: true,
                add_child = &StatusPage {
                    set_icon_name: Some("help-browser-symbolic"),
                    set_title: "无信号",
                    set_description: Some("请点击上方按钮启动视频拉流"),
                    set_visible: track!(model.changed(SlaveVideoModel::pixbuf()), model.pixbuf == None),
                },
                add_child = &Picture {
                    set_hexpand: true,
                    set_vexpand: true,
                    set_can_shrink: true,
                    set_pixbuf: track!(model.changed(SlaveVideoModel::pixbuf()), match &model.pixbuf {
                        Some(pixbuf) => Some(&pixbuf),
                        None => None,
                    }),
                },
            },
        }
    }
}

