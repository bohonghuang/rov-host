use std::{cell::{Cell, RefCell}, net::Ipv4Addr, path::PathBuf, rc::Rc, str::FromStr};

use fragile::Fragile;
use glib::{MainContext, Object, Sender, Type, clone};

use gstreamer as gst;
use gst::{Pipeline, prelude::*};
use gtk4 as gtk;
use gtk::{AboutDialog, Align, ApplicationWindow, Box as GtkBox, Button, CenterBox, Dialog, DialogFlags, Entry, Frame, Grid, Image, Inhibit, Label, MenuButton, Orientation, ResponseType, SpinButton, Stack, StringList, gdk_pixbuf::Pixbuf, gio::{Menu, MenuItem}, prelude::*};

use adw::{ActionRow, ComboRow, HeaderBar, PreferencesGroup, PreferencesPage, PreferencesWindow, StatusPage, Window, prelude::*};

use relm4::{AppUpdate, ComponentUpdate, Components, Model, RelmApp, RelmComponent, Widgets, actions::{ActionGroupName, ActionName, RelmAction, RelmActionGroup}, factory::{DynamicIndex, FactoryPrototype, FactoryVec, FactoryVecDeque, positions::GridPosition}, new_action_group, new_statful_action, new_statless_action, send};
use relm4_macros::widget;

use strum::IntoEnumIterator;
use strum_macros::{EnumIter, EnumString as EnumFromString, Display as EnumToString};

use lazy_static::lazy_static;

use crate::{AppModel, preferences::{PreferencesMsg, PreferencesModel}, video::{self, MatExt}};
use crate::AppMsg;
use crate::prelude::ObjectExt;

use derivative::*;

lazy_static! {
    pub static ref COMPONENTS: Fragile<RefCell<Vec<SlaveComponents>>> = Fragile::new(RefCell::new(Vec::new()));
}

#[tracker::track(pub)]
#[derive(Debug, Derivative, PartialEq)]
#[derivative(Default)]
pub struct SlaveModel {
    pub index: usize, 
    pub config: Rc<RefCell<SlaveConfigModel>>,
    #[derivative(Default(value="Some(false)"))]
    pub connected: Option<bool>,
    #[derivative(Default(value="Some(false)"))]
    pub polling: Option<bool>,
    pub preferences: Rc<RefCell<PreferencesModel>>,
}

impl SlaveModel {
    pub fn new(index: usize, config: SlaveConfigModel, preferences: Rc<RefCell<PreferencesModel>>) -> Self {
        Self {
            index, 
            config: Rc::new(RefCell::new(config)),
            preferences,
            ..Default::default()
        }
    }
}
#[derive(EnumIter, EnumToString, EnumFromString, PartialEq, Clone, Debug)]
pub enum VideoAlgorithm {
    Algorithm1, Algorithm2, Algorithm3, Algorithm4
}

