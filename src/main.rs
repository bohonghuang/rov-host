use std::{cell::{Cell, RefCell}, collections::HashMap, net::Ipv4Addr, rc::Rc};

use fragile::Fragile;
use glib::{MainContext, PRIORITY_DEFAULT, PRIORITY_HIGH, Sender, Type, clone};

use gstreamer as gst;

use gtk::{AboutDialog, Align, Box as GtkBox, Button, CenterBox, Frame, Grid, Image, Inhibit, Label, MenuButton, Orientation, Stack, ToggleButton, gio::{Menu, MenuItem}, prelude::*};

use adw::{ApplicationWindow, CenteringPolicy, ColorScheme, HeaderBar, StatusPage, StyleManager, prelude::*, traits::ApplicationWindowExt};

use input::{InputEvent, InputSource, InputSourceEvent, InputSystem};
use preferences::PreferencesModel;
use relm4::{AppUpdate, ComponentUpdate, Components, Model, RelmApp, RelmComponent, Widgets, actions::{ActionGroupName, ActionName, RelmAction, RelmActionGroup}, factory::{DynamicIndex, FactoryPrototype, FactoryVec, FactoryVecDeque, positions::GridPosition}, new_action_group, new_statful_action, new_statless_action, send};
use relm4_macros::widget;
use lazy_static::{__Deref, lazy_static};

mod preferences;
mod slave;
mod components;
mod prelude;
mod video;
mod input;

use crate::{preferences::PreferencesMsg, slave::{SlaveConfigModel, SlaveConfigMsg, SlaveModel, SlaveVideoMsg}};

use sdl2::{JoystickSubsystem, Sdl, event::Event, joystick::Joystick};

use derivative::*;

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
            set_modal: true,
            connect_close_request(sender) => move |window| {
                window.hide();
                Inhibit(true)
            },
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
    recording: Option<bool>,
    #[no_eq]
    #[derivative(Default(value="FactoryVec::new()"))]
    slaves: FactoryVec<SlaveModel>,
    // #[no_eq]
    // slave_input_event_senders: Rc<RefCell<Vec<Sender<InputSourceEvent>>>>,
    #[no_eq]
    preferences: Rc<RefCell<PreferencesModel>>,
    #[no_eq]
    input_system: Rc<InputSystem>,
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

fn application_window() -> ApplicationWindow {
    ApplicationWindow::builder().build()
}

