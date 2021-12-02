use std::{cell::RefCell, fmt::Display, fs, rc::Rc};

use glib::{GEnum, Object, Type, Value, value::FromValue};
use gtk4 as gtk;

use gtk::{Adjustment, Align, ApplicationWindow, Box as GtkBox, Button, Dialog, Entry, FileChooser, FileChooserDialog, Label, ListBox, ListBoxRow, MapListModel, Orientation, ResponseType, ScrolledWindow, SelectionModel, SpinButton, StringList, Switch, Viewport, Window, gio::{self, ListModel}, prelude::*};

use gio::{prelude::*, ListStore};

use adw::{ActionRow, ComboRow, ComboRowBuilder, EnumListModel, PreferencesGroup, PreferencesPage, PreferencesWindow, prelude::*};

use crate::MainWindowWrapper;

use strum::IntoEnumIterator;
use strum_macros::{EnumIter, EnumString as EnumFromString, Display as EnumToString};

#[derive(EnumIter, EnumToString, EnumFromString)]
enum VideoEncoder {
    Copy, H264, H265
}

pub struct Preferences {
    video_save_path: String,
    video_encoder: VideoEncoder,
}

impl Default for Preferences {
    fn default() -> Self {
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
            video_encoder: VideoEncoder::Copy
        }
    }
}

pub struct PreferencesWindowWrapper {
    window: Window,
    preferences: Rc<RefCell<Preferences>>,
}

impl PreferencesWindowWrapper {
    pub fn new(app_window: &ApplicationWindow, preferences: Rc<RefCell<Preferences>>) -> PreferencesWindowWrapper {
        let mut wrapper = PreferencesWindowWrapper {
            window: Window::new(),
            preferences,
        };
        wrapper.window = wrapper.build_window(app_window.dynamic_cast_ref().unwrap());
        wrapper
    }

    pub fn window(&self) -> &Window {
        &self.window
    }
    
    fn build_window(&self, parent_window: &Window) -> Window {
        let window = PreferencesWindow::builder().
            can_swipe_back(true)
            .title("首选项")
            .modal(true)
            .transient_for(parent_window)
            .build();
        let page_connection = {
            let page =  PreferencesPage::builder()
                .icon_name("network-transmit-receive-symbolic")
                .title("网络")
                .build();
            let group_1 = {
                let group = PreferencesGroup::builder()
                    .description("与机器人的连接通信设置")
                    .title("连接")
                    .build();
                let row_default_address = {
                    let row = ActionRow::builder()
                        .title("默认地址")
                        .subtitle("第一机位的机器人使用的默认IPV4地址，其他机位的地址将在该基础上进行累加")
                        .build();
                    let entry = Entry::builder()
                        .text("192.168.137.219")
                        .valign(Align::Center)
                        .build();
                    row.add_suffix(&entry);
                    row
                };
                let row_default_port = {
                    let row = ActionRow::builder()
                        .title("默认端口")
                        .subtitle("连接机器人的默认端口")
                        .build();
                    let spin_button = SpinButton::with_range(0.0, 65535.0, 1.0);
                    spin_button.set_value(8888.0);
                    spin_button.set_digits(0);
                    spin_button.set_valign(Align::Center);
                    row.add_suffix(&spin_button);
                    row
                };
                group.add(&row_default_address);
                group.add(&row_default_port);
                group
            };
            page.add(&group_1);
            page
        };
        let page_video = {
            let page =  PreferencesPage::builder()
                .icon_name("emblem-videos-symbolic")
                .title("视频")
                .build();
            let group_record = {
                let group = PreferencesGroup::builder()
                    .description("视频流的录制选项")
                    .title("录制")
                    .build();
                let row_record_directory = {
                    let row = ActionRow::builder()
                        .title("视频保存目录")
                        .subtitle(self.preferences.borrow().video_save_path.as_str())
                        .activatable(true)
                        .build();
                    let preference = self.preferences.clone();
                    let parent_window = parent_window.clone();
                    row.connect_activated(move |row| {
                        let file_chooser_dialog = FileChooserDialog::builder()
                            .modal(true)
                            .transient_for(&parent_window)
                            .action(gtk::FileChooserAction::SelectFolder)
                            .build();
                        file_chooser_dialog.add_buttons(&[("取消", ResponseType::Cancel), ("选择", ResponseType::Accept)]);
                        
                        let row = row.clone();
                        let preference = preference.clone();
                        file_chooser_dialog.connect_response(move |file_chooser_dialog, response| {
                            match response {
                                gtk::ResponseType::Accept => {
                                    match file_chooser_dialog.file().and_then(|x| x.path()).and_then(|x| x.to_str().map(String::from)) { 
                                        Some(path) => {
                                            row.set_subtitle(path.as_str());
                                            preference.borrow_mut().video_save_path = path;
                                        },
                                        None => {},
                                    }
                                },
                                _ => {}
                            }
                            file_chooser_dialog.hide();
                        });
                        file_chooser_dialog.show();
                    });
                    row
                };
                let row_encoder = {
                    let model = StringList::new(&[]);
                    for value in VideoEncoder::iter() {
                        model.append(&value.to_string());
                    }
                    let row = ComboRowBuilder::new()
                        .title("编码器")
                        .subtitle("视频录制时使用的编码器")
                        .model(&model)
                        .build();
                    let preferences = self.preferences.clone();
                    row.connect_selected_notify(move |item| {
                        preferences.borrow_mut().video_encoder = VideoEncoder::iter().nth(item.selected() as usize).unwrap();
                    });
                    row
                };
                group.add(&row_record_directory);
                group.add(&row_encoder);
                group
            };
            page.add(&group_record);
            page
        };
        window.add(&page_connection);
        window.add(&page_video);
        window.dynamic_cast().unwrap()
        // let gtkbox = GtkBox::builder().orientation(Orientation::Horizontal).build();
        // let category_scrolled_window = {
        //     let scrolled_window = ScrolledWindow::builder()
        //         .width_request(128)
        //         .can_focus(true)
        //         .build();
        //     let viewport = Viewport::builder().build();
        //     let list_box = ListBox::builder().build();
        //     for category_name in PreferenceCategory::iter().map(|x| x.to_str()) {
        //         let list_box_row = ListBoxRow::builder().height_request(40).build();
        //         let label = Label::builder().label(category_name).build();
        //         list_box_row.set_child(Some(&label));
        //         list_box.append(&list_box_row);
        //     }
        //     viewport.set_child(Some(&list_box));
        //     scrolled_window.set_child(Some(&viewport));
        //     scrolled_window
        // };
        // gtkbox.append(&category_scrolled_window);
        // self.window().set_child(Some(&gtkbox));
    }
}

