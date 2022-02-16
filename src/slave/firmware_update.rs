use std::{path::PathBuf, fmt::Debug};
use async_std::{io::ReadExt, net::TcpStream, task, prelude::*};

use glib::Sender;
use glib_macros::clone;
use gtk::{Align, Box as GtkBox, Orientation, prelude::*, FileFilter, ProgressBar, FileChooserAction, Button};
use adw::{HeaderBar, PreferencesGroup, StatusPage, Window, prelude::*, ActionRow, Carousel};
use once_cell::unsync::OnceCell;
use relm4::{send, MicroWidgets, MicroModel};
use relm4_macros::micro_widget;

use serde::{Serialize, Deserialize};
use derivative::*;

use crate::prelude::*;
use crate::slave::SlaveTcpMsg;
use crate::ui::generic::select_path;

use super::SlaveMsg;

pub enum SlaveFirmwareUpdaterMsg {
    StartUpload,
    NextStep,
    FirmwareFileSelected(PathBuf),
    FirmwareUploadProgressUpdated(f32),
    FirmwareUploadFailed,
}

#[tracker::track(pub)]
#[derive(Debug, Derivative)]
#[derivative(Default)]
pub struct SlaveFirmwareUpdaterModel {
    current_page: u32,
    firmware_file_path: Option<PathBuf>,
    firmware_uploading_progress: f32,
    #[no_eq]
    _tcp_stream: OnceCell<TcpStream>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SlaveFirmwareUpdatePacket {
    firmware_update: SlaveFirmwarePacket,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SlaveFirmwarePacket {
    size: usize,
    compression: String,
    md5: String,
}

impl SlaveFirmwareUpdaterModel {
    pub fn new(tcp_stream: TcpStream) -> SlaveFirmwareUpdaterModel {
        SlaveFirmwareUpdaterModel {
            _tcp_stream: OnceCell::from(tcp_stream),
            ..Default::default()
        }
    }
    
    pub fn get_tcp_stream(&self) -> &TcpStream {
        self._tcp_stream.get().unwrap()
    }
}

impl MicroModel for SlaveFirmwareUpdaterModel {
    type Msg = SlaveFirmwareUpdaterMsg;
    type Widgets = SlaveFirmwareUpdaterWidgets;
    type Data = Sender<SlaveMsg>;
    
    fn update(&mut self, msg: SlaveFirmwareUpdaterMsg, parent_sender: &Sender<SlaveMsg>, sender: Sender<SlaveFirmwareUpdaterMsg>) {
        self.reset();
        match msg {
            SlaveFirmwareUpdaterMsg::NextStep => self.set_current_page(self.get_current_page().wrapping_add(1)),
            SlaveFirmwareUpdaterMsg::FirmwareFileSelected(path) => self.set_firmware_file_path(Some(path)),
            SlaveFirmwareUpdaterMsg::FirmwareUploadProgressUpdated(progress) => {
                self.set_firmware_uploading_progress(progress);
                if progress >= 1.0 {
                    send!(sender, SlaveFirmwareUpdaterMsg::NextStep);
                }
            },
            SlaveFirmwareUpdaterMsg::StartUpload => {
                if let Some(path) = self.get_firmware_file_path() {
                    send!(sender, SlaveFirmwareUpdaterMsg::NextStep);
                    let mut tcp_stream = self.get_tcp_stream().clone();
                    let handle = task::spawn(clone!(@strong path => async move {
                        match async_std::fs::File::open(path).await {
                            Ok(mut file) => {
                                let mut bytes = Vec::new();
                                file.read_to_end(&mut bytes).await.unwrap();
                                let bytes = bytes.as_slice();
                                let md5_string = format!("{:x}", md5::compute(&bytes));
                                let packet = SlaveFirmwareUpdatePacket {
                                    firmware_update: SlaveFirmwarePacket {
                                        size: bytes.len(),
                                        compression: String::from("gzip"),
                                        md5: md5_string,
                                    }
                                };
                                let json = serde_json::to_string(&packet).unwrap();
                                let mut json_bytes = json.as_bytes();
                                if async_std::io::copy(&mut json_bytes, &mut tcp_stream).await.is_err() {
                                    send!(sender, SlaveFirmwareUpdaterMsg::FirmwareUploadFailed);
                                    return
                                }
                                let chunks = bytes.chunks(1024);
                                let chunk_num = chunks.len();
                                if chunk_num > 0 {
                                    for (chunk_index, chunk) in chunks.enumerate() {
                                        if tcp_stream.write(chunk).await.is_err() {
                                            send!(sender, SlaveFirmwareUpdaterMsg::FirmwareUploadFailed);
                                            return
                                        }
                                        let progress = (chunk_index + 1) as f32 / chunk_num as f32;
                                        send!(sender, SlaveFirmwareUpdaterMsg::FirmwareUploadProgressUpdated(progress));
                                    }
                                    if tcp_stream.flush().await.is_err() {
                                        send!(sender, SlaveFirmwareUpdaterMsg::FirmwareUploadFailed);
                                        return
                                    }
                                } else {
                                    send!(sender, SlaveFirmwareUpdaterMsg::FirmwareUploadProgressUpdated(1.0));
                                }
                            },
                            Err(_) => return,
                        }
                    }));
                    send!(parent_sender, SlaveMsg::TcpMessage(SlaveTcpMsg::Block(handle)));
                }
            },
            SlaveFirmwareUpdaterMsg::FirmwareUploadFailed => eprintln!("固件上传失败！"),
        }
    }
}

#[micro_widget(pub)]
impl MicroWidgets<SlaveFirmwareUpdaterModel> for SlaveFirmwareUpdaterWidgets {
    view! {
        window = Window {
            set_title: Some("固件更新向导"),
            set_width_request: 480,
            set_height_request: 480, 
            set_modal: true,
            set_visible: true,
            set_content = Some(&GtkBox) {
                set_orientation: Orientation::Vertical,
                append = &HeaderBar {
                    set_sensitive: track!(model.changed(SlaveFirmwareUpdaterModel::firmware_uploading_progress()), *model.get_firmware_uploading_progress() <= 0.0 || *model.get_firmware_uploading_progress() >= 1.0),
                },
                append: carousel = &Carousel {
                    set_hexpand: true,
                    set_vexpand: true,
                    set_interactive: false,
                    scroll_to_page: track!(model.changed(SlaveFirmwareUpdaterModel::current_page()), model.current_page, true),
                    append = &StatusPage {
                        set_icon_name: Some("software-update-available-symbolic"),
                        set_title: "欢迎使用固件更新向导",
                        set_hexpand: true,
                        set_vexpand: true,
                        set_description: Some("请确保固件更新期间机器人有充足的电量供应。"),
                        set_child = Some(&Button) {
                            set_css_classes: &["suggested-action", "pill"],
                            set_halign: Align::Center,
                            set_label: "下一步",
                            connect_clicked(sender) => move |_button| {
                                send!(sender, SlaveFirmwareUpdaterMsg::NextStep);
                            },
                        },
                    },
                    append = &StatusPage {
                        set_icon_name: Some("system-file-manager-symbolic"),
                        set_title: "请选择固件文件",
                        set_hexpand: true,
                        set_vexpand: true,
                        set_description: Some("选择的固件文件必须为下位机的可执行文件。"),
                        set_child = Some(&GtkBox) {
                            set_orientation: Orientation::Vertical,
                            set_spacing: 50,
                            append = &PreferencesGroup {
                                add = &ActionRow {
                                    set_title: "固件文件",
                                    set_subtitle: track!(model.changed(SlaveFirmwareUpdaterModel::firmware_file_path()), &model.firmware_file_path.as_ref().map_or("请选择文件".to_string(), |path| path.to_str().unwrap().to_string())),
                                    add_suffix: browse_firmware_file_button = &Button {
                                        set_label: "浏览",
                                        set_valign: Align::Center,
                                        connect_clicked(sender, window) => move |_button| {
                                            let filter = FileFilter::new();
                                            filter.add_suffix("bin");
                                            filter.set_name(Some("固件文件"));
                                            std::mem::forget(select_path(FileChooserAction::Open, &[filter], &window, clone!(@strong sender => move |path| {
                                                match path {
                                                    Some(path) => {
                                                        send!(sender, SlaveFirmwareUpdaterMsg::FirmwareFileSelected(path));
                                                    },
                                                    None => (),
                                                }
                                            }))); // 内存泄露修复
                                        },
                                    },
                                    set_activatable_widget: Some(&browse_firmware_file_button),
                                },
                            },
                            append = &Button {
                                set_css_classes: &["suggested-action", "pill"],
                                set_halign: Align::Center,
                                set_label: "开始更新",
                                set_sensitive: track!(model.changed(SlaveFirmwareUpdaterModel::firmware_file_path()), model.get_firmware_file_path().as_ref().map_or(false, |pathbuf| pathbuf.exists() && pathbuf.is_file())),
                                connect_clicked(sender) => move |_button| {
                                    send!(sender, SlaveFirmwareUpdaterMsg::StartUpload);
                                },
                            }
                        },
                    },
                    append = &StatusPage {
                        set_icon_name: Some("folder-download-symbolic"),
                        set_title: "正在更新固件...",
                        set_hexpand: true,
                        set_vexpand: true,
                        set_description: Some("请不要切断连接或电源。"),
                        set_child = Some(&GtkBox) {
                            set_orientation: Orientation::Vertical,
                            set_spacing: 50,
                            append = &ProgressBar {
                                set_fraction: track!(model.changed(SlaveFirmwareUpdaterModel::firmware_uploading_progress()), *model.get_firmware_uploading_progress() as f64)
                            },
                        },
                    },
                    append = &StatusPage {
                        set_icon_name: Some("emblem-ok-symbolic"),
                        set_title: "固件更新完成",
                        set_hexpand: true,
                        set_vexpand: true,
                        set_description: Some("机器人将自动重启，请稍后手动进行连接。"),
                        set_child = Some(&Button) {
                            set_css_classes: &["suggested-action", "pill"],
                            set_halign: Align::Center,
                            set_label: "完成",
                            connect_clicked(window) => move |_button| {
                                window.destroy();
                            },
                        },
                    },
                },
            },
        }
    }
}

impl Debug for SlaveFirmwareUpdaterWidgets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.root_widget().fmt(f)
    }
}
