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
            connect_response => |dialog, response| {
                dialog.destroy();
            }
        }
    }
    dialog.show();
    dialog
}
