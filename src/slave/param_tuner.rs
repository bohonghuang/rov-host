use std::{sync::Arc, fmt::Debug, cmp::{max, min}};
use async_std::net::TcpStream;

use glib::Sender;
use gstreamer as gst;
use gst::prelude::*;
use gtk::{Align, Box as GtkBox, Button, Image, Inhibit, Label, Orientation, SpinButton, Switch, prelude::*, FlowBox, Scale, SelectionMode};
use adw::{HeaderBar, PreferencesGroup, PreferencesPage, PreferencesWindow, prelude::*, Clamp, Leaflet, ToastOverlay, ExpanderRow, ActionRow};
use relm4::{Widgets, factory::{FactoryPrototype, FactoryVec}, send, MicroWidgets, MicroModel};
use relm4_macros::micro_widget;

use rand::Rng;
use derivative::*;

use crate::graph_view::{GraphView, Point as GraphPoint};

pub enum SlaveParameterTunerMsg {
    SetPropellerLowerDeadzone(usize, i8),
    SetPropellerUpperDeadzone(usize, i8),
    SetPropellerPower(usize, f64),
    SetPropellerReversed(usize, bool),
    SetPropellerEnabled(usize, bool),
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
    #[derivative(Default(value="true"))]
    enabled: bool,
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
                    add = &ExpanderRow {
                        set_title: "启用",
                        set_show_enable_switch: true,
                        set_expanded: *self.get_enabled(),
                        set_enable_expansion: track!(self.changed(PropellerDeadzone::enabled()), *self.get_enabled()),
                        connect_enable_expansion_notify(sender, key) => move |expander| {
                            send!(sender, SlaveParameterTunerMsg::SetPropellerEnabled(key, expander.enables_expansion()));
                        },
                        add_row = &ActionRow {
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
                        add_row = &ActionRow {
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
                        add_row = &ActionRow {
                            set_child = Some(&Scale::with_range(Orientation::Horizontal, 0.01, 1.0, 0.01)) {
                                set_width_request: CARD_MIN_WIDTH,
                                set_round_digits: 2,
                                set_value: track!(self.changed(PropellerDeadzone::power()), self.get_actual_power() as f64),
                                connect_value_changed(key, sender) => move |scale| {
                                    send!(sender, SlaveParameterTunerMsg::SetPropellerPower(key, scale.value()));
                                }
                            }
                        },
                        add_row = &ActionRow {
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
                        add_row = &ActionRow {
                            set_child = Some(&Scale::with_range(Orientation::Horizontal, -128.0, 127.0, 1.0)) {
                                set_width_request: CARD_MIN_WIDTH,
                                set_round_digits: 0,
                                set_value: track!(self.changed(PropellerDeadzone::upper()), *self.get_upper() as f64),
                                connect_value_changed(key, sender) => move |scale| {
                                    send!(sender, SlaveParameterTunerMsg::SetPropellerUpperDeadzone(key, scale.value() as i8));
                                }
                            }
                        },
                        add_row = &ActionRow {
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
                        add_row = &ActionRow {
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
    pub fn new(tcp_stream: Arc<TcpStream>) -> Self {
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
                {           // Relm4存在Bug，以下代码应防止在 `post_view` 函数里
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
            SlaveParameterTunerMsg::SetPropellerEnabled(index, enabled) => {
                if let Some(deadzone) = self.propeller_deadzones.get_mut(index) {
                    deadzone.set_enabled(enabled);
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
