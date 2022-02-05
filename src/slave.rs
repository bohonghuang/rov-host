use std::{cell::{Cell, RefCell}, collections::HashMap, net::Ipv4Addr, path::PathBuf, rc::Rc, str::FromStr, sync::{Arc, Mutex}, fmt::Debug, thread, time::Duration};

use fragile::Fragile;
use glib::{MainContext, Object, PRIORITY_DEFAULT, Sender, Type, clone::{self, Upgrade}, WeakRef};

use gstreamer as gst;
use gst::{Pipeline, prelude::*};
use gtk::{AboutDialog, Align, Box as GtkBox, Button, CenterBox, CheckButton, Dialog, DialogFlags, Entry, Frame, Grid, Image, Inhibit, Label, ListBox, MenuButton, Orientation, Overlay, Popover, ResponseType, Revealer, RevealerTransitionType, ScrolledWindow, SelectionModel, Separator, SingleSelection, SpinButton, Stack, StringList, Switch, ToggleButton, Viewport, gdk_pixbuf::Pixbuf, gio::{Menu, MenuItem, MenuModel}, prelude::*, Picture, FileFilter, ProgressBar, MessageDialog};

use adw::{ActionRow, ComboRow, HeaderBar, PreferencesGroup, PreferencesPage, PreferencesWindow, StatusPage, Window, prelude::*, Carousel, ApplicationWindow, Clamp, ToastOverlay};

use relm4::{AppUpdate, WidgetPlus, ComponentUpdate, Components, Model, RelmApp, RelmComponent, Widgets, actions::{ActionGroupName, ActionName, RelmAction, RelmActionGroup}, factory::{DynamicIndex, FactoryPrototype, FactoryVec, FactoryVecDeque, positions::GridPosition}, new_action_group, send, MicroWidgets, MicroModel, MicroComponent};

use relm4_macros::{widget, micro_widget};

use strum::IntoEnumIterator;
use strum_macros::{EnumIter, EnumString as EnumFromString, Display as EnumToString};

use crate::{AppModel, input::{InputEvent, InputSource, InputSourceEvent, InputSystem}, preferences::{PreferencesMsg, PreferencesModel}, video::{self, MatExt, VideoDecoder}};
use crate::AppMsg;
use crate::prelude::ObjectExt;

use glib_macros::clone;

use derivative::*;

use self::param_tuner::SlaveParameterTunerModel;

#[tracker::track(pub)]
#[derive(Debug, Derivative)]
#[derivative(Default)]
pub struct SlaveModel {
    #[no_eq]
    #[derivative(Default(value="MyComponent::new(Default::default(), MainContext::channel(PRIORITY_DEFAULT).0)"))]
    pub config: MyComponent<SlaveConfigModel>,
    #[no_eq]
    #[derivative(Default(value="MyComponent::new(Default::default(), MainContext::channel(PRIORITY_DEFAULT).0)"))]
    pub video: MyComponent<SlaveVideoModel>,
    #[derivative(Default(value="Some(false)"))]
    pub connected: Option<bool>,
    #[derivative(Default(value="Some(false)"))]
    pub polling: Option<bool>,
    #[no_eq]
    pub preferences: Rc<RefCell<PreferencesModel>>,
    pub input_source: Option<InputSource>,
    #[no_eq]
    pub input_system: Rc<InputSystem>,
    #[no_eq]
    #[derivative(Default(value="MainContext::channel(PRIORITY_DEFAULT).0"))]
    pub input_event_sender: Sender<InputSourceEvent>,
    pub slave_info_displayed: bool,
    #[no_eq]
    pub status: Arc<Mutex<HashMap<SlaveStatusClass, i16>>>,
}

#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub enum SlaveStatusClass {
    MotionX, MotionY, MotionZ, MotionRotate,
    DepthLocked, DirectionLocked,
}

impl SlaveStatusClass {
    pub fn from_button(button: u8) -> Option<SlaveStatusClass> {
        match button {
            7 => Some(SlaveStatusClass::DepthLocked),
            8 => Some(SlaveStatusClass::DirectionLocked),
            _ => None,
        }
    }
    pub fn from_axis(axis: u8) -> Option<SlaveStatusClass> {
        match axis {
            0 => Some(SlaveStatusClass::MotionX),
            1 => Some(SlaveStatusClass::MotionY),
            2 => Some(SlaveStatusClass::MotionRotate),
            3 => Some(SlaveStatusClass::MotionZ),
            _ => None
        }
    }
}

const JOYSTICK_DISPLAY_THRESHOLD: i16 = 500;

impl SlaveModel {
    pub fn new(config: SlaveConfigModel, preferences: Rc<RefCell<PreferencesModel>>, component_sender: &Sender<SlaveMsg>, input_event_sender: Sender<InputSourceEvent>) -> Self {
        Self {
            config: MyComponent::new(config, component_sender.clone()),
            video: MyComponent::new(Default::default(), component_sender.clone()),
            preferences,
            input_event_sender,
            status: Arc::new(Mutex::new(HashMap::new())),
            ..Default::default()
        }
    }
    
    pub fn get_target_status_or_insert_0(&mut self, status_class: &SlaveStatusClass) -> i16 {
        let mut status = self.status.lock().unwrap();
        *status.entry(status_class.clone()).or_insert(0)
    }

    pub fn get_target_status(&self, status_class: &SlaveStatusClass) -> i16 {
        let status = self.status.lock().unwrap();
        *status.get(status_class).unwrap_or(&0)
    }
    pub fn set_target_status(&mut self, status_class: &SlaveStatusClass, new_status: i16) {
        let mut status = self.status.lock().unwrap();
        *status.entry(status_class.clone()).or_insert(0) = new_status;
    }
    
}
#[derive(EnumIter, EnumToString, EnumFromString, PartialEq, Clone, Debug)]
pub enum VideoAlgorithm {
    CLAHE, Algorithm1, Algorithm2, Algorithm3, Algorithm4
}

pub fn input_sources_list_box(input_source: &Option<InputSource>, input_system: &InputSystem, sender: &Sender<SlaveMsg>) -> ListBox {
    let sources = input_system.get_sources().unwrap();
    let list_box = ListBox::builder().build();
    let mut radio_button_group: Option<CheckButton> = None;
    for (source, name) in sources {
        let radio_button = CheckButton::builder().label(&name).build();
        let sender = sender.clone();
        radio_button.set_active(match input_source {
            Some(current_souce) => current_souce.eq(&source),
            None => false,
        });
        
        radio_button.connect_toggled(move |button| {
            sender.send(SlaveMsg::SetInputSource(if button.is_active() { Some(source.clone()) } else { None } )).unwrap();
        });
        {
            let radio_button = radio_button.clone();
            match &radio_button_group {
                Some(button) => radio_button.set_group(Some(button)),
                None => radio_button_group = Some(radio_button),
            }
        }
        list_box.append(&radio_button);
    }
    list_box
}

