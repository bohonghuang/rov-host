use std::{cell::{Cell, RefCell}, net::Ipv4Addr, rc::Rc, str::FromStr};

use glib::{Object, Sender, Type, clone};

use gtk4 as gtk;
use gtk::{AboutDialog, Align, ApplicationWindow, Box as GtkBox, Button, CenterBox, Dialog, DialogFlags, Entry, Frame, Grid, Image, Inhibit, Label, MenuButton, Orientation, ResponseType, SpinButton, Stack, StringList, gio::{Menu, MenuItem}, prelude::*};

use adw::{ActionRow, ComboRow, HeaderBar, PreferencesGroup, PreferencesPage, PreferencesWindow, StatusPage, Window, prelude::*};

use relm4::{AppUpdate, ComponentUpdate, Components, Model, RelmApp, RelmComponent, Widgets, actions::{ActionGroupName, ActionName, RelmAction, RelmActionGroup}, factory::{DynamicIndex, FactoryPrototype, FactoryVec, FactoryVecDeque, positions::GridPosition}, new_action_group, new_statful_action, new_statless_action, send};
use relm4_macros::widget;

use strum::IntoEnumIterator;
use strum_macros::{EnumIter, EnumString as EnumFromString, Display as EnumToString};

use crate::{AppModel, preferences::{PreferencesMsg, PreferencesModel}};
use crate::AppMsg;
use crate::prelude::ObjectExt;

use derivative::*;

#[tracker::track(pub)]
#[derive(Debug, Derivative, PartialEq)]
#[derivative(Default)]
pub struct SlaveModel {
    pub index: usize, 
    pub config: SlaveConfigModel, 
    pub connected: Option<bool>,
    pub polling: Option<bool>,
}

impl SlaveModel {
    pub fn new(index: usize, config: SlaveConfigModel) -> Self {
        Self {
            index, 
            config,
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
                        set_icon_name: "network-transmit-symbolic",
                        set_css_classes: &["circular"],
                        set_tooltip_text: Some("连接/断开连接"),
                        
                    },
                    append = &Button {
                        set_icon_name: "media-playback-start-symbolic",
                        set_css_classes: &["circular"],
                        set_tooltip_text: Some("启动/停止视频"),
                    },
                },
                set_center_widget = Some(&GtkBox) {
                    set_hexpand: true,
                    set_halign: Align::Center,
                    set_spacing: 1,
                    append = &Label {
                        set_text: track!(model.changed(SlaveModel::config()), format!("{}:{}", model.config.ip, model.config.port).as_str()),
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
                        put_data: args!("index", model.index),
                        connect_clicked(sender) => move |button| {
                            if let Some(&index) = button.get_data("index") {
                                send!(sender, SlaveMsg::DisplaySlaveConfigWindow(index));
                            }
                        },
                    },
                },
            },
            append = &Frame {
                set_child = Some(&Stack) {
                    set_vexpand: true,
                    set_hexpand: true,
                    add_child = &StatusPage {
                        set_icon_name: Some("help-browser-symbolic"),
                        set_title: "无信号",
                        set_description: Some("请点击上方按钮启动视频拉流"),
                    },
                },
            },
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
        Widgets::init_view(self, &SlaveComponents::init_components(self, sender.clone()) , sender.clone())
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
    ) {}

    fn get_root(widgets: &SlaveWidgets) -> &GtkBox {
        &widgets.vbox
    }
}

impl Components<SlaveModel> for SlaveConfigModel {
    fn init_components(parent_model: &SlaveModel, parent_sender: Sender<SlaveMsg>)
        -> Self {
        todo!()
    }

    fn connect_parent(&mut self, _parent_widgets: &SlaveWidgets) {
        todo!()
    }
}

#[tracker::track(pub)]
#[derive(Debug, Derivative, PartialEq)]
#[derivative(Default)]
pub struct SlaveConfigModel {
    pub window_presented: bool,
    #[derivative(Default(value="PreferencesModel::default().default_slave_ipv4_address"))]
    pub ip: Ipv4Addr,
    #[derivative(Default(value="PreferencesModel::default().default_slave_port"))]
    pub port: u16,
    pub video_port: u16,
    pub video_algorithms: Vec<VideoAlgorithm>,
}

impl SlaveConfigModel {
    pub fn new(ip: Ipv4Addr, port: u16) -> Self {
        Self {
            ip, port,
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
            set_visible: true,//true,//track!(model.changed(SlaveConfigModel::window_presented()), model.window_presented),
            set_can_swipe_back: true,
            add = &PreferencesPage {
                set_title: "视频",
                set_icon_name: Some("view-grid-symbolic"),
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
                set_icon_name: Some("view-grid-symbolic"),
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
        Self {
            ..Default::default()
        }
    }

    fn update(
        &mut self,
        msg: SlaveConfigMsg,
        components: &(),
        sender: Sender<SlaveConfigMsg>,
        parent_sender: Sender<SlaveMsg>,
    ) {
        match msg {
            SlaveConfigMsg::SetIp(ip) => self.ip = ip,
            SlaveConfigMsg::SetPort(port) => self.port = port,
            SlaveConfigMsg::SetVideoPort(port) => self.video_port = port,
            SlaveConfigMsg::SetWindowPresented(presented) => self.window_presented = presented,
        }
    }
}

pub struct SlaveComponents {
    config: RelmComponent<SlaveConfigModel, SlaveModel>, 
}

impl Components<SlaveModel> for SlaveComponents {
    fn init_components(parent_model: &SlaveModel, parent_sender: Sender<SlaveMsg>)  -> Self {
        Self {
            config: RelmComponent::new(parent_model, parent_sender.clone()),
        }
    }

    fn connect_parent(&mut self, _parent_widgets: &SlaveWidgets) {
        self.config.connect_parent(_parent_widgets);
    }
}
