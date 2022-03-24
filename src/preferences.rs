/* preferences.rs
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

use std::{fs, path::PathBuf, str::FromStr};

use glib::Sender;
use gtk::{Align, Entry, Inhibit, Label, SpinButton, StringList, Switch, prelude::*};
use adw::{PreferencesGroup, PreferencesPage, PreferencesWindow, prelude::*, ComboRow, ActionRow, ExpanderRow};
use relm4::{ComponentUpdate, Model, Widgets, send};
use relm4_macros::widget;

use serde::{Serialize, Deserialize};
use strum::IntoEnumIterator;
use derivative::*;
use url::Url;

use crate::{AppModel, AppMsg, slave::video::{VideoEncoder, VideoDecoder, ImageFormat, ColorspaceConversion, VideoCodec, VideoCodecProvider}};

pub fn get_data_path() -> PathBuf {
    const APP_DIR_NAME: &str = "rovhost";
    let mut data_path = dirs::data_local_dir().expect("无法找到本地数据文件夹");
    data_path.push(APP_DIR_NAME);
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
    pub default_reencode_recording_video: bool,
    pub default_video_encoder: VideoEncoder,
    #[derivative(Default(value="Url::from_str(\"tcp://192.168.137.219:8888\").unwrap()"))]
    pub default_slave_url: Url,
    #[derivative(Default(value="Url::from_str(\"udp://127.0.0.1:5600\").unwrap()"))]
    pub default_video_url: Url,
    #[derivative(Default(value="60"))]
    pub default_input_sending_rate: u16,
    #[derivative(Default(value="true"))]
    pub default_keep_video_display_ratio: bool,
    pub default_video_decoder: VideoDecoder,
    pub default_colorspace_conversion: ColorspaceConversion,
    #[derivative(Default(value="64"))]
    pub default_param_tuner_graph_view_point_num_limit: u16,
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
    SetInitialSlaveNum(u8),
    SetInputSendingRate(u16),
    SetDefaultKeepVideoDisplayRatio(bool),
    SetDefaultVideoDecoderCodec(VideoCodec),
    SetDefaultVideoDecoderCodecProvider(VideoCodecProvider),
    SetDefaultVideoEncoderCodec(VideoCodec),
    SetDefaultVideoEncoderCodecProvider(VideoCodecProvider),
    SetDefaultParameterTunerGraphViewPointNumberLimit(u16),
    SetDefaultColorspaceConversion(ColorspaceConversion),
    SetDefaultReencodeRecordingVideo(bool),
    SetDefaultVideoUrl(Url),
    SetDefaultSlaveUrl(Url),
    SaveToFile,
    OpenVideoDirectory,
    OpenImageDirectory,
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
                        set_title: "默认连接 URL",
                        set_subtitle: "连接第一机位的机器人使用的默认 URL，其他机位会自动累加 IPV4 地址",
                        add_suffix = &Entry {
                            set_text: track!(model.changed(PreferencesModel::default_slave_url()), model.get_default_slave_url().to_string().as_str()),
                            set_valign: Align::Center,
                            set_width_request: 200,
                            connect_changed(sender) => move |entry| {
                                if let Ok(url) = Url::from_str(&entry.text()) {
                                    send!(sender, PreferencesMsg::SetDefaultSlaveUrl(url));
                                }
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
                        set_subtitle: "每次发送只发送相对上一次发送的变化值以节省数据发送量",
                        set_sensitive: false,
                        add_suffix: increamental_sending_switch = &Switch {
                            set_active: false,
                            set_valign: Align::Center,
                        },
                        set_activatable_widget: Some(&increamental_sending_switch),
                    },
                    add = &ActionRow {
                        set_title: "输入发送率",
                        set_subtitle: "每秒钟向下位机发送的控制数据包的个数，该值越高意味着控制越灵敏，但在较差的网络条件下可能产生更大的延迟",
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
                set_icon_name: Some("video-display-symbolic"),
                add = &PreferencesGroup {
                    set_title: "显示",
                    set_description: Some("上位机的显示的画面设置"),
                    add = &ActionRow {
                        set_title: "默认保持长宽比",
                        set_subtitle: "在改变窗口大小的时是否保持画面比例，这可能导致画面无法全屏",
                        add_suffix: default_keep_video_display_ratio_switch = &Switch {
                            set_active: track!(model.changed(PreferencesModel::default_keep_video_display_ratio()), model.default_keep_video_display_ratio),
                            set_valign: Align::Center,
                            connect_state_set(sender) => move |_switch, state| {
                                send!(sender, PreferencesMsg::SetDefaultKeepVideoDisplayRatio(state));
                                Inhibit(false)
                            }
                        },
                        set_activatable_widget: Some(&default_keep_video_display_ratio_switch),
                    },
                },
                add = &PreferencesGroup {
                    set_title: "管道",
                    set_description: Some("配置拉流以及录制所使用的管道"),
                    add = &ActionRow {
                        set_title: "默认拉流 URL",
                        set_subtitle: "第一机位拉流使用的默认 URL，其他机位会自动累加端口",
                        add_suffix = &Entry {
                            set_text: track!(model.changed(PreferencesModel::default_video_url()), model.get_default_video_url().to_string().as_str()),
                            set_valign: Align::Center,
                            set_width_request: 200,
                            connect_changed(sender) => move |entry| {
                                if let Ok(url) = Url::from_str(&entry.text()) {
                                    send!(sender, PreferencesMsg::SetDefaultVideoUrl(url));
                                }
                            }
                        },
                    },
                    add = &ComboRow {
                        set_title: "默认色彩空间转换",
                        set_subtitle: "设置视频编解码、视频流显示要求的色彩空间转换所使用的默认硬件",
                        set_model: Some(&{
                            let model = StringList::new(&[]);
                            for value in ColorspaceConversion::iter() {
                                model.append(&value.to_string());
                            }
                            model
                        }),
                        set_selected: track!(model.changed(PreferencesModel::default_colorspace_conversion()), ColorspaceConversion::iter().position(|x| x == model.default_colorspace_conversion).unwrap() as u32),
                        connect_selected_notify(sender) => move |row| {
                            send!(sender, PreferencesMsg::SetDefaultColorspaceConversion(ColorspaceConversion::iter().nth(row.selected() as usize).unwrap()));
                        }
                    },
                    add = &ComboRow {
                        set_title: "默认解码器",
                        set_subtitle: "拉流时默认使用的解码器",
                        set_model: Some(&{
                            let model = StringList::new(&[]);
                            for value in VideoCodec::iter() {
                                model.append(&value.to_string());
                            }
                            model
                        }),
                        set_selected: track!(model.changed(PreferencesModel::default_video_decoder()), VideoCodec::iter().position(|x| x == model.default_video_decoder.0).unwrap() as u32),
                        connect_selected_notify(sender) => move |row| {
                            send!(sender, PreferencesMsg::SetDefaultVideoDecoderCodec(VideoCodec::iter().nth(row.selected() as usize).unwrap()))
                        }
                    },
                    add = &ComboRow {
                        set_title: "默认解码器接口",
                        set_subtitle: "拉流时默认调用的解码器接口",
                        set_model: Some(&{
                            let model = StringList::new(&[]);
                            for value in VideoCodecProvider::iter() {
                                model.append(&value.to_string());
                            }
                            model
                        }),
                        set_selected: track!(model.changed(PreferencesModel::default_video_decoder()), VideoCodecProvider::iter().position(|x| x == model.default_video_decoder.1).unwrap() as u32),
                        connect_selected_notify(sender) => move |row| {
                            send!(sender, PreferencesMsg::SetDefaultVideoDecoderCodecProvider(VideoCodecProvider::iter().nth(row.selected() as usize).unwrap()))
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
                        connect_activated(sender) => move |_row| {
                            send!(sender, PreferencesMsg::OpenImageDirectory);
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
                        connect_activated(sender) => move |_row| {
                            send!(sender, PreferencesMsg::OpenVideoDirectory);
                        }
                    },
                    add = &ExpanderRow {
                        set_title: "录制时重新编码",
                        set_show_enable_switch: true,
                        set_expanded: *model.get_default_reencode_recording_video(),
                        set_enable_expansion: track!(model.changed(PreferencesModel::default_reencode_recording_video()), *model.get_default_reencode_recording_video()),
                        connect_enable_expansion_notify(sender) => move |expander| {
                            send!(sender, PreferencesMsg::SetDefaultReencodeRecordingVideo(expander.enables_expansion()));
                        },
                        add_row = &ComboRow {
                            set_title: "默认编码器",
                            set_subtitle: "视频录制时默认使用的编码器",
                            set_model: Some(&{
                                let model = StringList::new(&[]);
                                for value in VideoCodec::iter() {
                                    model.append(&value.to_string());
                                }
                                model
                            }),
                            set_selected: track!(model.changed(PreferencesModel::default_video_encoder()), VideoCodec::iter().position(|x| x == model.default_video_encoder.0).unwrap() as u32),
                            connect_selected_notify(sender) => move |row| {
                                send!(sender, PreferencesMsg::SetDefaultVideoEncoderCodec(VideoCodec::iter().nth(row.selected() as usize).unwrap()))
                            }
                        },
                        add_row = &ComboRow {
                            set_title: "默认编码器接口",
                            set_subtitle: "视频录制时默认调用的编码器接口",
                            set_model: Some(&{
                                let model = StringList::new(&[]);
                                for value in VideoCodecProvider::iter() {
                                    model.append(&value.to_string());
                                }
                                model
                            }),
                            set_selected: track!(model.changed(PreferencesModel::default_video_encoder()), VideoCodecProvider::iter().position(|x| x == model.default_video_encoder.1).unwrap() as u32),
                            connect_selected_notify(sender) => move |row| {
                                send!(sender, PreferencesMsg::SetDefaultVideoEncoderCodecProvider(VideoCodecProvider::iter().nth(row.selected() as usize).unwrap()))
                            }
                        },
                    }
                }
            },
            add = &PreferencesPage {
                set_title: "调试",
                set_icon_name: Some("preferences-other-symbolic"),
                add = &PreferencesGroup {
                    set_title: "控制环",
                    set_description: Some("配置控制环调试选项"),
                    add = &ActionRow {
                        set_title: "可视化最大点数",
                        set_subtitle: "绘制控制环可视化图表时使用最多使用多少个点，这将影响最多能观测的历史数据",
                        add_suffix = &SpinButton::with_range(1.0, 255.0, 1.0) {
                            set_value: track!(model.changed(PreferencesModel::default_param_tuner_graph_view_point_num_limit()), model.default_param_tuner_graph_view_point_num_limit as f64),
                            set_digits: 0,
                            set_valign: Align::Center,
                            connect_value_changed(sender) => move |button| {
                                send!(sender, PreferencesMsg::SetDefaultParameterTunerGraphViewPointNumberLimit(button.value() as u16));
                            }
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
        _components: &(),
        _sender: Sender<PreferencesMsg>,
        parent_sender: Sender<AppMsg>,
    ) {
        self.reset();
        match msg {
            PreferencesMsg::SetVideoSavePath(path) => self.set_video_save_path(path),
            PreferencesMsg::SetInitialSlaveNum(num) => self.set_initial_slave_num(num),
            PreferencesMsg::SetInputSendingRate(rate) => self.set_default_input_sending_rate(rate),
            PreferencesMsg::SetDefaultKeepVideoDisplayRatio(value) => self.set_default_keep_video_display_ratio(value),
            PreferencesMsg::SaveToFile => serde_json::to_string_pretty(&self).ok().and_then(|json| fs::write(get_preference_path(), json).ok()).unwrap(),
            PreferencesMsg::SetImageSavePath(path) => self.set_image_save_path(path),
            PreferencesMsg::SetImageSaveFormat(format) => self.set_image_save_format(format),
            PreferencesMsg::SetDefaultParameterTunerGraphViewPointNumberLimit(limit) => self.set_default_param_tuner_graph_view_point_num_limit(limit),
            PreferencesMsg::OpenVideoDirectory => gtk::show_uri(None as Option<&PreferencesWindow>, glib::filename_to_uri(self.get_video_save_path().to_str().unwrap(), None).unwrap().as_str(), gdk::CURRENT_TIME),
            PreferencesMsg::OpenImageDirectory => gtk::show_uri(None as Option<&PreferencesWindow>, glib::filename_to_uri(self.get_image_save_path().to_str().unwrap(), None).unwrap().as_str(), gdk::CURRENT_TIME),
            PreferencesMsg::SetDefaultColorspaceConversion(conversion) => self.set_default_colorspace_conversion(conversion),
            PreferencesMsg::SetDefaultVideoUrl(url) => self.default_video_url = url,
            PreferencesMsg::SetDefaultSlaveUrl(url) => self.default_slave_url = url,
            PreferencesMsg::SetDefaultVideoDecoderCodec(codec) => self.get_mut_default_video_decoder().0 = codec,
            PreferencesMsg::SetDefaultVideoDecoderCodecProvider(provider) => self.get_mut_default_video_decoder().1 = provider,
            PreferencesMsg::SetDefaultReencodeRecordingVideo(reencode) => self.set_default_reencode_recording_video(reencode),
            PreferencesMsg::SetDefaultVideoEncoderCodec(codec) => self.get_mut_default_video_encoder().0 = codec,
            PreferencesMsg::SetDefaultVideoEncoderCodecProvider(provider) => self.get_mut_default_video_encoder().1 = provider,
        }
        send!(parent_sender, AppMsg::PreferencesUpdated(self.clone()));
    }
}