#[micro_widget(pub)]
impl MicroWidgets<SlaveModel> for SlaveWidgets {
    view! {
        vbox = GtkBox {
            put_data: args!("sender", sender.clone()),
            set_orientation: Orientation::Vertical,
            append = &CenterBox {
                set_css_classes: &["toolbar"],
                set_orientation: Orientation::Horizontal,
                set_start_widget = Some(&GtkBox) {
                    set_hexpand: true,
                    set_halign: Align::Start,
                    set_spacing: 5,
                    append = &Button {
                        set_icon_name?: watch!(model.connected.map(|x| if x { "network-offline-symbolic" } else { "network-transmit-symbolic" })),
                        set_sensitive: track!(model.changed(SlaveModel::connected()), model.connected != None),
                        set_css_classes: &["circular"],
                        set_tooltip_text: Some("连接/断开连接"),
                        connect_clicked(sender) => move |button| {
                            send!(sender, SlaveMsg::ToggleConnect);
                        },
                    },
                    append = &Button {
                        set_icon_name?: watch!(model.polling.map(|x| if x { "media-playback-pause-symbolic" } else { "media-playback-start-symbolic" })),
                        set_sensitive: track!(model.changed(SlaveModel::polling()), model.polling != None),
                        set_css_classes: &["circular"],
                        set_tooltip_text: Some("启动/停止视频"),
                        connect_clicked(sender) => move |button| {
                            send!(sender, SlaveMsg::TogglePolling);
                        },
                    },
                },
                set_center_widget = Some(&GtkBox) {
                    set_hexpand: true,
                    set_halign: Align::Center,
                    set_spacing: 5,
                    append = &Label {
                        set_text: track!(model.changed(SlaveModel::config()), format!("{}:{}", model.config.model().get_ip(), model.config.model().get_port()).as_str()),
                    },
                    append = &MenuButton {
                        set_icon_name: "input-gaming-symbolic",
                        set_css_classes: &["circular"],
                        set_tooltip_text: Some("切换当前机位使用的输入设备"),
                        set_popover = Some(&Popover) {
                            set_child = Some(&GtkBox) {
                                set_spacing: 5,
                                set_orientation: Orientation::Vertical, 
                                append = &CenterBox {
                                    set_center_widget = Some(&Label) {
                                        set_margin_start: 10,
                                        set_margin_end: 10,
                                        set_text: "输入设备"
                                    },
                                    set_end_widget = Some(&Button) {
                                        set_icon_name: "view-refresh-symbolic",
                                        set_css_classes: &["circular"],
                                        set_tooltip_text: Some("刷新输入设备"),
                                        connect_clicked(sender) => move |button| {
                                            send!(sender, SlaveMsg::UpdateInputSources);
                                        },
                                    },
                                },
                                append = &Frame {
                                    set_child: track!(model.changed(SlaveModel::input_system()), Some(&input_sources_list_box(&model.input_source, &model.input_system ,&sender))),
                                },
                                
                            },
                        },
                    },
                },
                set_end_widget = Some(&GtkBox) {
                    set_hexpand: true,
                    set_halign: Align::End,
                    set_spacing: 5,
                    set_margin_end: 5,
                    append = &Button {
                        set_icon_name: "software-update-available-symbolic",
                        set_css_classes: &["circular"],
                        set_tooltip_text: Some("固件更新"),
                        connect_clicked(sender) => move |button| {
                            send!(sender, SlaveMsg::OpenFirmwareUpater(button.clone()));
                        },
                    },
                    append = &Button {
                        set_icon_name: "utilities-system-monitor-symbolic",
                        set_css_classes: &["circular"],
                        set_tooltip_text: Some("参数调校"),
                        connect_clicked(sender) => move |button| {
                            send!(sender, SlaveMsg::OpenParameterTuner);
                        },
                    },
                    append = &ToggleButton {
                        set_icon_name: "emblem-system-symbolic",
                        set_css_classes: &["circular"],
                        set_tooltip_text: Some("机位设置"),
                        put_data: args!("sender", model.config.sender().clone()),
                        connect_active_notify => move |button| {
                            let sender = button.get_data::<Sender<SlaveConfigMsg>>("sender").unwrap().clone();
                            send!(sender, SlaveConfigMsg::TogglePresented);
                        },
                    },
                    append = &ToggleButton {
                        set_icon_name: "window-close-symbolic",
                        set_css_classes: &["circular"],
                        set_tooltip_text: Some("移除机位"),
                        set_visible: false,
                        connect_active_notify(sender) => move |button| {
                            send!(sender, SlaveMsg::DestroySlave);
                        },
                    },
                },
            },
            append = &GtkBox {
                set_orientation: Orientation::Horizontal,
                append = &Overlay {
                    set_child: Some(model.video.root_widget()),
                    add_overlay = &GtkBox {
                        set_valign: Align::Start,
                        set_halign: Align::End,
                        set_hexpand: true,
                        set_margin_all: 20, 
                        append = &Frame {
                            set_css_classes: &["card"],
                            set_child = Some(&GtkBox) {
                                set_orientation: Orientation::Vertical,
                                set_margin_all: 5,
                                set_width_request: 50,
                                set_spacing: 5,
                                append = &Button {
                                    set_child = Some(&CenterBox) {
                                        set_center_widget = Some(&Label) {
                                            set_margin_start: 10,
                                            set_margin_end: 10,
                                            set_text: "机位信息",
                                        },
                                        set_end_widget = Some(&Image) {
                                            set_icon_name: watch!(Some(if model.slave_info_displayed { "go-down-symbolic" } else { "go-next-symbolic" })),
                                        },
                                    },
                                    connect_clicked(sender) => move |button| {
                                        send!(sender, SlaveMsg::ToggleDisplayInfo);
                                    },
                                },
                                append = &Revealer {
                                    set_reveal_child: watch!(model.slave_info_displayed),
                                    set_child = Some(&GtkBox) {
                                        set_spacing: 2,
                                        set_margin_all: 5,
                                        set_orientation: Orientation::Vertical,
                                        set_halign: Align::Center,
                                        append = &Frame {
                                            set_hexpand: true,
                                            set_halign: Align::Center,
                                            set_child = Some(&Grid) {
                                                set_margin_all: 2,
                                                set_row_spacing: 2,
                                                set_column_spacing: 2,
                                                attach(0, 0, 1, 1) = &ToggleButton {
                                                    set_icon_name: "object-rotate-left-symbolic",
                                                    set_active: watch!(model.get_target_status(&SlaveStatusClass::MotionRotate) < -JOYSTICK_DISPLAY_THRESHOLD),
                                                },
                                                attach(2, 0, 1, 1) = &ToggleButton {
                                                    set_icon_name: "object-rotate-right-symbolic",
                                                    set_active: watch!(model.get_target_status(&SlaveStatusClass::MotionRotate) > JOYSTICK_DISPLAY_THRESHOLD),
                                                },
                                                attach(0, 2, 1, 1) = &ToggleButton {
                                                    set_icon_name: "go-bottom-symbolic",
                                                    set_active: watch!(model.get_target_status(&SlaveStatusClass::MotionZ) < -JOYSTICK_DISPLAY_THRESHOLD),
                                                },
                                                attach(2, 2, 1, 1) = &ToggleButton {
                                                    set_icon_name: "go-top-symbolic",
                                                    set_active: watch!(model.get_target_status(&SlaveStatusClass::MotionZ) > JOYSTICK_DISPLAY_THRESHOLD),
                                                },
                                                attach(1, 0, 1, 1) = &ToggleButton {
                                                    set_icon_name: "go-up-symbolic",
                                                    set_active: watch!(model.get_target_status(&SlaveStatusClass::MotionY) > JOYSTICK_DISPLAY_THRESHOLD),
                                                },
                                                attach(0, 1, 1, 1) = &ToggleButton {
                                                    set_icon_name: "go-previous-symbolic",
                                                    set_active: watch!(model.get_target_status(&SlaveStatusClass::MotionX) < -JOYSTICK_DISPLAY_THRESHOLD),
                                                },
                                                attach(2, 1, 1, 1) = &ToggleButton {
                                                    set_icon_name: "go-next-symbolic",
                                                    set_active: watch!(model.get_target_status(&SlaveStatusClass::MotionX) > JOYSTICK_DISPLAY_THRESHOLD),
                                                },
                                                attach(1, 2, 1, 1) = &ToggleButton {
                                                    set_icon_name: "go-down-symbolic",
                                                    set_active: watch!(model.get_target_status(&SlaveStatusClass::MotionY) < -JOYSTICK_DISPLAY_THRESHOLD),
                                                },
                                            },
                                        },
                                        append = &Label {
                                            set_halign: Align::Start,
                                            set_text: "机位信息 1",
                                        },
                                        append = &Label {
                                            set_halign: Align::Start,
                                            set_text: "机位信息 2",
                                        },
                                        append = &Label {
                                            set_halign: Align::Start,
                                            set_text: "机位信息 3",
                                        },
                                        append = &Label {
                                            set_halign: Align::Start,
                                            set_text: "机位信息 4",
                                        },
                                        append = &Label {
                                            set_halign: Align::Start,
                                            set_text: "机位信息 5",
                                        },
                                        append = &CenterBox {
                                            set_hexpand: true,
                                            set_start_widget = Some(&Label) {
                                                set_text: "深度锁定",
                                            },
                                            set_end_widget = Some(&Switch) {
                                                set_active: watch!(model.get_target_status(&SlaveStatusClass::DepthLocked) != 0),
                                            },
                                        },
                                        append = &CenterBox {
                                            set_hexpand: true,
                                            set_start_widget = Some(&Label) {
                                                set_text: "方向锁定",
                                            },
                                            set_end_widget = Some(&Switch) {
                                                set_active: watch!(model.get_target_status(&SlaveStatusClass::DirectionLocked) != 0),
                                            },
                                        },
                                    },
                                },
                            }
                        }
                    }
                }, 
                append: model.config.root_widget(),
            },
        }
    }
}