#[widget(pub)]
impl Widgets<AppModel, ()> for AppWidgets {
    view! {
        app_window = application_window() -> ApplicationWindow {
            set_title: Some("水下机器人上位机"),
            set_default_width: 1280,
            set_default_height: 720, 
            set_content = Some(&GtkBox) {
                set_orientation: Orientation::Vertical,
                append = &HeaderBar {
                    set_centering_policy: CenteringPolicy::Strict,
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
                            send!(sender, AppMsg::ToggleRecording);
                        }
                    },
                    pack_end = &MenuButton {
                        set_menu_model: Some(&main_menu),
                        set_icon_name: "open-menu-symbolic",
                        set_focus_on_click: false,
                        set_valign: Align::Center,
                    },
                    pack_end = &Button {
                        set_icon_name: "night-light-symbolic",
                        set_tooltip_text: Some("切换配色方案"),
                        connect_clicked(sender) => move |button| {
                            send!(sender, AppMsg::SwitchColorScheme);
                        }
                    },
                    pack_end = &Button {
                        set_icon_name: "window-new-symbolic",
                        set_tooltip_text: Some("新建机位"),
                        set_sensitive: track!(model.changed(AppModel::recording()), model.recording != Some(true)),
                        connect_clicked(sender) => move |button| {
                            send!(sender, AppMsg::NewSlave);
                        },
                    },
                },
                append = &Stack  {
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
            },
            connect_close_request(sender) => move |window| {
                send!(sender, AppMsg::StopInputSystem);
                Inhibit(false)
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
        
        let (input_event_sender, input_event_receiver) = MainContext::channel(PRIORITY_DEFAULT);
        *model.input_system.event_sender.borrow_mut() = Some(input_event_sender);
        
        input_event_receiver.attach(None, clone!(@strong sender => move |event| {
            send!(sender, AppMsg::DispatchInputEvent(event));
            Continue(true)
        }));
    }
}

pub enum AppMsg {
    NewSlave,
    SlaveConfigUpdated(usize, SlaveConfigModel),
    SlaveToggleConnect(usize),
    SlaveTogglePolling(usize),
    SlaveSetInputSource(usize, Option<InputSource>),
    SlaveUpdateInputSources(usize),
    DispatchInputEvent(InputEvent),
    PreferencesUpdated(PreferencesModel),
    ToggleRecording, 
    OpenAboutDialog,
    OpenPreferencesWindow,
    OpenKeybindingsWindow,
    SwitchColorScheme,
    StopInputSystem, 
}

#[derive(relm4_macros::Components)]
pub struct AppComponents {
    about: RelmComponent::<AboutModel, AppModel>,
    preferences: RelmComponent::<PreferencesModel, AppModel>,
}


impl AppUpdate for AppModel {
    fn update(
        &mut self,
        msg: AppMsg,
        components: &AppComponents,
        sender: Sender<AppMsg>,
    ) -> bool {
        match msg {
            AppMsg::OpenAboutDialog => {
                components.about.root_widget().present();
            },
            AppMsg::OpenPreferencesWindow => {
                components.preferences.root_widget().present();
            },
            AppMsg::OpenKeybindingsWindow => todo!(),
            AppMsg::NewSlave => {
                let mut ip_octets = self.get_preferences().borrow().get_default_slave_ipv4_address().octets();
                let index = self.get_slaves().len() as u8;
                ip_octets[3] = ip_octets[3].wrapping_add(index);
                let video_port = self.get_preferences().borrow().get_default_local_video_port().wrapping_add(index as u16);
                let slave = SlaveModel::new(index as usize, SlaveConfigModel::new(index as usize, Ipv4Addr::from(ip_octets), *self.get_preferences().clone().borrow().get_default_slave_port(), video_port), self.get_preferences().clone(), self.input_system.clone());
                self.get_mut_slaves().push(slave);
                self.set_recording(Some(false));
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
            AppMsg::ToggleRecording => match self.recording {
                Some(recording) => {
                    for components in slave::COMPONENTS.get().borrow().deref() {
                        components.video.send(SlaveVideoMsg::SetRecording(!recording)).unwrap();
                    }
                    self.set_recording(Some(!recording));
                },
                None => (),
            },
            AppMsg::SwitchColorScheme => {
                let style_manager = StyleManager::default().unwrap();
                style_manager.set_color_scheme(if style_manager.is_dark() { ColorScheme::PreferLight } else { ColorScheme::ForceDark });
            },
            AppMsg::SlaveSetInputSource(index, source) => {
                if let Some(slave) = self.get_mut_slaves().get_mut(index) {
                    slave.set_input_source(source);
                }
            },
            AppMsg::SlaveUpdateInputSources(index) => {
                if let Some(slave) = self.get_mut_slaves().get_mut(index) {
                    slave.set_input_system(slave.get_input_system().clone());
                }
            },
            AppMsg::DispatchInputEvent(InputEvent(source, event)) => {
                for slave in self.slaves.iter() {
                    if let Some(target_input_source) = slave.get_input_source() {
                        if target_input_source.eq(&source) {
                            slave.input_event_sender.send(event.clone()).unwrap();
                        }
                    }
                }
            },
            AppMsg::StopInputSystem => {
                self.input_system.stop();
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
    // sdl2::init().and_then(|sdl| init_joystick(&sdl)).expect("无法初始化 SDL2 用于手柄输入");
    let model = AppModel {
        ..Default::default()
    };
    model.input_system.run();
    
    let relm = RelmApp::new(model);
    // let win = ApplicationWindow::builder().build();
    // win.set_content(content)
    relm.run()
}
