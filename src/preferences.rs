use std::{fs, net::Ipv4Addr, path::PathBuf, str::FromStr};

use glib::{Sender, clone};

use gtk::{Adjustment, Align, ApplicationWindow, Box as GtkBox, Button, Dialog, Entry, FileChooser, FileChooserDialog, Inhibit, Label, ListBox, ListBoxRow, MapListModel, Orientation, ResponseType, ScrolledWindow, SelectionModel, SpinButton, StringList, Switch, Viewport, Window, prelude::*};

use adw::{ActionRow, ComboRow, EnumListModel, PreferencesGroup, PreferencesPage, PreferencesWindow, prelude::*};

use relm4::{AppUpdate, ComponentUpdate, Components, Model, RelmApp, RelmComponent, Widgets, actions::{ActionGroupName, ActionName, RelmAction, RelmActionGroup}, factory::FactoryVecDeque, send, new_action_group, new_stateful_action, new_stateless_action};
use relm4_macros::widget;

use strum::IntoEnumIterator;
use strum_macros::{EnumIter, EnumString as EnumFromString, Display as EnumToString};

use crate::{AppModel, AppMsg};

use derivative::*;

#[derive(EnumIter, EnumToString, EnumFromString, PartialEq, Clone, Debug)]
pub enum VideoEncoder {
    Copy, H264, H265, WEBM
}

impl Default for VideoEncoder {
    fn default() -> Self { Self::Copy }
}

fn get_data_path() -> PathBuf {
    const app_dir_name: &str = "rovhost";
    let mut data_path = dirs::data_local_dir().expect("无法找到本地数据文件夹");
    data_path.push(app_dir_name);
    if !data_path.exists() {
        fs::create_dir(data_path.clone()).expect("无法创建应用数据文件夹");
    }
    data_path
}

fn get_video_path() -> PathBuf {
    let mut video_path = get_data_path();
    video_path.push("Videos");
    if !video_path.exists() {
        fs::create_dir(video_path.clone()).expect("无法创建视频文件夹");
    }
    video_path
}

#[tracker::track]
#[derive(Derivative, Clone, PartialEq, Debug)]
#[derivative(Default)]
pub struct PreferencesModel {
    #[derivative(Default(value="0"))]
    pub initial_slave_num: u8,
    #[derivative(Default(value="String::from(get_video_path().to_str().unwrap())"))]
    pub video_save_path: String,
    #[derivative(Default(value="VideoEncoder::Copy"))]
    pub video_encoder: VideoEncoder,
    #[derivative(Default(value="Ipv4Addr::new(192, 168, 137, 219)"))]
    pub default_slave_ipv4_address: Ipv4Addr,
    #[derivative(Default(value="8888"))]
    pub default_slave_port: u16,
    #[derivative(Default(value="5600"))]
    pub default_local_video_port: u16,
}

pub enum PreferencesMsg {
    SetVideoSavePath(String),
    SetVideoEncoder(VideoEncoder),
    SetSlaveDefaultIpv4Address(Ipv4Addr),
    SetSlaveDefaultPort(u16),
    SetInitialSlaveNum(u8),
    SetDefaultLocalVideoPort(u16),
}

impl Model for PreferencesModel {
    type Msg = PreferencesMsg;
    type Widgets = PreferencesWidgets;
    type Components = ();
}