impl std::fmt::Debug for SlaveWidgets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.vbox.fmt(f)
    }
}

pub enum SlaveMsg {
    ConfigUpdated,
    ToggleConnect,
    TogglePolling,
    SetInputSource(Option<InputSource>),
    UpdateInputSources,
    ToggleDisplayInfo,
    InputReceived(InputSourceEvent),
    OpenFirmwareUpater(Button),
    OpenParameterTuner,
    DestroySlave,
    VideoPipelineError(String),
}

impl MicroModel for SlaveModel {
    type Msg = SlaveMsg;
    type Widgets = SlaveWidgets;
    type Data = (Sender<AppMsg>, WeakRef<ApplicationWindow>);
    fn update(&mut self, msg: SlaveMsg, (parent_sender, window): &Self::Data, sender: Sender<SlaveMsg>) {
        match msg {
            SlaveMsg::ConfigUpdated => {
                let config = self.get_mut_config().model().clone();
                self.video.send(SlaveVideoMsg::ConfigUpdated(config));
            },
            SlaveMsg::ToggleConnect => {
                match self.get_connected() {
                    Some(true) =>{
                        self.set_connected(Some(false));
                        self.config.send(SlaveConfigMsg::SetConnected(Some(false))).unwrap();
                    },
                    Some(false) => {
                        self.set_connected(Some(true));
                        self.config.send(SlaveConfigMsg::SetConnected(Some(true))).unwrap();
                    },
                    None => (),
                }
            },
            SlaveMsg::TogglePolling => {
                match self.get_polling() {
                    Some(true) =>{
                        self.video.send(SlaveVideoMsg::StopPipeline).unwrap();
                        self.set_polling(Some(false));
                        self.config.send(SlaveConfigMsg::SetPolling(Some(false))).unwrap();
                    },
                    Some(false) => {
                        self.video.send(SlaveVideoMsg::StartPipeline).unwrap();
                        self.set_polling(Some(true));
                        self.config.send(SlaveConfigMsg::SetPolling(Some(true))).unwrap();
                    },
                    None => (),
                }
            },
            SlaveMsg::SetInputSource(source) => {
                self.set_input_source(source);
            },
            SlaveMsg::UpdateInputSources => {
                self.set_input_system(self.get_input_system().clone());
            },
            SlaveMsg::ToggleDisplayInfo => {
                self.set_slave_info_displayed(!*self.get_slave_info_displayed());
            },
            SlaveMsg::InputReceived(event) => {
                match event {
                    InputSourceEvent::ButtonChanged(button, pressed) => {
                        if let Some(status_class) = SlaveStatusClass::from_button(button) {
                            if pressed {
                                self.set_target_status(&status_class, !(self.get_target_status(&status_class) != 0) as i16);
                            }
                        }
                    },
                    InputSourceEvent::AxisChanged(axis, value) => {
                        if let Some(status_class) = SlaveStatusClass::from_axis(axis) {
                            self.set_target_status(&status_class, value.saturating_mul(if axis == 1 || axis == 3 { -1 } else { 1 }));
                        }
                    },
                }
                self.set_status(self.get_status().clone());
            },
            SlaveMsg::OpenFirmwareUpater(button) => {
                let component = MicroComponent::new(SlaveFirmwareUpdaterModel::new(), ());
                component.root_widget().set_transient_for(Some(&window.upgrade().unwrap()));
            },
            SlaveMsg::OpenParameterTuner => {
                let component = MicroComponent::new(SlaveParameterTunerModel::new(), ());
                component.root_widget().set_transient_for(Some(&window.upgrade().unwrap()));
            },
            SlaveMsg::DestroySlave => {
                send!(parent_sender, AppMsg::DestroySlave(self as *const Self));
            },
            SlaveMsg::VideoPipelineError(msg) => {
                relm4_macros::view! {
                    dialog = MessageDialog {
                        set_message_type: gtk::MessageType::Error,
                        set_text: Some(&msg),
                        set_title: Some("启动视频流错误"),
                        set_transient_for: window.upgrade().as_ref(),
                    }
                }
                self.set_polling(Some(false));
                self.config.send(SlaveConfigMsg::SetPolling(Some(false))).unwrap();
                dialog.present();
            },
        }
    }
}

