use std::fs;

use glib::{Sender, clone};

use gtk4 as gtk;
use gtk::{Adjustment, Align, ApplicationWindow, Box as GtkBox, Button, Dialog, Entry, FileChooser, FileChooserDialog, Inhibit, Label, ListBox, ListBoxRow, MapListModel, Orientation, ResponseType, ScrolledWindow, SelectionModel, SpinButton, StringList, Switch, Viewport, Window, prelude::*};

use adw::{ActionRow, ComboRow, ComboRowBuilder, EnumListModel, PreferencesGroup, PreferencesPage, PreferencesWindow, prelude::*};

use relm4::{AppUpdate, ComponentUpdate, Components, Model, RelmApp, RelmComponent, Widgets, actions::{ActionGroupName, ActionName, RelmAction, RelmActionGroup}, factory::FactoryVecDeque, send, new_action_group, new_statful_action, new_statless_action};
use relm4_macros::widget;

use strum::IntoEnumIterator;
use strum_macros::{EnumIter, EnumString as EnumFromString, Display as EnumToString};

use crate::{AppModel, AppMsg};

#[derive(EnumIter, EnumToString, EnumFromString, PartialEq)]
pub enum VideoEncoder {
    Copy, H264, H265, WEBM
}

impl Default for VideoEncoder {
    fn default() -> Self { Self::Copy }
}

#[tracker::track]
#[derive(Default)]
pub struct PreferencesModel {
    video_save_path: String,
    video_encoder: VideoEncoder,
    default_slave_ipv4_address: String,
    default_slave_port: u16,
}

pub enum PreferencesMsg {
    SetVideoSavePath(String),
    SetVideoEncoder(VideoEncoder),
    SetSlaveDefaultIPV4Address(String),
    SetSlaveDefaultPort(u16),
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
            set_modal: true,
            set_visible: true,
            set_can_swipe_back: true,
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
                            set_text: track!(model.changed(PreferencesModel::default_slave_ipv4_address()), model.default_slave_ipv4_address.as_str()),
                            set_valign: Align::Center,
                            connect_changed(sender) => move |entry| {
                                send!(sender, PreferencesMsg::SetSlaveDefaultIPV4Address(String::from(entry.text())));
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
                        connect_selected_notify(sender) => move |row| {
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
        let app_dir_name = "RovHost";
        let mut data_path = dirs::data_local_dir().expect("无法找到本地数据文件夹");
        data_path.push(app_dir_name);
        if !data_path.exists() {
            fs::create_dir(data_path.clone()).expect("无法创建应用数据文件夹");
        }
        let mut video_path = data_path.clone();
        video_path.push("Videos");
        if !video_path.exists() {
            fs::create_dir(video_path.clone()).expect("无法创建视频文件夹");
        }
        Self {
            video_save_path: video_path.to_str().unwrap().to_string(),
            video_encoder: Default::default(),
            default_slave_ipv4_address: String::from("192.168.137.219"),
            default_slave_port: 5600, 
            ..Default::default()
        }
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
            PreferencesMsg::SetSlaveDefaultIPV4Address(address) => self.default_slave_ipv4_address = address,
            PreferencesMsg::SetSlaveDefaultPort(port) => self.default_slave_port = port,
        }
    }
}