#[widget(pub)]
impl Widgets<SlaveModel, ()> for SlaveWidgets {
    view! {
        vbox = GtkBox {
            put_data: args!("sender", sender.clone()),
            set_orientation: Orientation::Vertical,
            set_margin_start: 1,
            set_margin_end: 1, 
            append = &CenterBox {
                set_css_classes: &["toolbar"],
                set_orientation: Orientation::Horizontal,
                set_start_widget = Some(&GtkBox) {
                    set_hexpand: true,
                    set_halign: Align::Start,
                    set_spacing: 1,
                    append = &Button {
                        set_icon_name?: watch!(model.connected.map(|x| if x { "network-offline-symbolic" } else { "network-transmit-symbolic" })),
                        set_sensitive: track!(model.changed(SlaveModel::connected()), model.connected !=None),
                        set_css_classes: &["circular"],
                        set_tooltip_text: Some("连接/断开连接"),
                        put_data: args!("index", model.index),
                        connect_clicked(sender) => move |button| {
                            send!(sender, SlaveMsg::SlaveToggleConnect(*button.get_data("index").unwrap()));
                        },
                    },
                    append = &Button {
                        set_icon_name?: watch!(model.polling.map(|x| if x { "media-playback-pause-symbolic" } else { "media-playback-start-symbolic" })),
                        set_sensitive: track!(model.changed(SlaveModel::polling()), model.polling !=None),
                        set_css_classes: &["circular"],
                        set_tooltip_text: Some("启动/停止视频"),
                        put_data: args!("index", model.index),
                        connect_clicked(sender) => move |button| {
                            send!(sender, SlaveMsg::SlaveTogglePolling(*button.get_data("index").unwrap()));
                        }
                    },
                },
                set_center_widget = Some(&GtkBox) {
                    set_hexpand: true,
                    set_halign: Align::Center,
                    set_spacing: 1,
                    append = &Label {
                        set_text: track!(model.changed(SlaveModel::config()), format!("{}:{}", model.config.borrow().get_ip(), model.config.borrow().get_port()).as_str()),
                    },
                },
                set_end_widget = Some(&GtkBox) {
                    set_hexpand: true,
                    set_halign: Align::End,
                    set_spacing: 1,
                    append = &Button {
                        set_icon_name: "emblem-system-symbolic",
                        set_css_classes: &["circular"],
                        set_tooltip_text: Some("机位设置"),
                        put_data: args!("sender", components.config.sender().clone()),
                        connect_clicked(sender) => move |button| {
                            let sender = button.get_data::<Sender<SlaveConfigMsg>>("sender").unwrap().clone();
                            send!(sender, SlaveConfigMsg::SetWindowPresented(true));
                        },
                    },
                },
            },
            append: components.video.root_widget(),
        }
    }
}

impl std::fmt::Debug for SlaveWidgets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.vbox.fmt(f)
    }
}

type SlaveMsg = AppMsg;

impl Model for SlaveModel {
    type Msg = SlaveMsg;
    type Widgets = SlaveWidgets;
    type Components = SlaveComponents;
}

impl FactoryPrototype for SlaveModel {
    type Factory = FactoryVec<Self>;
    type Widgets = SlaveWidgets;
    type Root = GtkBox;
    type View = Grid;
    type Msg = SlaveMsg;