pub enum SlaveFirmwareUpdaterMsg {
    NextStep,
    FirmwareFileSelected(PathBuf),
    FirmwareUploadProgressUpdated(f32),
}

#[tracker::track(pub)]
#[derive(Debug, Derivative)]
#[derivative(Default)]
pub struct SlaveFirmwareUpdaterModel {
    current_page: u32,
    firmware_file_path: Option<PathBuf>,
    firmware_uploading_progress: f32,
}

impl SlaveFirmwareUpdaterModel {
    pub fn new() -> SlaveFirmwareUpdaterModel {
        SlaveFirmwareUpdaterModel { ..Default::default() }
    }
}

impl MicroModel for SlaveFirmwareUpdaterModel {
    type Msg = SlaveFirmwareUpdaterMsg;
    type Widgets = SlaveFirmwareUpdaterWidgets;
    type Data = ();
    
    fn update(&mut self, msg: SlaveFirmwareUpdaterMsg, data: &(), sender: Sender<SlaveFirmwareUpdaterMsg>) {
        match msg {
            SlaveFirmwareUpdaterMsg::NextStep => self.set_current_page(self.get_current_page().wrapping_add(1)),
            SlaveFirmwareUpdaterMsg::FirmwareFileSelected(path) => self.set_firmware_file_path(Some(path)),
            SlaveFirmwareUpdaterMsg::FirmwareUploadProgressUpdated(progress) => {
                self.set_firmware_uploading_progress(progress);
                if progress >= 1.0 {
                    send!(sender, SlaveFirmwareUpdaterMsg::NextStep);
                }
            },
        }
    }
}

trait CarouselExt {
    fn scroll_to_page(&self, page_index: u32, animate: bool);
}

impl CarouselExt for Carousel {
    fn scroll_to_page(&self, page_index: u32, animate: bool) {
        self.scroll_to(&self.nth_page(page_index), animate);
    }
}

fn open_file<T, F>(filters: &[FileFilter], parent_window: &T, callback: F)
where T: IsA<gtk::Window>,
      F: 'static + Fn(Option<PathBuf>) -> () {
    relm4_macros::view! {
        file_chooser = gtk::FileChooserNative {
            set_action: gtk::FileChooserAction::Open,
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
                        callback(None)
                    },
                    _ => (),
                }
            },
        }
    }
    file_chooser.show();
    std::mem::forget(file_chooser); // TODO: 内存泄露处理
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
                            connect_clicked(sender) => move |button| {
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
                                        connect_clicked(sender, window) => move |button| {
                                            let filter = FileFilter::new();
                                            filter.add_suffix("bin");
                                            filter.set_name(Some("固件文件"));
                                            open_file(&[filter], &window, clone!(@strong sender => move |path| {
                                                match path {
                                                    Some(path) => {
                                                        send!(sender, SlaveFirmwareUpdaterMsg::FirmwareFileSelected(path));
                                                    },
                                                    None => (),
                                                }
                                            }));
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
                                connect_clicked(sender) => move |button| {
                                    send!(sender, SlaveFirmwareUpdaterMsg::NextStep);
                                    thread::spawn(clone!(@strong sender => move || {
                                        for i in 0 ..= 100 {
                                            send!(sender, SlaveFirmwareUpdaterMsg::FirmwareUploadProgressUpdated((i as f32) / 100.0));
                                            thread::sleep(Duration::from_millis(50));
                                        }
                                    }));
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
                            connect_clicked(window) => move |button| {
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

pub struct MyComponent<T: MicroModel> {
    pub component: MicroComponent<T>,
}

impl <Model> MyComponent<Model>
    where
Model::Widgets: MicroWidgets<Model> + 'static,
Model::Msg: 'static,
Model::Data: 'static,
Model: MicroModel + 'static,  {
    fn model(&self) -> std::cell::Ref<'_, Model> {
        self.component.model().unwrap()
    }
    fn model_mut(&self) -> std::cell::RefMut<'_, Model> {
        self.component.model_mut().unwrap()
    }
    fn widgets(&self) -> std::cell::RefMut<'_, Model::Widgets> {
        self.component.widgets().unwrap()
    }
}

impl <T: MicroModel> std::ops::Deref for MyComponent<T> {
    type Target = MicroComponent<T>;
    fn deref(&self) -> &MicroComponent<T> {
        &self.component
    }
}



impl <T: MicroModel> Debug for MyComponent<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MyComponent").finish()
    }
}

impl <Model> Default for MyComponent<Model>
where
    Model::Widgets: MicroWidgets<Model> + 'static,
    Model::Msg: 'static,
    Model::Data: Default + 'static,
    Model: MicroModel + Default + 'static, {
    fn default() -> Self {
        MyComponent { component: MicroComponent::new(Model::default(), Model::Data::default()) }
    }
}

impl <Model> MyComponent<Model>
where
    Model::Widgets: MicroWidgets<Model> + 'static,
    Model::Msg: 'static,
    Model::Data: 'static,
    Model: MicroModel + 'static, {
    pub fn new(model: Model, data: Model::Data) -> MyComponent<Model> {
        MyComponent { component: MicroComponent::new(model, data) }
    }
}

impl FactoryPrototype for MyComponent<SlaveModel> {
    type Factory = FactoryVec<Self>;
    type Widgets = GtkBox;
    type Root = GtkBox;
    type View = Grid;
    type Msg = AppMsg;

    fn init_view(
        &self,
        index: &usize,
        sender: Sender<AppMsg>,
    ) -> GtkBox {
        self.component.root_widget().clone()
    }

    fn position(
        &self,
        index: &usize,
    ) -> GridPosition {
        let index = *index as i32;
        let row = index / 3;
        let column = index % 3;
        GridPosition {
            column,
            row,
            width: 1,
            height: 1,
        }
    }

    fn view(
        &self,
        index: &usize,
        widgets: &GtkBox,
    ) {
        self.component.update_view().unwrap();
    }

    fn root_widget(widgets: &GtkBox) -> &GtkBox {
        widgets
    }
}

#[tracker::track(pub)]
#[derive(Debug, Derivative, PartialEq, Clone)]
#[derivative(Default)]
pub struct SlaveConfigModel {
    #[derivative(Default(value="Some(false)"))]
    polling: Option<bool>,
    #[derivative(Default(value="Some(false)"))]
    connected: Option<bool>,
    presented: bool,
    #[derivative(Default(value="PreferencesModel::default().default_slave_ipv4_address"))]
    pub ip: Ipv4Addr,
    #[derivative(Default(value="PreferencesModel::default().default_slave_port"))]
    pub port: u16,
    #[derivative(Default(value="5600"))]
    pub video_port: u16,
    pub video_algorithms: Vec<VideoAlgorithm>,
    #[derivative(Default(value="PreferencesModel::default().default_keep_video_display_ratio"))]
    pub keep_video_display_ratio: bool,
    pub video_decoder: VideoDecoder,
}

