use std::{fs, net::Ipv4Addr, path::PathBuf, str::FromStr};

use glib::Sender;
use gtk::{Align, Entry, Inhibit, Label, SpinButton, StringList, Switch, prelude::*};
use adw::{PreferencesGroup, PreferencesPage, PreferencesWindow, prelude::*, ComboRow, ActionRow};
use relm4::{ComponentUpdate, Model, Widgets, send};
use relm4_macros::widget;

use serde::{Serialize, Deserialize};
use strum::IntoEnumIterator;
use derivative::*;

use crate::{AppModel, AppMsg, slave::video::{VideoEncoder, VideoDecoder, ImageFormat}};

pub fn get_data_path() -> PathBuf {
    const app_dir_name: &str = "rovhost";
    let mut data_path = dirs::data_local_dir().expect("无法找到本地数据文件夹");
    data_path.push(app_dir_name);
    if !data_path.exists() {
        fs::create_dir(data_path.clone()).expect("无法创建应用数据文件夹");
    }
    data_path
}

pub fn get_preference_path() -> PathBuf {
    let mut path = get_data_path();
    path.push("preferences.json");
    path
}

pub fn get_video_path() -> PathBuf {
    let mut video_path = get_data_path();
    video_path.push("Videos");
    if !video_path.exists() {
        fs::create_dir(video_path.clone()).expect("无法创建视频文件夹");
    }
    video_path
}

pub fn get_image_path() -> PathBuf {
    let mut video_path = get_data_path();
    video_path.push("Images");
    if !video_path.exists() {
        fs::create_dir(video_path.clone()).expect("无法创建图片文件夹");
    }
    video_path
}

#[tracker::track]
#[derive(Derivative, Clone, PartialEq, Debug, Serialize, Deserialize)]
#[derivative(Default)]
pub struct PreferencesModel {
    #[derivative(Default(value="0"))]
    pub initial_slave_num: u8,
    #[derivative(Default(value="get_video_path()"))]
    pub video_save_path: PathBuf,
    #[derivative(Default(value="get_image_path()"))]
    pub image_save_path: PathBuf,
    #[derivative(Default(value="ImageFormat::JPEG"))]
    pub image_save_format: ImageFormat,
    #[derivative(Default(value="VideoEncoder::Copy"))]
    pub video_encoder: VideoEncoder,
    #[derivative(Default(value="Ipv4Addr::new(192, 168, 137, 219)"))]
    pub default_slave_ipv4_address: Ipv4Addr,
    #[derivative(Default(value="8888"))]
    pub default_slave_port: u16,
    #[derivative(Default(value="5600"))]
    pub default_local_video_port: u16,
    #[derivative(Default(value="60"))]
    pub default_input_sending_rate: u16,
    #[derivative(Default(value="true"))]
    pub default_keep_video_display_ratio: bool,
    pub default_video_decoder: VideoDecoder,
}

impl PreferencesModel {
    pub fn load_or_default() -> PreferencesModel {
        match fs::read_to_string(get_preference_path()).ok().and_then(|json| serde_json::from_str(&json).ok()) {
            Some(model) => model,
            None => Default::default(),
        }
    }
}

