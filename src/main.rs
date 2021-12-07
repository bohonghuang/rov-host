use std::{cell::{Cell, RefCell}, net::Ipv4Addr, rc::Rc};

use fragile::Fragile;
use glib::{Sender, Type, clone};

use gstreamer as gst;

use gtk4 as gtk;
use gtk::{AboutDialog, Align, ApplicationWindow, Box as GtkBox, Button, CenterBox, Frame, Grid, Image, Inhibit, Label, MenuButton, Orientation, Stack, gio::{Menu, MenuItem}, prelude::*};

use adw::{HeaderBar, StatusPage, Window, prelude::*};

use preferences::PreferencesModel;
use relm4::{AppUpdate, ComponentUpdate, Components, Model, RelmApp, RelmComponent, Widgets, actions::{ActionGroupName, ActionName, RelmAction, RelmActionGroup}, factory::{DynamicIndex, FactoryPrototype, FactoryVec, FactoryVecDeque, positions::GridPosition}, new_action_group, new_statful_action, new_statless_action, send};
use relm4_macros::widget;
use lazy_static::{__Deref, lazy_static};

mod preferences;
mod slave;
mod components;
mod prelude;
mod video;

use crate::{preferences::PreferencesMsg, slave::{SlaveConfigModel, SlaveConfigMsg, SlaveModel, SlaveVideoMsg}};

use derivative::*;

#[tracker::track]
#[derive(Derivative)]
#[derivative(Default)]
struct HeaderModel {
    #[derivative(Default(value="None"))]
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
                set_visible: watch!(model.recording != None),
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
                set_sensitive: track!(model.changed(HeaderModel::recording()), model.recording != Some(true)),
                connect_clicked(sender) => move |button| {
                    send!(sender, HeaderMsg::NewSlave);
                },
            },
        }
    }
    menu! {
        main_menu: {
            "首选项"     => PreferencesAction,
            "键盘快捷键" => KeybindingsAction,
            "关于"       => AboutDialogAction,
        }
    }
}

enum HeaderMsg {
    SetRecording(Option<bool>),
    ToggleRecord,
    NewSlave, 
}

impl ComponentUpdate<AppModel> for HeaderModel {
    fn init_model(parent_model: &AppModel) -> Self {
        HeaderModel {
            // recording: Some(parent_model.recording),
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
            HeaderMsg::SetRecording(recording) => {
                self.set_recording(recording);
            },
            HeaderMsg::ToggleRecord => {
                match *self.get_recording() {
                    Some(recording) => {
                        self.set_recording(None);
                        parent_sender.send(AppMsg::SetRecording(!recording)).unwrap();
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

#[tracker::track]
#[derive(Derivative)]
#[derivative(Default)]
pub struct AppModel {
    recording: bool,
    #[no_eq]
    #[derivative(Default(value="FactoryVec::new()"))]
    slaves: FactoryVec<SlaveModel>,
    #[no_eq]
    preferences: Rc<RefCell<PreferencesModel>>,
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
        for _ in 0..*model.preferences.borrow().get_initial_slave_num() {
            send!(sender, AppMsg::NewSlave);
        }
    }
}

pub enum AppMsg {
    NewSlave,
    SlaveConfigUpdated(usize, SlaveConfigModel),
    SlaveToggleConnect(usize),
    SlaveTogglePolling(usize),
    PreferencesUpdated(PreferencesModel),
    SetRecording(bool), 
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
            AppMsg::SetRecording(recording) => {
                // slave::COMPONENTS.get().borrow_mut()
                for components in slave::COMPONENTS.get().borrow().deref() {
                    components.video.send(SlaveVideoMsg::SetRecording(recording)).unwrap();
                }
                components.header.send(HeaderMsg::SetRecording(Some(recording))).unwrap();
            },
            AppMsg::OpenAboutDialog => {
                RelmComponent::<AboutModel, AppModel>::new(self, sender.clone());
            },
            AppMsg::OpenPreferencesWindow => {
                RelmComponent::<PreferencesModel, AppModel>::new(self, sender.clone());
            },
            AppMsg::OpenKeybindingsWindow => todo!(),
            AppMsg::NewSlave => {
                let mut ip_octets = self.get_preferences().borrow().get_default_slave_ipv4_address().octets();
                let index = self.get_slaves().len() as u8;
                ip_octets[3] = ip_octets[3].wrapping_add(index);
                let video_port = self.get_preferences().borrow().get_default_local_video_port().wrapping_add(index as u16);
                self.slaves.push(SlaveModel::new(index as usize, SlaveConfigModel::new(index as usize, Ipv4Addr::from(ip_octets), *self.get_preferences().clone().borrow().get_default_slave_port(), video_port), self.get_preferences().clone()));
                components.header.send(HeaderMsg::SetRecording(Some(false))).unwrap();
            },
            AppMsg::PreferencesUpdated(preferences) => {
                *self.get_preferences().borrow_mut() = preferences;
                self.set_preferences(self.get_preferences().clone());
            },
            AppMsg::SlaveConfigUpdated(index, config) =>
                if let Some(slave) = self.get_mut_slaves().get_mut(index) {
                    *slave.get_config().borrow_mut() = config;
                    slave.set_config(slave.get_config().clone());
                },
            AppMsg::SlaveToggleConnect(index) => {
                if let Some(slave) = self.get_mut_slaves().get_mut(index) {
                    slave.set_connected(None);
                }
            },
            AppMsg::SlaveTogglePolling(index) => {
                if let Some(slave) = self.get_mut_slaves().get_mut(index) {
                    if let Some(components) = slave::COMPONENTS.get().borrow_mut().get(index) {
                        match slave.get_polling() {
                            Some(true) =>{
                                components.video.send(SlaveVideoMsg::SetPipeline(None)).unwrap();
                                slave.set_polling(Some(false));
                            },
                            Some(false) => {
                                components.video.send(SlaveVideoMsg::SetPipeline(Some(video::create_pipeline(*slave.get_config().borrow().get_video_port()).unwrap()))).unwrap();
                                slave.set_polling(Some(true));
                            },
                            None => (),
                        }
                    }
                }
            },
        }
        for i in 0..self.slaves.len() {
            self.get_mut_slaves().get_mut(i).unwrap();
        }
        true
    }
}

fn main() {
    gst::init().expect("无法初始化 GStreamer");
    gtk::init().map(|_| adw::init()).expect("无法初始化 GTK4");
    let model = AppModel {
        ..Default::default()
    };
    let relm = RelmApp::new(model);
    relm.run()
}