impl SlaveConfigModel {
    pub fn new(ip: Ipv4Addr, port: u16, video_port: u16) -> Self {
        Self {
            ip, port, video_port,
            ..Default::default()
        }
    }
}

impl MicroModel for SlaveConfigModel {
    type Msg = SlaveConfigMsg;
    type Widgets = SlaveConfigWidgets;
    type Data = Sender<SlaveMsg>;
    fn update(&mut self, msg: SlaveConfigMsg, parent_sender: &Sender<SlaveMsg>, sender: Sender<SlaveConfigMsg>) {
        match msg {
            SlaveConfigMsg::SetIp(ip) => self.set_ip(ip),
            SlaveConfigMsg::SetPort(port) => self.set_port(port),
            SlaveConfigMsg::SetVideoPort(port) => self.set_video_port(port),
            SlaveConfigMsg::TogglePresented => self.set_presented(!self.get_presented()),
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
    SetVideoPort(u16),
    SetKeepVideoDisplayRatio(bool),
    TogglePresented,
    SetPolling(Option<bool>),
    SetConnected(Option<bool>),
    SetVideoAlgorithm(Option<VideoAlgorithm>),
    SetVideoDecoder(VideoDecoder),
}

#[micro_widget(pub)]
impl MicroWidgets<SlaveConfigModel> for SlaveConfigWidgets {
    view! {
        window = Revealer {
            set_reveal_child: watch!(model.presented), //track!(model.changed(SlaveConfigModel::window_presented()), model.window_presented),
            set_transition_type: RevealerTransitionType::SlideLeft,
            set_child = Some(&GtkBox) {
                set_orientation: Orientation::Horizontal,
                append = &Separator {
                    set_orientation: Orientation::Horizontal,
                },
                append = &ScrolledWindow {
                    set_width_request: 300,
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
                                        connect_state_set(sender) => move |switch, state| {
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
                                set_title: "拉流",
                                set_description: Some("从下位机拉取视频流的选项"),
                                add = &ActionRow {
                                    set_title: "端口",
                                    set_subtitle: "拉取视频流的本地端口",
                                    add_suffix = &SpinButton::with_range(0.0, 65535.0, 1.0) {
                                        set_value: track!(model.changed(SlaveConfigModel::video_port()), model.video_port as f64),
                                        set_digits: 0,
                                        set_valign: Align::Center,
                                        connect_value_changed(sender) => move |button| {
                                            send!(sender, SlaveConfigMsg::SetVideoPort(button.value() as u16));
                                        }
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
                                }
                            },
                        },
                    },
                },
            },
            // connect_close_request(sender) => move |window| {
            //     send!(sender, SlaveConfigMsg::SetWindowPresented(false));
            //     Inhibit(false)
            // },
        }
    }
}

#[tracker::track(pub)]
#[derive(Debug, Derivative)]
#[derivative(Default)]
pub struct SlaveVideoModel {
    #[no_eq]
    pub pixbuf: Option<Pixbuf>,
    #[no_eq]
    pub pipeline: Option<Pipeline>,
    #[no_eq]
    pub config: Arc<Mutex<SlaveConfigModel>>,
    pub record_handle: Option<(gst::Pad, Vec<gst::Element>)>,
    pub preferences: Rc<RefCell<PreferencesModel>>, 
}

pub enum SlaveVideoMsg {
    StartPipeline,
    StopPipeline,
    SetPixbuf(Option<Pixbuf>),
    StartRecord(String),
    StopRecord,
    ConfigUpdated(SlaveConfigModel),
}

impl MicroModel for SlaveVideoModel {
    type Msg = SlaveVideoMsg;
    type Widgets = SlaveVideoWidgets;
    type Data = Sender<SlaveMsg>;

    fn update(&mut self, msg: SlaveVideoMsg, parent_sender: &Sender<SlaveMsg>, sender: Sender<SlaveVideoMsg>) {
        match msg {
            SlaveVideoMsg::SetPixbuf(pixbuf) => self.set_pixbuf(pixbuf),
            SlaveVideoMsg::StartRecord(file_name) => {
                if let Some(pipeline) = &self.pipeline {
                    let mut pathbuf = PathBuf::from_str(self.preferences.borrow().get_video_save_path()).unwrap();
                    pathbuf.push(format!("{}.mkv", file_name));
                    println!("{}", pathbuf.to_str().unwrap());
                    let elements = video::create_queue_to_file(pathbuf.to_str().unwrap()).unwrap();
                    let pad = video::connect_elements_to_pipeline(pipeline, &elements).unwrap();
                    pipeline.set_state(gst::State::Playing).unwrap(); // 添加元素后会自动暂停，需要手动重新开始播放
                    self.record_handle = Some((pad, Vec::from(elements)));
                }
            },
            SlaveVideoMsg::StopRecord => {
                if let Some(pipeline) = &self.pipeline {
                    if let Some((teepad, elements)) = &self.record_handle{
                        video::disconnect_elements_to_pipeline(pipeline, teepad, elements).unwrap();
                    }
                }
            },
            SlaveVideoMsg::ConfigUpdated(config) => {
                *self.get_mut_config().lock().unwrap() = config;
            },
            SlaveVideoMsg::StartPipeline => {
                assert!(self.pipeline == None);
                let video_port = self.get_config().lock().unwrap().get_video_port().clone();
                let video_decoder = self.get_config().lock().unwrap().get_video_decoder().clone();
                match video::create_pipeline(video_port, video_decoder) {
                    Ok(pipeline) => {
                        let sender = sender.clone();
                        let (mat_sender, mat_receiver) = MainContext::channel(glib::PRIORITY_DEFAULT);
                        video::attach_pipeline_callback(&pipeline, mat_sender, self.get_config().clone()).unwrap();
                        mat_receiver.attach(None, move |mat| {
                            sender.send(SlaveVideoMsg::SetPixbuf(Some(mat.as_pixbuf()))).unwrap();
                            Continue(true)
                        });
                        pipeline.set_state(gst::State::Playing).unwrap();
                        self.set_pipeline(Some(pipeline));
                    },
                    Err(msg) => {
                        send!(parent_sender, SlaveMsg::VideoPipelineError(String::from(msg)));
                    },
                }
            },
            SlaveVideoMsg::StopPipeline => {
                assert!(self.pipeline != None);
                if let Some(pipeline) = &self.pipeline {
                    pipeline.set_state(gst::State::Null);
                    self.pipeline = None;
                }
            },
        }
    }
}

impl std::fmt::Debug for SlaveVideoWidgets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.root_widget().fmt(f)
    }
}