#[derive(Debug)]
pub enum PreferencesMsg {
    SetVideoSavePath(PathBuf),
    SetImageSavePath(PathBuf),
    SetImageSaveFormat(ImageFormat),
    SetVideoEncoder(VideoEncoder),
    SetSlaveDefaultIpv4Address(Ipv4Addr),
    SetSlaveDefaultPort(u16),
    SetInitialSlaveNum(u8),
    SetDefaultLocalVideoPort(u16),
    SetInputSendingRate(u16),
    SetDefaultKeepVideoDisplayRatio(bool),
    SetDefaultVideoDecoder(VideoDecoder),
    SaveToFile,
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
            set_search_enabled: false,
            connect_close_request(sender) => move |window| {
                send!(sender, PreferencesMsg::SaveToFile);
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
                            connect_value_changed(sender) => move |button| {
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
                        set_subtitle: "第一机位的机器人使用的默认IPV4地址",
                        add_suffix = &Entry {
                            set_text: track!(model.changed(PreferencesModel::default_slave_ipv4_address()), model.get_default_slave_ipv4_address().to_string().as_str()),
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
                            connect_value_changed(sender) => move |button| {
                                send!(sender, PreferencesMsg::SetSlaveDefaultPort(button.value() as u16));
                            }
                        },
                    },
                },
            },
            add = &PreferencesPage {
                set_title: "控制",
                set_icon_name: Some("input-gaming-symbolic"),
                add = &PreferencesGroup {
                    set_title: "发送",
                    set_description: Some("向下位机发送控制信号的设置（需要重新连接以应用更改）"),
                    add = &ActionRow {
                        set_title: "增量发送",
                        set_subtitle: "每次发送只发送相对上一次发送的变化值以节省数据发送量。",
                        set_sensitive: false,
                        add_suffix: increamental_sending_switch = &Switch {
                            set_active: false,
                            set_valign: Align::Center,
                        },
                        set_activatable_widget: Some(&increamental_sending_switch),
                    },
                    add = &ActionRow {
                        set_title: "输入发送率",
                        set_subtitle: "每秒钟向下位机发送的控制数据包的个数，该值越高意味着控制越灵敏，但在较差的网络条件下可能产生更大的延迟。",
                        add_suffix = &SpinButton::with_range(1.0, 1000.0, 1.0) {
                            set_value: track!(model.changed(PreferencesModel::default_input_sending_rate()), model.default_input_sending_rate as f64),
                            set_digits: 0,
                            set_valign: Align::Center,
                            connect_value_changed(sender) => move |button| {
                                send!(sender, PreferencesMsg::SetInputSendingRate(button.value() as u16));
                            }
                        },
                        add_suffix = &Label {
                            set_label: "Hz",
                        },
                    },
                },
            },
            add = &PreferencesPage {
                set_title: "视频",
                set_icon_name: Some("emblem-videos-symbolic"),
                add = &PreferencesGroup {
                    set_title: "显示",
                    set_description: Some("上位机的显示的画面设置"),
                    add = &ActionRow {
                        set_title: "默认保持长宽比",
                        set_subtitle: "在改变窗口大小的时是否保持画面比例，这可能导致画面无法全屏",
                        add_suffix: default_keep_video_display_ratio_switch = &Switch {
                            set_active: track!(model.changed(PreferencesModel::default_keep_video_display_ratio()), model.default_keep_video_display_ratio),
                            set_valign: Align::Center,
                            connect_state_set(sender) => move |switch, state| {
                                send!(sender, PreferencesMsg::SetDefaultKeepVideoDisplayRatio(state));
                                Inhibit(false)
                            }
                        },
                        set_activatable_widget: Some(&default_keep_video_display_ratio_switch),
                    },
                },
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
                            connect_value_changed(sender) => move |button| {
                                send!(sender, PreferencesMsg::SetDefaultLocalVideoPort(button.value() as u16));
                            }
                        },
                    },
                    add = &ComboRow {
                        set_title: "默认解码器",
                        set_subtitle: "拉流时使用的解码器",
                        set_model: Some(&{
                            let model = StringList::new(&[]);
                            for value in VideoDecoder::iter() {
                                model.append(&value.to_string());
                            }
                            model
                        }),
                        set_selected: track!(model.changed(PreferencesModel::default_video_decoder()), VideoDecoder::iter().position(|x| x == model.default_video_decoder).unwrap() as u32),
                        connect_selected_notify(sender) => move |row| {
                            send!(sender, PreferencesMsg::SetDefaultVideoDecoder(VideoDecoder::iter().nth(row.selected() as usize).unwrap()))
                        }
                    },
                },
                add = &PreferencesGroup {
                    set_title: "截图",
                    set_description: Some("画面的截图选项"),
                    add = &ActionRow {
                        set_title: "图片保存目录",
                        set_subtitle: track!(model.changed(PreferencesModel::image_save_path()), model.image_save_path.to_str().unwrap()),
                        set_activatable: true,
                        connect_activated[sender = sender.clone(), window = window.clone().downgrade()] => move |row| {
                            
                        }
                    },
                    add = &ComboRow {
                        set_title: "保存格式",
                        set_subtitle: "截图保存的格式",
                        set_model: Some(&{
                            let model = StringList::new(&[]);
                            for value in ImageFormat::iter() {
                                model.append(&value.to_string());
                            }
                            model
                        }),
                        set_selected: track!(model.changed(PreferencesModel::image_save_format()), ImageFormat::iter().position(|x| x == model.image_save_format).unwrap() as u32),
                        connect_selected_notify(sender) => move |row| {
                            send!(sender, PreferencesMsg::SetImageSaveFormat(ImageFormat::iter().nth(row.selected() as usize).unwrap()))
                        }
                    },
                },
                add = &PreferencesGroup {
                    set_title: "录制",
                    set_description: Some("视频流的录制选项"),
                    add = &ActionRow {
                        set_title: "视频保存目录",
                        set_subtitle: track!(model.changed(PreferencesModel::video_save_path()), model.video_save_path.to_str().unwrap()),
                        set_activatable: true,
                        connect_activated[sender = sender.clone(), window = window.clone().downgrade()] => move |row| {
                            
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
            PreferencesMsg::SetVideoSavePath(path) => self.set_video_save_path(path),
            PreferencesMsg::SetVideoEncoder(encoder) => self.set_video_encoder(encoder),
            PreferencesMsg::SetSlaveDefaultIpv4Address(address) => self.set_default_slave_ipv4_address(address),
            PreferencesMsg::SetSlaveDefaultPort(port) => self.set_default_slave_port(port),
            PreferencesMsg::SetInitialSlaveNum(num) => self.set_initial_slave_num(num),
            PreferencesMsg::SetDefaultLocalVideoPort(port) => self.set_default_local_video_port(port),
            PreferencesMsg::SetInputSendingRate(rate) => self.set_default_input_sending_rate(rate),
            PreferencesMsg::SetDefaultKeepVideoDisplayRatio(value) => self.set_default_keep_video_display_ratio(value),
            PreferencesMsg::SetDefaultVideoDecoder(decoder) => self.set_default_video_decoder(decoder),
            PreferencesMsg::SaveToFile => serde_json::to_string_pretty(&self).ok().and_then(|json| fs::write(get_preference_path(), json).ok()).unwrap(),
            PreferencesMsg::SetImageSavePath(path) => self.set_image_save_path(path),
            PreferencesMsg::SetImageSaveFormat(format) => self.set_image_save_format(format),
        }
        self.reset();
        send!(parent_sender, AppMsg::PreferencesUpdated(self.clone()));
    }
}