    fn generate(
        &self,
        index: &usize,
        sender: Sender<SlaveMsg>,
    ) -> SlaveWidgets {
        let components = SlaveComponents::init_components(self, sender.clone());
        let widgets = Widgets::init_view(self, &components, sender.clone());
        COMPONENTS.get().borrow_mut().push(components);
        widgets
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

    fn update(
        &self,
        index: &usize,
        widgets: &SlaveWidgets,
    ) {
        SlaveWidgets::view(unsafe {
            let const_ptr = widgets as *const SlaveWidgets;
            let mut_ptr = const_ptr as *mut SlaveWidgets;
            &mut *mut_ptr
        }, self, widgets.vbox.get_data::<Sender<SlaveMsg>>("sender").unwrap().clone());
        
        if let Some(components) = COMPONENTS.get().borrow().get(*index) {
            components.config.send(SlaveConfigMsg::Dummy).unwrap();
            components.video.send(SlaveVideoMsg::Dummy).unwrap()
        }
    }

    fn get_root(widgets: &SlaveWidgets) -> &GtkBox {
        &widgets.vbox
    }
}

#[tracker::track(pub)]
#[derive(Debug, Derivative, PartialEq, Clone)]
#[derivative(Default)]
pub struct SlaveConfigModel {
    index: usize,
    pub window_presented: bool,
    #[derivative(Default(value="PreferencesModel::default().default_slave_ipv4_address"))]
    pub ip: Ipv4Addr,
    #[derivative(Default(value="PreferencesModel::default().default_slave_port"))]
    pub port: u16,
    #[derivative(Default(value="5600"))]
    pub video_port: u16,
    pub video_algorithms: Vec<VideoAlgorithm>,
}

impl SlaveConfigModel {
    pub fn new(index: usize, ip: Ipv4Addr, port: u16, video_port: u16) -> Self {
        Self {
            index, ip, port, video_port,
            ..Default::default()
        }
    }
}

impl Model for SlaveConfigModel {
    type Msg = SlaveConfigMsg;
    type Widgets = SlaveConfigWidgets;
    type Components = ();
}

pub enum SlaveConfigMsg {
    Dummy, 
    SetIp(Ipv4Addr),
    SetPort(u16),
    SetVideoPort(u16),
    SetWindowPresented(bool),
}

#[widget(pub)]
impl Widgets<SlaveConfigModel, SlaveModel> for SlaveConfigWidgets {
    view! {
        window = PreferencesWindow {
            set_title: Some("机位选项"),
            set_modal: true,
            set_visible: watch!(model.window_presented), //track!(model.changed(SlaveConfigModel::window_presented()), model.window_presented),
            set_can_swipe_back: true,
            add = &PreferencesPage {
                set_title: "视频",
                set_icon_name: Some("emblem-videos-symbolic"),
                add = &PreferencesGroup {
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
                add = &PreferencesGroup {
                    set_title: "拉流",
                    set_description: Some("从下位机拉取视频流的选项"),
                    add = &ActionRow {
                        set_title: "端口",
                        set_subtitle: "从指定本地端口拉取视频流",
                        add_suffix = &SpinButton::with_range(0.0, 65535.0, 1.0) {
                            set_value: track!(model.changed(SlaveConfigModel::video_port()), model.video_port as f64),
                            set_digits: 0,
                            set_valign: Align::Center,
                        }
                    }
                }
            },
            add = &PreferencesPage {
                set_title: "连接",
                set_icon_name: Some("network-transmit-receive-symbolic"),
                add = &PreferencesGroup {
                    set_title: "通讯",
                    set_description: Some("设置下位机的通讯选项"),
                    add = &ActionRow {
                        set_title: "地址",
                        set_subtitle: "下位机的内网地址",
                        add_suffix = &Entry {
                            set_text: track!(model.changed(SlaveConfigModel::ip()), model.ip.to_string().as_str()),
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
                        set_subtitle: "连接到下位机的指定端口",
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
            },
            connect_close_request(sender) => move |window| {
                send!(sender, SlaveConfigMsg::SetWindowPresented(false));
                Inhibit(false)
            }
        }
    }
}

impl ComponentUpdate<SlaveModel> for SlaveConfigModel {
    fn init_model(parent_model: &SlaveModel) -> Self {
        parent_model.config.borrow().clone()
    }

    fn update(
        &mut self,
        msg: SlaveConfigMsg,
        components: &(),
        sender: Sender<SlaveConfigMsg>,
        parent_sender: Sender<SlaveMsg>,
    ) {
        let mut config_updated = true;
        match msg {
            SlaveConfigMsg::SetIp(ip) => self.ip = ip,
            SlaveConfigMsg::SetPort(port) => self.port = port,
            SlaveConfigMsg::SetVideoPort(port) => self.video_port = port,
            SlaveConfigMsg::SetWindowPresented(presented) =>self.window_presented = presented,
            SlaveConfigMsg::Dummy =>config_updated = false,
        }
        if config_updated {
            send!(parent_sender, SlaveMsg::SlaveConfigUpdated(self.index, self.clone()));
        }
    }
}

#[tracker::track(pub)]
#[derive(Debug, Derivative, PartialEq)]
#[derivative(Default)]
pub struct SlaveVideoModel {
    index: usize, 
    #[no_eq]
    pub pixbuf: Option<Pixbuf>,
    #[no_eq]
    pub pipeline: Option<Pipeline>,
    pub config: Rc<RefCell<SlaveConfigModel>>,
    pub record_handle: Option<(gst::Pad, Vec<gst::Element>)>,
    pub preferences: Rc<RefCell<PreferencesModel>>, 
}

pub enum SlaveVideoMsg {
    Dummy,
    SetPipeline(Option<Pipeline>),
    SetPixbuf(Option<Pixbuf>),
    SetRecording(bool),
}

impl Model for SlaveVideoModel {
    type Msg = SlaveVideoMsg;
    type Widgets = SlaveVideoWidgets;
    type Components = ();
}

#[widget(pub)]
impl Widgets<SlaveVideoModel, SlaveModel> for SlaveVideoWidgets {
    view! {
        frame = Frame {
            set_child = Some(&Stack) {
                set_vexpand: true,
                set_hexpand: true,
                add_child = &StatusPage {
                    set_icon_name: Some("help-browser-symbolic"),
                    set_title: "无信号",
                    set_description: Some("请点击上方按钮启动视频拉流"),
                    set_visible: track!(model.changed(SlaveVideoModel::pixbuf()), model.pixbuf == None),
                },
                add_child = &Image {
                    set_hexpand: true,
                    set_vexpand: true,
                    set_from_pixbuf: track!(model.changed(SlaveVideoModel::pixbuf()), match &model.pixbuf {
                        Some(pixbuf) =>Some(&pixbuf),
                        None => None,
                    }),
                },
            },
        }
    }
}

impl ComponentUpdate<SlaveModel> for SlaveVideoModel {
    fn init_model(parent_model: &SlaveModel) -> Self {
        Self {
            index: *parent_model.get_index(),
            config: parent_model.get_config().clone(),
            preferences: parent_model.get_preferences().clone(),
            ..Default::default()
        }
    }

    fn update(
        &mut self,
        msg: SlaveVideoMsg,
        components: &(),
        sender: Sender<SlaveVideoMsg>,
        parent_sender: Sender<SlaveMsg>,
    ) {
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
            SlaveVideoMsg::Dummy => (),
            SlaveVideoMsg::SetPixbuf(pixbuf) => self.set_pixbuf(pixbuf),
            SlaveVideoMsg::SetRecording(recording) => {
                dbg!(recording);
                match &self.pipeline {
                    Some(pipeline) => {
                        if recording {
                            let mut pathbuf = PathBuf::from_str(self.preferences.borrow().get_video_save_path()).unwrap();
                            pathbuf.push(format!("{}.mkv", self.index + 1));
                            println!("{}", pathbuf.to_str().unwrap());
                            let elements = video::create_queue_to_file(pathbuf.to_str().unwrap()).unwrap();
                            let pad = video::connect_elements_to_pipeline(pipeline, &elements).unwrap();
                            pipeline.set_state(gst::State::Playing).unwrap(); // 添加元素后会自动暂停，需要手动重新开始播放
                            self.record_handle = Some((pad, Vec::from(elements)));
                        } else {
                            match &self.record_handle {
                                Some((teepad, elements)) => {
                                    video::disconnect_elements_to_pipeline(pipeline, teepad, elements).unwrap();
                                },
                                None => {},
                            }
                        }
                    },
                    None => {},
                }
            },
        }
    }
}

pub struct SlaveComponents {
    pub config: RelmComponent<SlaveConfigModel, SlaveModel>,
    pub video: RelmComponent<SlaveVideoModel, SlaveModel>,
}

impl Components<SlaveModel> for SlaveComponents {
    fn init_components(parent_model: &SlaveModel, parent_sender: Sender<SlaveMsg>)  -> Self {
        Self {
            config: RelmComponent::new(parent_model, parent_sender.clone()),
            video: RelmComponent::new(parent_model, parent_sender.clone()),
        }
    }

    fn connect_parent(&mut self, _parent_widgets: &SlaveWidgets) {
        self.config.connect_parent(_parent_widgets);
        self.video.connect_parent(_parent_widgets);
    }
}