#[micro_widget(pub)]
impl MicroWidgets<SlaveVideoModel> for SlaveVideoWidgets {
    view! {
        frame = GtkBox {
            append = &Stack {
                set_vexpand: true,
                set_hexpand: true,
                add_child = &StatusPage {
                    set_icon_name: Some("help-browser-symbolic"),
                    set_title: "无信号",
                    set_description: Some("请点击上方按钮启动视频拉流"),
                    set_visible: track!(model.changed(SlaveVideoModel::pixbuf()), model.pixbuf == None),
                },
                add_child = &Picture {
                    set_hexpand: true,
                    set_vexpand: true,
                    set_can_shrink: true,
                    set_keep_aspect_ratio: track!(model.changed(SlaveVideoModel::config()), *model.config.lock().unwrap().get_keep_video_display_ratio()),
                    set_pixbuf: track!(model.changed(SlaveVideoModel::pixbuf()), match &model.pixbuf {
                        Some(pixbuf) => Some(&pixbuf),
                        None => None,
                    }),
                },
            },
        }
    }
}

pub mod param_tuner {
    use std::{cell::{Cell, RefCell}, collections::HashMap, net::Ipv4Addr, path::PathBuf, rc::Rc, str::FromStr, sync::{Arc, Mutex}, fmt::Debug, thread, time::Duration, cmp::{max, min}};
    
    use fragile::Fragile;
    use glib::{MainContext, Object, PRIORITY_DEFAULT, Sender, Type, clone, WeakRef};
    
    use gstreamer as gst;
    use gst::{Pipeline, prelude::*};
    use gtk::{AboutDialog, Align, Box as GtkBox, Button, CenterBox, CheckButton, Dialog, DialogFlags, Entry, Frame, Grid, Image, Inhibit, Label, ListBox, MenuButton, Orientation, Overlay, Popover, ResponseType, Revealer, RevealerTransitionType, ScrolledWindow, SelectionModel, Separator, SingleSelection, SpinButton, Stack, StringList, Switch, ToggleButton, Viewport, gdk_pixbuf::Pixbuf, gio::{Menu, MenuItem, MenuModel}, prelude::*, Picture, FileFilter, ProgressBar, FlowBox, Scale, SelectionMode};
    
    use adw::{ActionRow, ComboRow, HeaderBar, PreferencesGroup, PreferencesPage, PreferencesWindow, StatusPage, Window, prelude::*, Carousel, ApplicationWindow, Clamp, PreferencesRow, Leaflet, ToastOverlay};
    
    use relm4::{AppUpdate, WidgetPlus, ComponentUpdate, Components, Model, RelmApp, RelmComponent, Widgets, actions::{ActionGroupName, ActionName, RelmAction, RelmActionGroup}, factory::{DynamicIndex, FactoryPrototype, FactoryVec, FactoryVecDeque, positions::GridPosition}, new_action_group, send, MicroWidgets, MicroModel, MicroComponent};
    use relm4_macros::{widget, micro_widget};
    
    use strum::IntoEnumIterator;
    use strum_macros::{EnumIter, EnumString as EnumFromString, Display as EnumToString};
    
    use lazy_static::lazy_static;
    
    use crate::{AppModel, input::{InputEvent, InputSource, InputSourceEvent, InputSystem}, preferences::{PreferencesMsg, PreferencesModel}, video::{self, MatExt}, graph_view::{GraphView, Point as GraphPoint}};
    use crate::AppMsg;
    use crate::prelude::ObjectExt;

    use rand::Rng;
    
    use derivative::*;

    pub enum SlaveParameterTunerMsg {
        SetPropellerLowerDeadzone(usize, i8),
        SetPropellerUpperDeadzone(usize, i8),
        SetPropellerPower(usize, f64),
        SetPropellerReversed(usize, bool),
        SetP(usize, f64),
        SetI(usize, f64),
        SetD(usize, f64),
    }
    
    #[tracker::track(pub)]
    #[derive(Debug, Derivative, PartialEq)]
    #[derivative(Default)]
    pub struct PropellerDeadzone {
        key: String,
        lower: i8,
        upper: i8,
        #[derivative(Default(value="0.75"))]
        power: f64,
    }
    
    const DEFAULT_PROPELLERS: [&'static str; 6] = ["front_left", "front_right", "back_left", "back_right", "center_left", "center_right"];
    const DEFAULT_CONTROL_LOOPS: [&'static str; 2] = ["depth_lock", "direction_lock"];
    const CARD_MIN_WIDTH: i32 = 300;
    
    impl PropellerDeadzone {
        fn new(key: &str) -> PropellerDeadzone {
            PropellerDeadzone {
                key: key.to_string(),
                ..Default::default()
            }
        }

        fn key_to_string<'a, 'b : 'a>(key: &'b str) -> &'a str {
            match key {
                "front_left" => "左前",
                "front_right" => "右前",
                "back_left" => "左后",
                "back_right" => "右后",
                "center_left" => "左中",
                "center_right" => "右中",
                key => key,
            }
        }

        fn is_reversed(&self) -> bool {
            self.power < 0.0
        }

        fn set_reversed(&mut self, reversed: bool) {
            self.set_power(if reversed { - self.power.abs() } else { self.power.abs() });
        }

        fn get_actual_power(&self) -> f64 {
            self.power.abs()
        }

        fn set_actual_power(&mut self, power: f64) {
            assert!(power >= 0.0);
            self.set_power(if self.is_reversed() { -power} else { power });
        }
    }

    #[tracker::track(pub)]
    #[derive(Debug, Derivative, PartialEq)]
    #[derivative(Default)]
    pub struct PID {
        key: String,
        #[derivative(Default(value="1.0"))]
        p: f64,
        #[derivative(Default(value="1.0"))]
        i: f64,
        #[derivative(Default(value="1.0"))]
        d: f64,
    }

    impl PID {
        fn new(key: &str) -> PID {
            PID {
                key: key.to_string(),
                ..Default::default()
            }
        }

