use std::{cell::{Cell, RefCell}, net::Ipv4Addr, rc::Rc};

use glib::{Sender, Type, clone};

use gtk4 as gtk;
use gtk::{AboutDialog, Align, ApplicationWindow, Box as GtkBox, Button, CenterBox, Frame, Grid, Image, Inhibit, Label, MenuButton, Orientation, Stack, gio::{Menu, MenuItem}, prelude::*};

use adw::{HeaderBar, StatusPage, Window, prelude::*};

use preferences::PreferencesModel;
use relm4::{AppUpdate, ComponentUpdate, Components, Model, RelmApp, RelmComponent, Widgets, actions::{ActionGroupName, ActionName, RelmAction, RelmActionGroup}, factory::{DynamicIndex, FactoryPrototype, FactoryVec, FactoryVecDeque, positions::GridPosition}, new_action_group, new_statful_action, new_statless_action, send};
use relm4_macros::widget;

mod preferences;
mod slave;
mod components;
mod prelude;

use crate::{preferences::PreferencesMsg, slave::{SlaveConfigModel, SlaveModel}};

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
            },
            pack_end = &Button {
                set_icon_name: "window-new-symbolic",
                connect_clicked(sender) => move |button| {
                    send!(sender, HeaderMsg::NewSlave);
                },
            },
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
    NewSlave, 
}

impl ComponentUpdate<AppModel> for HeaderModel {
    fn init_model(parent_model: &AppModel) -> Self {
        HeaderModel {
            recording: Some(parent_model.recording),
            ..Default::default()
        }
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
            HeaderMsg::NewSlave => send!(parent_sender, AppMsg::NewSlave),
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

#[derive(Derivative)]
#[derivative(Default)]
pub struct AppModel {
    recording: bool,
    #[derivative(Default(value="FactoryVec::new()"))]
    slaves: FactoryVec<SlaveModel>,
    preferences: PreferencesModel,
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
            set_width_request: 1280,
            set_height_request: 720, 
            set_child = Some(&Stack) {
                add_child = &StatusPage {
                    set_icon_name: Some("window-new-symbolic"),
                    set_title: "无机位",
                    set_visible: watch!(model.slaves.len() == 0),
                    set_description: Some("请点击标题栏右侧按钮添加机位"),
                },
                add_child = &Grid {
                    set_hexpand: true,
                    set_vexpand: true,
                    factory!(model.slaves),
                },
            },
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
        for _ in 0..*model.preferences.get_initial_slave_num() {
            send!(sender, AppMsg::NewSlave);
        }
    }
}

pub enum AppMsg {
    NewSlave,
    SlaveConfigUpdated(usize, SlaveConfigModel),
    DisplaySlaveConfigWindow(usize),
    PreferencesUpdated(PreferencesModel),
    StartRecord,
    StopRecord,
    OpenAboutDialog,
    OpenPreferencesWindow,
    OpenKeybindingsWindow,
}

pub struct AppComponents {
    header: RelmComponent<HeaderModel, AppModel>,
}

impl Components<AppModel> for AppComponents {
    fn init_components(parent_model: &AppModel, parent_sender: Sender<AppMsg>)
                       -> Self {
        Self {
            header: RelmComponent::new(parent_model, parent_sender.clone()),
        }
        
    }

    fn connect_parent(&mut self, _parent_widgets: &AppWidgets) {
        self.header.connect_parent(_parent_widgets);
    }
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
                components.header.send(HeaderMsg::RecordStarted).unwrap();
            },
            AppMsg::StopRecord => {
                components.header.send(HeaderMsg::RecordStopped).unwrap();
            },
            AppMsg::OpenAboutDialog => {
                RelmComponent::<AboutModel, AppModel>::new(self, sender.clone());
            },
            AppMsg::OpenPreferencesWindow => {
                RelmComponent::<PreferencesModel, AppModel>::new(self, sender.clone());
            },
            AppMsg::OpenKeybindingsWindow => todo!(),
            AppMsg::NewSlave => {
                let mut ip_octets = self.preferences.get_default_slave_ipv4_address().octets();
                let index = self.slaves.len() as u8;
                ip_octets[3] = ip_octets[3].wrapping_add(index);
                self.slaves.push(SlaveModel::new(index as usize, SlaveConfigModel::new(Ipv4Addr::from(ip_octets), *self.preferences.get_default_slave_port())));
            },
            AppMsg::DisplaySlaveConfigWindow(index) => {
                if let Some(slave) = self.slaves.get_mut(index) {
                    slave.config.set_window_presented(true);
                }
            },
            AppMsg::PreferencesUpdated(preferences) => {
                self.preferences = preferences;
            },
            AppMsg::SlaveConfigUpdated(index, config) =>
                if let Some(slave) = self.slaves.get_mut(index) {
                    slave.set_config(config);
                },
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
    // let btn = Button::new();
    // unsafe {
    //     btn.set_data("aaaa", "Hello");
    //     let a: &str = *btn.data("aaaa").unwrap().as_ref();
    //     println!("{}", a);
    // };
    let relm = RelmApp::new(model);
    relm.run()
}
