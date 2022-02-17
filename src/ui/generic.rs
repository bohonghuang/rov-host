/* generic.rs
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

use std::path::PathBuf;

use gtk::{FileChooserNative, FileFilter, prelude::*, FileChooserAction, MessageDialog, ResponseType};

pub fn select_path<T, F>(action: FileChooserAction, filters: &[FileFilter], parent_window: &T, callback: F) -> FileChooserNative
where T: IsA<gtk::Window>,
      F: 'static + Fn(Option<PathBuf>) -> () {
    relm4_macros::view! {
        file_chooser = FileChooserNative {
            set_action: action,
            add_filter: iterate!(filters),
            set_create_folders: true,
            set_cancel_label: Some("取消"),
            set_accept_label: Some("打开"),
            set_modal: true,
            set_transient_for: Some(parent_window),
            connect_response => move |dialog, res_ty| {
                match res_ty {
                    gtk::ResponseType::Accept => {
                        if let Some(file) = dialog.file() {
                            if let Some(path) = file.path() {
                                callback(Some(path));
                                return;
                            }
                        }
                    },
                    gtk::ResponseType::Cancel => {
                        callback(None);
                    },
                    _ => (),
                }
            },
        }
    }
    file_chooser.show();
    file_chooser
}

pub fn error_message<T>(title: &str, msg: &str, window: Option<&T>) -> MessageDialog where T: IsA<gtk::Window> {
    relm4_macros::view! {
        dialog = MessageDialog {
            set_message_type: gtk::MessageType::Error,
            set_text: Some(msg),
            set_title: Some(title),
            set_modal: true,
            set_transient_for: window,
            add_button: args!("确定", ResponseType::Ok),
            connect_response => |dialog, _response| {
                dialog.destroy();
            }
        }
    }
    dialog.show();
    dialog
}