        fn key_to_string<'a, 'b : 'a>(key: &'b str) -> &'a str {
            match key {
                "depth_lock" => "深度锁定", 
                "direction_lock" => "方向锁定",
                key => key,
            }
        }
    }
    
    #[tracker::track(pub)]
    #[derive(Debug, Derivative)]
    #[derivative(Default)]
    pub struct SlaveParameterTunerModel {
        #[no_eq]
        #[derivative(Default(value="FactoryVec::new()"))]
        propeller_deadzones: FactoryVec<PropellerDeadzone>,
        #[no_eq]
        #[derivative(Default(value="FactoryVec::new()"))]
        pids: FactoryVec<PID>,
    }

    #[relm4::factory_prototype(pub)]
    impl FactoryPrototype for PropellerDeadzone {
        type Factory = FactoryVec<Self>;
        type Widgets = PropellerConfigWidgets;
        type View = FlowBox;
        type Msg = SlaveParameterTunerMsg;
    
        view! {
            group = &PreferencesGroup {
                set_title: PropellerDeadzone::key_to_string(&self.key),
                add = &GtkBox {
                    set_orientation: Orientation::Vertical,
                    set_spacing: 12,
                    append = &PreferencesGroup {
                        add = &ActionRow {
                            set_title: "反转",
                            add_suffix: reversed_switch = &Switch {
                                set_valign: Align::Center,
                                set_active: track!(self.changed(PropellerDeadzone::power()), self.is_reversed()),
                                connect_state_set(sender, key) => move |switch, state| {
                                    send!(sender, SlaveParameterTunerMsg::SetPropellerReversed(key, state));
                                    Inhibit(false)
                                }
                            },
                            set_activatable_widget: Some(&reversed_switch),
                        },
                    },
                    append = &PreferencesGroup {
                        add = &ActionRow {
                            set_title: "动力",
                            add_suffix = &SpinButton::with_range(0.01, 1.0, 0.01) {
                                set_value: track!(self.changed(PropellerDeadzone::power()), self.get_actual_power()),
                                set_digits: 2,
                                set_valign: Align::Center,
                                connect_value_changed(key, sender) => move |button| {
                                    send!(sender, SlaveParameterTunerMsg::SetPropellerPower(key, button.value()));
                                }
                            },
                        },
                        add = &ActionRow {
                            set_child = Some(&Scale::with_range(Orientation::Horizontal, 0.01, 1.0, 0.01)) {
                                set_width_request: CARD_MIN_WIDTH,
                                set_round_digits: 2,
                                set_value: track!(self.changed(PropellerDeadzone::power()), self.get_actual_power() as f64),
                                connect_value_changed(key, sender) => move |scale| {
                                    send!(sender, SlaveParameterTunerMsg::SetPropellerPower(key, scale.value()));
                                }
                            }
                        },
                    },
                    append = &PreferencesGroup {
                        add = &ActionRow {
                            set_title: "死区上限",
                            add_suffix = &SpinButton::with_range(-128.0, 127.0, 1.0) {
                                set_value: track!(self.changed(PropellerDeadzone::upper()), *self.get_upper() as f64),
                                set_digits: 0,
                                set_valign: Align::Center,
                                connect_value_changed(key, sender) => move |button| {
                                    send!(sender, SlaveParameterTunerMsg::SetPropellerUpperDeadzone(key, button.value() as i8));
                                }
                            },
                        },
                        add = &ActionRow {
                            set_child = Some(&Scale::with_range(Orientation::Horizontal, -128.0, 127.0, 1.0)) {
                                set_width_request: CARD_MIN_WIDTH,
                                set_round_digits: 0,
                                set_value: track!(self.changed(PropellerDeadzone::upper()), *self.get_upper() as f64),
                                connect_value_changed(key, sender) => move |scale| {
                                    send!(sender, SlaveParameterTunerMsg::SetPropellerUpperDeadzone(key, scale.value() as i8));
                                }
                            }
                        },
                    },
                    append = &PreferencesGroup {
                        add = &ActionRow {
                            set_title: "死区下限",
                            add_suffix = &SpinButton::with_range(-128.0, 127.0, 1.0) {
                                set_value: track!(self.changed(PropellerDeadzone::lower()), *self.get_lower() as f64),
                                set_digits: 0,
                                set_valign: Align::Center,
                                connect_value_changed(key, sender) => move |button| {
                                    send!(sender, SlaveParameterTunerMsg::SetPropellerLowerDeadzone(key, button.value() as i8));
                                }
                            },
                        },
                        add = &ActionRow {
                            set_child = Some(&Scale::with_range(Orientation::Horizontal, -128.0, 127.0, 1.0)) {
                                set_width_request: CARD_MIN_WIDTH,
                                set_round_digits: 0,
                                set_value: track!(self.changed(PropellerDeadzone::lower()), *self.get_lower() as f64),
                                connect_value_changed(key, sender) => move |scale| {
                                    send!(sender, SlaveParameterTunerMsg::SetPropellerLowerDeadzone(key, scale.value() as i8));
                                }
                            }
                        },
                    },
                }
            }
        }

        fn position(&self, index: &usize) {
            
        }
    }
    
    #[relm4::factory_prototype(pub)]
    impl FactoryPrototype for PID {
        type Factory = FactoryVec<Self>;
        type Widgets = PIDWidgets;
        type View = FlowBox;
        type Msg = SlaveParameterTunerMsg;
        
        view! {
            group = &PreferencesGroup {
                set_title: PID::key_to_string(&self.key),
                add = &GtkBox {
                    set_orientation: Orientation::Vertical,
                    set_spacing: 12,
                    append = &PreferencesGroup {
                        add = &ActionRow {
                            set_child = Some(&GraphView::new()) {
                                set_width_request: CARD_MIN_WIDTH,
                                set_height_request: CARD_MIN_WIDTH / 2,
                                set_points: (0..100).map(|_| GraphPoint { time: 0.0, value: rand::thread_rng().gen_range(-100.0..100.0) }).collect(),
                                set_upper_value: 100.0,
                                set_lower_value: -100.0,
                                // set_limit: Some(200.0),
                            },
                        },
                    },
                    append = &PreferencesGroup {
                        add = &ActionRow {
                            set_title: "P",
                            add_suffix = &SpinButton::with_range(0.0, 100.0, 0.01) {
                                set_value: track!(self.changed(PID::p()), *self.get_p()),
                                set_digits: 2,
                                set_valign: Align::Center,
                                connect_value_changed(key, sender) => move |button| {
                                    send!(sender, SlaveParameterTunerMsg::SetP(key, button.value()));
                                }
                            },
                        },
                        add = &ActionRow {
                            set_child = Some(&Scale::with_range(Orientation::Horizontal, 0.0, 100.0, 0.01)) {
                                set_width_request: CARD_MIN_WIDTH,
                                set_round_digits: 2,
                                set_value: track!(self.changed(PID::p()), *self.get_p()),
                                connect_value_changed(key, sender) => move |scale| {
                                    send!(sender, SlaveParameterTunerMsg::SetP(key, scale.value()));
                                }
                            }
                        },
                    },
                    append = &PreferencesGroup {
                        add = &ActionRow {
                            set_title: "I",
                            add_suffix = &SpinButton::with_range(0.0, 100.0, 0.01) {
                                set_value: track!(self.changed(PID::i()), *self.get_i()),
                                set_digits: 2,
                                set_valign: Align::Center,
                                connect_value_changed(key, sender) => move |button| {
                                    send!(sender, SlaveParameterTunerMsg::SetI(key, button.value()));
                                }
                            },
                        },
                        add = &ActionRow {
                            set_child = Some(&Scale::with_range(Orientation::Horizontal, 0.0, 100.0, 0.01)) {
                                set_width_request: CARD_MIN_WIDTH,
                                set_round_digits: 2,
                                set_value: track!(self.changed(PID::i()), *self.get_i()),
                                connect_value_changed(key, sender) => move |scale| {
                                    send!(sender, SlaveParameterTunerMsg::SetI(key, scale.value()));
                                }
                            }
                        },
                    },
                    append = &PreferencesGroup {
                        add = &ActionRow {
                            set_title: "D",
                            add_suffix = &SpinButton::with_range(0.0, 100.0, 0.01) {
                                set_value: track!(self.changed(PID::d()), *self.get_d()),
                                set_digits: 2,
                                set_valign: Align::Center,
                                connect_value_changed(key, sender) => move |button| {
                                    send!(sender, SlaveParameterTunerMsg::SetD(key, button.value()));
                                }
                            },
                        },
                        add = &ActionRow {
                            set_child = Some(&Scale::with_range(Orientation::Horizontal, 0.0, 100.0, 0.01)) {
                                set_width_request: CARD_MIN_WIDTH,
                                set_round_digits: 2,
                                set_value: track!(self.changed(PID::d()), *self.get_d()),
                                connect_value_changed(key, sender) => move |scale| {
                                    send!(sender, SlaveParameterTunerMsg::SetD(key, scale.value()));
                                }
                            }
                        },
                    },
                }
            }
        }
        
        fn position(&self, index: &usize) {
            
        }
    }
    
    impl SlaveParameterTunerModel {
        pub fn new() -> Self {
            SlaveParameterTunerModel {
                propeller_deadzones: FactoryVec::from_vec(DEFAULT_PROPELLERS.iter().map(|key| PropellerDeadzone::new(key)).collect()),
                pids: FactoryVec::from_vec(DEFAULT_CONTROL_LOOPS.iter().map(|key| PID::new(key)).collect()),
                ..Default::default()
            }
        }
    }
    
    #[micro_widget(pub)]
    impl MicroWidgets<SlaveParameterTunerModel> for SlaveParameterTunerWidgets {
        view! {
            window = PreferencesWindow {
                set_visible: true,
                set_destroy_with_parent: true,
                set_modal: true,
                set_search_enabled: false,
                add = &PreferencesPage {
                    set_title: "推进器",
                    set_icon_name: Some("weather-windy-symbolic"),
                    set_hexpand: true,
                    set_vexpand: true,
                    add: group_propeller = &PreferencesGroup {
                        set_title: "推进器参数",
                        add = &FlowBox {
                            set_activate_on_single_click: false,
                            set_valign: Align::Start,
                            set_row_spacing: 12,
                            set_selection_mode: SelectionMode::None,
                            factory!(model.propeller_deadzones)
                        },
                    },
                },
                add = &PreferencesPage {
                    set_title: "控制环",
                    set_icon_name: Some("media-playlist-repeat-symbolic"),
                    set_hexpand: true,
                    set_vexpand: true,
                    add: group_pid = &PreferencesGroup {
                        set_title: "PID参数",
                        add = &FlowBox {
                            set_activate_on_single_click: false,
                            set_valign: Align::Start,
                            set_row_spacing: 12,
                            set_selection_mode: SelectionMode::None,
                            factory!(model.pids)
                        },
                    },
                },
                set_title: {
                    {
                        let groups = [&group_propeller, &group_pid];
                        let clamps = groups.iter().map(|x| x.parent().and_then(|x| x.parent()).and_then(|x| x.dynamic_cast::<Clamp>().ok())).filter_map(|x| x);
                        for clamp in clamps {
                            clamp.set_maximum_size(10000);
                        }
                        let overlay: ToastOverlay = window.content().unwrap().dynamic_cast().unwrap();
                        let leaflet: Leaflet = overlay.child().unwrap().dynamic_cast().unwrap();
                        let root_box: GtkBox = leaflet.observe_children().into_iter().find_map(|x| x.dynamic_cast().ok()).unwrap();
                        let header_bar: HeaderBar = root_box.first_child().unwrap().dynamic_cast().unwrap();
                        relm4_macros::view! {
                            HeaderBar::from(header_bar) {
                                pack_start = &Button {
                                    set_css_classes: &["suggested-action"],
                                    set_halign: Align::Center,
                                    set_child = Some(&GtkBox) {
                                        set_spacing: 6,
                                        append = &Image {
                                            set_icon_name: Some("document-save-symbolic"),
                                        },
                                        append = &Label {
                                            set_label: "保存",
                                        },
                                    },
                                    connect_clicked(sender) => move |button| {
                                    },
                                },
                                pack_end = &Button {
                                    set_css_classes: &["destructive-action"],
                                    set_halign: Align::Center,
                                    set_child = Some(&GtkBox) {
                                        set_spacing: 6,
                                        append = &Image {
                                            set_icon_name: Some("view-refresh-symbolic"),
                                        },
                                        append = &Label {
                                            set_label: "重置",
                                        },
                                    },
                                    connect_clicked(sender) => move |button| {
                                    },
                                },
                            }
                        }
                    }
                    Some("参数调校")
                },
            }
        }
    }

    impl Debug for SlaveParameterTunerWidgets {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            self.root_widget().fmt(f)
        }
    }
    
    impl MicroModel for SlaveParameterTunerModel {
        type Msg = SlaveParameterTunerMsg;
        type Widgets = SlaveParameterTunerWidgets;
        type Data = ();
        
        fn update(&mut self, msg: SlaveParameterTunerMsg, data: &(), sender: Sender<SlaveParameterTunerMsg>) {
            match msg {
                SlaveParameterTunerMsg::SetPropellerLowerDeadzone(index, value) => {
                    if let Some(deadzone) = self.propeller_deadzones.get_mut(index) {
                        deadzone.set_lower(value);
                        deadzone.set_upper(max(*deadzone.get_upper(), value));
                    }
                },
                SlaveParameterTunerMsg::SetPropellerUpperDeadzone(index, value) => {
                    if let Some(deadzone) = self.propeller_deadzones.get_mut(index) {
                        deadzone.set_upper(value);
                        deadzone.set_lower(min(*deadzone.get_lower(), value));
                    }
                },
                SlaveParameterTunerMsg::SetPropellerPower(index, value) => {
                    if let Some(deadzone) = self.propeller_deadzones.get_mut(index) {
                        deadzone.set_actual_power(value);
                    }
                },
                SlaveParameterTunerMsg::SetPropellerReversed(index, reversed) => {
                    if let Some(deadzone) = self.propeller_deadzones.get_mut(index) {
                        deadzone.set_reversed(reversed);
                    }
                },
                SlaveParameterTunerMsg::SetP(index, value) => {
                    if let Some(pids) = self.pids.get_mut(index) {
                        pids.set_p(value);
                    }
                },
                SlaveParameterTunerMsg::SetI(index, value) => {
                    if let Some(pids) = self.pids.get_mut(index) {
                        pids.set_i(value);
                    }
                },
                SlaveParameterTunerMsg::SetD(index, value) => {
                    if let Some(pids) = self.pids.get_mut(index) {
                        pids.set_d(value);
                    }
                },
            }
        }
    }
}

