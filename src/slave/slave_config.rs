/* slave_config.rs
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

use std::{net::Ipv4Addr, str::FromStr, fmt::Debug};

use glib::Sender;
use gtk::{Align, Box as GtkBox, Entry, Inhibit, Orientation, ScrolledWindow, Separator, SpinButton, StringList, Switch, Viewport, prelude::*};
use adw::{ActionRow, PreferencesGroup, prelude::*, ComboRow, ExpanderRow};
use relm4::{WidgetPlus, send, MicroModel, MicroWidgets};
use relm4_macros::micro_widget;

use strum::IntoEnumIterator;
use derivative::*;

use crate::{preferences::PreferencesModel, slave::video::{VideoDecoder, ColorspaceConversion}};
use super::{SlaveMsg, video::{VideoAlgorithm, VideoSource}};

#[tracker::track(pub)]
#[derive(Debug, Derivative, PartialEq, Clone)]
#[derivative(Default)]
pub struct SlaveConfigModel {
    #[derivative(Default(value="Some(false)"))]
    polling: Option<bool>,
    #[derivative(Default(value="Some(false)"))]
    connected: Option<bool>,
    #[derivative(Default(value="PreferencesModel::default().default_slave_address"))]
    pub ip: Ipv4Addr,
    #[derivative(Default(value="PreferencesModel::default().default_slave_port"))]
    pub port: u16,
    #[derivative(Default(value="PreferencesModel::default().default_specify_video_address"))]
    pub specify_video_address: bool,
    #[derivative(Default(value="PreferencesModel::default().default_video_address"))]
    pub video_address: Ipv4Addr,
    #[derivative(Default(value="PreferencesModel::default().default_specify_video_port"))]
    pub specify_video_port: bool,
    #[derivative(Default(value="PreferencesModel::default().default_video_port"))]
    pub video_port: u16,
    
    pub video_algorithms: Vec<VideoAlgorithm>,
    #[derivative(Default(value="PreferencesModel::default().default_keep_video_display_ratio"))]
    pub keep_video_display_ratio: bool,
    #[derivative(Default(value="PreferencesModel::default().default_video_decoder"))]
    pub video_decoder: VideoDecoder,
    #[derivative(Default(value="PreferencesModel::default().default_colorspace_conversion"))]
    pub colorspace_conversion: ColorspaceConversion,
    #[derivative(Default(value="PreferencesModel::default().default_video_source"))]
    pub video_source: VideoSource,
}

impl SlaveConfigModel {
    pub fn new(ip: Ipv4Addr, video_port: u16) -> Self {
        Self {
            ip, video_port, 
            ..Default::default()
        }
    }
}

impl MicroModel for SlaveConfigModel {
    type Msg = SlaveConfigMsg;
    type Widgets = SlaveConfigWidgets;
    type Data = Sender<SlaveMsg>;
    fn update(&mut self, msg: SlaveConfigMsg, parent_sender: &Sender<SlaveMsg>, _sender: Sender<SlaveConfigMsg>) {
        self.reset();
        match msg {
            SlaveConfigMsg::SetIp(ip) => self.set_ip(ip),
            SlaveConfigMsg::SetPort(port) => self.set_port(port),
            SlaveConfigMsg::SetVideoPort(port) => self.set_video_port(port),
            SlaveConfigMsg::SetKeepVideoDisplayRatio(value) => self.set_keep_video_display_ratio(value),
            SlaveConfigMsg::SetPolling(polling) => self.set_polling(polling),
            SlaveConfigMsg::SetConnected(connected) => self.set_connected(connected),
            SlaveConfigMsg::SetVideoAlgorithm(algorithm) => {
                self.get_mut_video_algorithms().clear();
                if let Some(algorithm) = algorithm {
                    self.get_mut_video_algorithms().push(algorithm);
                }
            },
            SlaveConfigMsg::SetVideoDecoder(decoder) => self.set_video_decoder(decoder),
            SlaveConfigMsg::SetColorspaceConversion(conversion) => self.set_colorspace_conversion(conversion),
            SlaveConfigMsg::SetSpecifyVideoPort(specify) => self.set_specify_video_port(specify),
            SlaveConfigMsg::SetSpecifyVideoAddress(specify) => self.set_specify_video_address(specify),
            SlaveConfigMsg::SetVideoAddress(addr) => self.set_video_address(addr),
            SlaveConfigMsg::SetVideoSource(source) => self.set_video_source(source),
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
    SetSpecifyVideoPort(bool),
    SetVideoPort(u16),
    SetSpecifyVideoAddress(bool),
    SetVideoAddress(Ipv4Addr),
    SetKeepVideoDisplayRatio(bool),
    SetPolling(Option<bool>),
    SetConnected(Option<bool>),
    SetVideoAlgorithm(Option<VideoAlgorithm>),
    SetVideoDecoder(VideoDecoder),
    SetColorspaceConversion(ColorspaceConversion),
    SetVideoSource(VideoSource),
}

#[micro_widget(pub)]
impl MicroWidgets<SlaveConfigModel> for SlaveConfigWidgets {
    view! {
        window = GtkBox {
            add_css_class: "background",
            set_orientation: Orientation::Horizontal,
            append = &Separator {
                set_orientation: Orientation::Horizontal,
            },
            append = &ScrolledWindow {
                set_width_request: 320,
                set_child = Some(&Viewport) {
                    set_child = Some(&GtkBox) {
                        set_spacing: 20,
                        set_margin_all: 10,
                        set_orientation: Orientation::Vertical,
                        append = &PreferencesGroup {
                            set_sensitive: track!(model.changed(SlaveConfigModel::connected()), model.get_connected().eq(&Some(false))),
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
                                    connect_value_changed(sender) => move |button| {
                                        send!(sender, SlaveConfigMsg::SetPort(button.value() as u16));
                                    }
                                },
                            },
                        },
                        append = &PreferencesGroup {
                            set_title: "画面",
                            set_description: Some("上位机端对画面进行的处理选项"),
                            add = &ActionRow {
                                set_title: "保持长宽比",
                                set_subtitle: "在改变窗口大小的时是否保持画面比例，这可能导致画面无法全屏",
                                add_suffix: default_keep_video_display_ratio_switch = &Switch {
                                    set_active: track!(model.changed(SlaveConfigModel::keep_video_display_ratio()), *model.get_keep_video_display_ratio()),
                                    set_valign: Align::Center,
                                    connect_state_set(sender) => move |_switch, state| {
                                        send!(sender, SlaveConfigMsg::SetKeepVideoDisplayRatio(state));
                                        Inhibit(false)
                                    }
                                },
                                set_activatable_widget: Some(&default_keep_video_display_ratio_switch),
                            },
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
                                set_selected: track!(model.changed(SlaveConfigModel::video_algorithms()), VideoAlgorithm::iter().position(|x| model.video_algorithms.first().map_or_else(|| false, |y| *y == x)).map_or_else(|| 0, |x| x + 1) as u32),
                                connect_selected_notify(sender) => move |row| {
                                    send!(sender, SlaveConfigMsg::SetVideoAlgorithm(if row.selected() > 0 { Some(VideoAlgorithm::iter().nth(row.selected().wrapping_sub(1) as usize).unwrap()) } else { None }));
                                }
                            }
                        },
                        append = &PreferencesGroup {
                            set_sensitive: track!(model.changed(SlaveConfigModel::polling()), model.get_polling().eq(&Some(false))),
                            set_title: "管道",
                            set_description: Some("配置拉流以及录制所使用的管道"),
                            add = &ComboRow {
                                set_title: "默认拉流方式",
                                set_subtitle: "默认使用的拉流方式",
                                set_model: Some(&{
                                    let model = StringList::new(&[]);
                                    for value in VideoSource::iter() {
                                        model.append(&value.to_string());
                                    }
                                    model
                                }),
                                set_selected: track!(model.changed(SlaveConfigModel::video_source()), VideoSource::iter().position(|x| x == model.video_source).unwrap() as u32),
                                connect_selected_notify(sender) => move |row| {
                                    send!(sender, SlaveConfigMsg::SetVideoSource(VideoSource::iter().nth(row.selected() as usize).unwrap()))
                                }
                            },
                            add = &ExpanderRow {
                                set_title: "指定拉流地址",
                                set_show_enable_switch: true,
                                set_expanded: *model.get_specify_video_port(),
                                set_enable_expansion: track!(model.changed(SlaveConfigModel::specify_video_address()), *model.get_specify_video_address()),
                                connect_enable_expansion_notify(sender) => move |expander| {
                                    send!(sender, SlaveConfigMsg::SetSpecifyVideoAddress(expander.enables_expansion()));
                                },
                                add_row = &ActionRow {
                                    set_title: "拉流地址",
                                    set_subtitle: "拉取第一个机位的视频流使用的地址",
                                    add_suffix = &Entry {
                                        set_text: track!(model.changed(SlaveConfigModel::video_address()), model.get_video_address().to_string().as_str()),
                                        set_valign: Align::Center,
                                        connect_changed(sender) => move |entry| {
                                            match Ipv4Addr::from_str(&entry.text()) {
                                                Ok(addr) => send!(sender, SlaveConfigMsg::SetVideoAddress(addr)),
                                                Err(_) => (),
                                            }
                                        }
                                    },
                                },
                            },
                            add = &ExpanderRow {
                                set_title: "指定拉流端口",
                                set_show_enable_switch: true,
                                set_expanded: *model.get_specify_video_port(),
                                set_enable_expansion: track!(model.changed(SlaveConfigModel::specify_video_port()), *model.get_specify_video_port()),
                                connect_enable_expansion_notify(sender) => move |expander| {
                                    send!(sender, SlaveConfigMsg::SetSpecifyVideoPort(expander.enables_expansion()));
                                },
                                add_row = &ActionRow {
                                    set_title: "拉流端口",
                                    set_subtitle: "拉取第一个机位的视频流使用的本地端口，其他机位的端口将在该基础上进行累加",
                                    add_suffix = &SpinButton::with_range(0.0, 65535.0, 1.0) {
                                        set_value: track!(model.changed(SlaveConfigModel::video_port()), model.video_port as f64),
                                        set_digits: 0,
                                        set_valign: Align::Center,
                                        connect_value_changed(sender) => move |button| {
                                            send!(sender, SlaveConfigMsg::SetVideoPort(button.value() as u16));
                                        }
                                    },
                                },
                            },
                            add = &ComboRow {
                                set_title: "色彩空间转换",
                                set_subtitle: "设置视频编解码、视频流显示要求的色彩空间转换所使用的硬件",
                                set_model: Some(&{
                                    let model = StringList::new(&[]);
                                    for value in ColorspaceConversion::iter() {
                                        model.append(&value.to_string());
                                    }
                                    model
                                }),
                                set_selected: track!(model.changed(SlaveConfigModel::colorspace_conversion()), ColorspaceConversion::iter().position(|x| x == model.colorspace_conversion).unwrap() as u32),
                                connect_selected_notify(sender) => move |row| {
                                    send!(sender, SlaveConfigMsg::SetColorspaceConversion(ColorspaceConversion::iter().nth(row.selected() as usize).unwrap()));
                                }
                            },
                            add = &ComboRow {
                                set_title: "视频解码器",
                                set_subtitle: "拉流时使用的解码器",
                                set_model: Some(&{
                                    let model = StringList::new(&[]);
                                    for value in VideoDecoder::iter() {
                                        model.append(&value.to_string());
                                    }
                                    model
                                }),
                                set_selected: track!(model.changed(SlaveConfigModel::video_decoder()), VideoDecoder::iter().position(|x| x == model.video_decoder).unwrap() as u32),
                                connect_selected_notify(sender) => move |row| {
                                    send!(sender, SlaveConfigMsg::SetVideoDecoder(VideoDecoder::iter().nth(row.selected() as usize).unwrap()));
                                }
                            },
                        },
                    },
                },
            },
        }
    }
}