#[widget(pub)]
impl Widgets<PreferencesModel, AppModel> for PreferencesWidgets {
    view! {
        window = PreferencesWindow {
            set_title: Some("首选项"),
            set_transient_for: parent!(Some(&parent_widgets.app_window)),
            set_destroy_with_parent: true,
            set_modal: true,
            connect_close_request(sender) => move |window| {
                window.hide();
                Inhibit(true)
            },
            add = &PreferencesPage {
                set_title: "通用",
                set_icon_name: Some("view-grid-symbolic"),
                add = &PreferencesGroup {
                    set_title: "机位",
                    set_description: Some("配置上位机的多机位功能"),
                    add = &ActionRow {
                        set_title: "初始机位",
                        set_subtitle: "程序启动时的初始机位数量",
                        add_suffix = &SpinButton::with_range(0.0, 12.0, 1.0) {
                            set_value: track!(model.changed(PreferencesModel::initial_slave_num()), model.initial_slave_num as f64),
                            set_digits: 0,
                            set_valign: Align::Center,
                            connect_changed(sender) => move |button| {
                                send!(sender, PreferencesMsg::SetInitialSlaveNum(button.value() as u8));
                            }
                        }
                    }
                }
            },
            add = &PreferencesPage {
                set_title: "网络",
                set_icon_name: Some("network-transmit-receive-symbolic"),
                add = &PreferencesGroup {
                    set_description: Some("与机器人的连接通信设置"),
                    set_title: "连接",
                    add = &ActionRow {
                        set_title: "默认地址",
                        set_subtitle: "第一机位的机器人使用的默认IPV4地址，其他机位的地址将在该基础上进行累加",
                        add_suffix = &Entry {
                            set_text: track!(model.changed(PreferencesModel::default_slave_ipv4_address()), model.default_slave_ipv4_address.to_string().as_str()),
                            set_valign: Align::Center,
                            connect_changed(sender) => move |entry| {
                                match Ipv4Addr::from_str(&entry.text()) {
                                    Ok(addr) => send!(sender, PreferencesMsg::SetSlaveDefaultIpv4Address(addr)),
                                    Err(_) => (),
                                }
                            }
                         },
                    },
                    add = &ActionRow {
                        set_title: "默认端口",
                        set_subtitle: "连接机器人的默认端口",
                        add_suffix = &SpinButton::with_range(0.0, 65535.0, 1.0) {
                            set_value: track!(model.changed(PreferencesModel::default_slave_port()), model.default_slave_port as f64),
                            set_digits: 0,
                            set_valign: Align::Center,
                            connect_changed(sender) => move |button| {
                                send!(sender, PreferencesMsg::SetSlaveDefaultPort(button.value() as u16));
                            }
                        },
                    },
                },
            },
            
            add = &PreferencesPage {
                set_title: "视频",
                set_icon_name: Some("emblem-videos-symbolic"),
                add = &PreferencesGroup {
                    set_title: "拉流",
                    set_description: Some("从下位机拉取视频流的选项"),
                    add = &ActionRow {
                        set_title: "默认端口",
                        set_subtitle: "拉取第一个机位的视频流使用的默认本地端口，其他机位的端口将在该基础上进行累加",
                        add_suffix = &SpinButton::with_range(0.0, 65535.0, 1.0) {
                            set_value: track!(model.changed(PreferencesModel::default_local_video_port()), model.default_local_video_port as f64),
                            set_digits: 0,
                            set_valign: Align::Center,
                            connect_changed(sender) => move |button| {
                                send!(sender, PreferencesMsg::SetDefaultLocalVideoPort(button.value() as u16));
                            }
                        },
                    }
                },
                add = &PreferencesGroup {
                    set_title: "录制",
                    set_description: Some("视频流的录制选项"),
                    add = &ActionRow {
                        set_title: "视频保存目录",
                        set_subtitle: track!(model.changed(PreferencesModel::video_save_path()), model.video_save_path.as_str()),
                        set_activatable: true,
                        connect_activated(sender) => move |row| {
                            
                        }
                    },
                    add = &ComboRow {
                        set_title: "编码器",
                        set_subtitle: "视频录制时使用的编码器",
                        set_model: Some(&{
                            let model = StringList::new(&[]);
                            for value in VideoEncoder::iter() {
                                model.append(&value.to_string());
                            }
                            model
                        }),
                        set_selected: track!(model.changed(PreferencesModel::video_encoder()), VideoEncoder::iter().position(|x| x == model.video_encoder).unwrap() as u32),
                        connect_activated(sender) => move |row| {
                            send!(sender, PreferencesMsg::SetVideoEncoder(VideoEncoder::iter().nth(row.selected() as usize).unwrap()))
                        }
                    }
                }
            },
        }
    }
}

impl ComponentUpdate<AppModel> for PreferencesModel {
    fn init_model(parent_model: &AppModel) -> Self {
        parent_model.preferences.borrow().clone()
    }
    fn update(
        &mut self,
        msg: PreferencesMsg,
        components: &(),
        sender: Sender<PreferencesMsg>,
        parent_sender: Sender<AppMsg>,
    ) {
        match msg {
            PreferencesMsg::SetVideoSavePath(path) => self.video_save_path = path,
            PreferencesMsg::SetVideoEncoder(encoder) => self.video_encoder = encoder,
            PreferencesMsg::SetSlaveDefaultIpv4Address(address) => self.default_slave_ipv4_address = address,
            PreferencesMsg::SetSlaveDefaultPort(port) => self.default_slave_port = port,
            PreferencesMsg::SetInitialSlaveNum(num) => self.initial_slave_num = num,
            PreferencesMsg::SetDefaultLocalVideoPort(port) => self.default_local_video_port = port,
        }
        send!(parent_sender, AppMsg::PreferencesUpdated(self.clone()));
    }
}

