use std::cell::{Cell, RefCell};

use glib::{Sender, clone};

use gtk4 as gtk;
use gtk::{AboutDialog, Align, ApplicationWindow, Box as GtkBox, Button, Image, Label, MenuButton, gio::{Menu, MenuItem}, prelude::*};

use adw::{prelude::*, HeaderBar};

use preferences::PreferencesModel;
use relm4::{AppUpdate, ComponentUpdate, Components, Model, RelmApp, RelmComponent, Widgets, actions::{ActionGroupName, ActionName, RelmAction, RelmActionGroup}, factory::FactoryVecDeque, send, new_action_group, new_statful_action, new_statless_action};
use relm4_macros::widget;

mod preferences;
mod slave;
mod components;

use slave::SlaveModel;

use crate::preferences::PreferencesMsg;

use derivative::*;

#[derive(Default)]
struct HeaderModel {
    recording: Option<bool>
}

impl Model for HeaderModel {
    type Msg = HeaderMsg;
    type Widgets = HeaderWidgets;
    type Components = ();
}

#[widget]
impl Widgets<HeaderModel, AppModel> for HeaderWidgets {
    view! {
        HeaderBar {
            pack_start = &Button {
                set_halign: Align::Center,
                set_sensitive: watch!(model.recording != None),
                set_css_classes?: watch!(model.recording.map(|x| if x { &["destructive-action"] as &[&str] } else { &[] as &[&str] })),
                set_child = Some(&GtkBox) {
                    set_spacing: 6,
                    append = &Image {
                        set_icon_name?: watch!(model.recording.map(|x| Some(if x { "media-playback-stop-symbolic" } else { "media-record-symbolic" })))
                    },
                    append = &Label {
                        set_label?: watch!(model.recording.map(|x| if x { "停止" } else { "录制" })),
                    },
                },
                connect_clicked(sender) => move |button| {
                    send!(sender, HeaderMsg::ToggleRecord);
                }
            },
            pack_end = &MenuButton {
                set_menu_model: Some(&main_menu),
                set_icon_name: "open-menu-symbolic",
                set_focus_on_click: false,
                set_valign: Align::Center,
            }
        }
    }
    menu! {
        main_menu: {
            "首选项" => PreferencesAction,
            "键盘快捷键" => KeybindingsAction,
            "关于" => AboutDialogAction,
        }
    }
}

enum HeaderMsg {
    RecordStarted,
    RecordStopped,
    ToggleRecord,
}

impl ComponentUpdate<AppModel> for HeaderModel {
    fn init_model(parent_model: &AppModel) -> Self {
        HeaderModel {
            recording: Some(parent_model.recording),
            ..Default::default() }
    }

    fn update(
        &mut self,
        msg: HeaderMsg,
        components: &(),
        sender: Sender<HeaderMsg>,
        parent_sender: Sender<AppMsg>,
    ) {
        match msg {
            HeaderMsg::RecordStarted => {
                self.recording = Some(true);
            },
            HeaderMsg::RecordStopped => {
                self.recording = Some(false);
            },
            HeaderMsg::ToggleRecord => {
                match self.recording {
                    Some(recording) => {
                        self.recording = None;
                        parent_sender.send(if recording { AppMsg::StopRecord } else { AppMsg::StartRecord }).unwrap();
                    },
                    None => (),
                }
            },
            
        }
    }
}

struct AboutModel {}
enum AboutMsg {}
impl Model for AboutModel {
    type Msg = AboutMsg;
    type Widgets = AboutWidgets;
    type Components = ();
}
#[widget]
impl Widgets<AboutModel, AppModel> for AboutWidgets {
    view! {
        dialog = AboutDialog {
            set_transient_for: parent!(Some(&parent_widgets.app_window)),
            set_destroy_with_parent: true,
            set_can_focus: false,
            set_modal: true,
            set_visible: true,
            set_authors: &["黄博宏"],
            set_program_name: Some("水下机器人上位机"),
            set_copyright: Some("© 2021 集美大学水下智能创新实验室"),
            set_comments: Some("跨平台的校园水下机器人上位机程序"),
            set_logo_icon_name: Some("applications-games"),
            set_version: Some("0.0.1"),
        }
    }
}
impl ComponentUpdate<AppModel> for AboutModel {
    fn init_model(parent_model: &AppModel) -> Self { AboutModel {} }
    fn update(&mut self, msg: AboutMsg, components: &(), sender: Sender<AboutMsg>, parent_sender: Sender<AppMsg>) {}
}

#[derive(Default)]
pub struct AppModel {
    recording: bool,
}

impl Model for AppModel {
    type Msg = AppMsg;
    type Widgets = AppWidgets;
    type Components = AppComponents;
}

new_action_group!(AppActionGroup, "main");
new_statless_action!(PreferencesAction, AppActionGroup, "preferences");
new_statless_action!(KeybindingsAction, AppActionGroup, "keybindings");
new_statless_action!(AboutDialogAction, AppActionGroup, "about");

#[widget(pub)]
impl Widgets<AppModel, ()> for AppWidgets {
    view! {
        app_window = ApplicationWindow {
            set_titlebar: Some(components.header.root_widget()),
            set_title: Some("水下机器人上位机"),
        }
    }
    
    fn post_init() {
        let app_group = RelmActionGroup::<AppActionGroup>::new();
        
        let action_preferences: RelmAction<PreferencesAction> = RelmAction::new_statelesss(clone!(@strong sender => move |_| {
            send!(sender, AppMsg::OpenPreferencesWindow);
        }));
        let action_keybindings: RelmAction<KeybindingsAction> = RelmAction::new_statelesss(clone!(@strong sender => move |_| {
            send!(sender, AppMsg::OpenKeybindingsWindow);
        }));
        let action_about: RelmAction<AboutDialogAction> = RelmAction::new_statelesss(clone!(@strong sender => move |_| {
            send!(sender, AppMsg::OpenAboutDialog);
        }));
        app_group.add_action(action_preferences);
        app_group.add_action(action_keybindings);
        app_group.add_action(action_about);
        app_window.insert_action_group("main", Some(&app_group.into_action_group()));
    }
}

pub enum AppMsg {
    StartRecord,
    StopRecord,
    OpenAboutDialog,
    OpenPreferencesWindow,
    OpenKeybindingsWindow,
}

#[derive(relm4_macros::Components)]
pub struct AppComponents {
    header: RelmComponent<HeaderModel, AppModel>,
}

impl AppUpdate for AppModel {
    fn update(
        &mut self,
        msg: AppMsg,
        components: &AppComponents,
        sender: Sender<AppMsg>,
    ) -> bool {
        match msg {
            AppMsg::StartRecord => {
                components.header.send(HeaderMsg::RecordStarted);
            },
            AppMsg::StopRecord => {
                components.header.send(HeaderMsg::RecordStopped);
            },
            AppMsg::OpenAboutDialog => {
                RelmComponent::<AboutModel, AppModel>::new(self, sender.clone());
            },
            AppMsg::OpenPreferencesWindow => {
                RelmComponent::<PreferencesModel, AppModel>::new(self, sender.clone());
            },
            AppMsg::OpenKeybindingsWindow => todo!(),
        }
        true
    }
}

fn main() {
    gtk::init().map(|_| adw::init()).expect("无法初始化 GTK4");
    let model = AppModel {
        ..Default::default()
    };
    // let model = components::AppModel {
    //     mode: components::AppMode
    // };
    
    let relm = RelmApp::new(model);
    relm.run()
}
