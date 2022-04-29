/* slave_video.rs
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

use std::{cell::RefCell, path::PathBuf, rc::Rc, sync::{Arc, Mutex}, fmt::Debug};

use glib::{MainContext, Sender, clone};
use gst::{Pipeline, prelude::*};
use gtk::{Box as GtkBox, Stack, prelude::*, Picture};
use gdk_pixbuf::Pixbuf;
use adw::StatusPage;
use relm4::{send, MicroWidgets, MicroModel};
use relm4_macros::micro_widget;

use derivative::*;

use crate::{preferences::PreferencesModel, slave::video::{MatExt, ImageFormat, VideoSource}, async_glib::{Promise, Future}};
use super::{slave_config::SlaveConfigModel, SlaveMsg};

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
    pub record_handle: Option<((gst::Element, gst::Pad), Vec<gst::Element>)>,
    #[derivative(Default(value="Rc::new(RefCell::new(PreferencesModel::load_or_default()))"))]
    pub preferences: Rc<RefCell<PreferencesModel>>, 
}

impl SlaveVideoModel {
    pub fn new(preferences: Rc<RefCell<PreferencesModel>>, config: Arc<Mutex<SlaveConfigModel>>) -> Self {
        SlaveVideoModel {
            preferences, config,
            ..Default::default()
        }
    }
    pub fn is_running(&self) -> bool {
        self.pipeline.is_some()
    }

    pub fn is_recording(&self) -> bool {
        self.record_handle.is_some()
    }
}

pub enum SlaveVideoMsg {
    StartPipeline,
    StopPipeline,
    SetPixbuf(Option<Pixbuf>),
    StartRecord(PathBuf),
    StopRecord(Option<Promise<()>>),
    ConfigUpdated(SlaveConfigModel),
    SaveScreenshot(PathBuf),
    RequestFrame,
}

impl MicroModel for SlaveVideoModel {
    type Msg = SlaveVideoMsg;
    type Widgets = SlaveVideoWidgets;
    type Data = Sender<SlaveMsg>;

    fn update(&mut self, msg: SlaveVideoMsg, parent_sender: &Sender<SlaveMsg>, sender: Sender<SlaveVideoMsg>) {
        self.reset();
        match msg {
            SlaveVideoMsg::SetPixbuf(pixbuf) => {
                if self.get_pixbuf().is_none() {
                    send!(parent_sender, SlaveMsg::PollingChanged(true)); // 主要是更新截图按钮的状态
                }
                self.set_pixbuf(pixbuf)
            },
            SlaveVideoMsg::StartRecord(pathbuf) => {
                if let Some(pipeline) = &self.pipeline {
                    let preferences = self.preferences.borrow();
                    let encoder = if *preferences.get_default_reencode_recording_video() { Some(preferences.get_default_video_encoder()) } else { None };
                    let colorspace_conversion = self.config.lock().unwrap().get_colorspace_conversion().clone();
                    let record_handle = match encoder {
                        Some(encoder) => {
                            let elements = encoder.gst_record_elements(colorspace_conversion, &pathbuf.to_str().unwrap());
                            let elements_and_pad = elements.and_then(|elements| super::video::connect_elements_to_pipeline(pipeline, "tee_decoded", &elements).map(|pad| (elements, pad)));
                            elements_and_pad
                        },
                        None => {
                            let elements = self.config.lock().unwrap().video_decoder.gst_record_elements(&pathbuf.to_str().unwrap());
                            let elements_and_pad = elements.and_then(|elements| super::video::connect_elements_to_pipeline(pipeline, "tee_source", &elements).map(|pad| (elements, pad)));
                            elements_and_pad
                        },
                    };
                    match record_handle {
                        Ok((elements, pad)) => {
                            self.record_handle = Some((pad, Vec::from(elements)));
                            send!(parent_sender, SlaveMsg::RecordingChanged(true));
                        },
                        Err(err) => {
                            send!(parent_sender, SlaveMsg::ErrorMessage(err.to_string()));
                            send!(parent_sender, SlaveMsg::RecordingChanged(false));
                        },
                    }
                }
            },
            SlaveVideoMsg::StopRecord(promise) => {
                if let Some(pipeline) = &self.pipeline {
                    if let Some((teepad, elements)) = &self.record_handle {
                        super::video::disconnect_elements_to_pipeline(pipeline, teepad, elements).unwrap().for_each(clone!(@strong parent_sender => move |_| {
                            send!(parent_sender, SlaveMsg::RecordingChanged(false));
                            if let Some(promise) = promise {
                                promise.success(());
                            }
                        }));
                        
                    }
                    self.set_record_handle(None);
                }
            },
            SlaveVideoMsg::ConfigUpdated(config) => {
                *self.get_mut_config().lock().unwrap() = config;
            },
            SlaveVideoMsg::StartPipeline => {
                assert!(self.pipeline == None);
                let config = self.get_config().lock().unwrap();
                let preferences = self.get_preferences().borrow();
                let video_url = config.get_video_url();
                if let Some(video_source) = VideoSource::from_url(video_url) {
                    let video_decoder = config.get_video_decoder().clone();
                    let colorspace_conversion = config.get_colorspace_conversion().clone();
                    let use_decodebin = config.get_use_decodebin().clone();
                    let appsink_leaky_enabled = preferences.get_appsink_queue_leaky_enabled().clone();
                    
                    drop(preferences); 
                    drop(config); // 结束 &self 的生命周期
                    
                    match if use_decodebin { super::video::create_decodebin_pipeline(video_source, appsink_leaky_enabled) } else { super::video::create_pipeline(
                        video_source,
                        colorspace_conversion,
                        video_decoder,
                        appsink_leaky_enabled) } {
                        Ok(pipeline) => {
                            let sender = sender.clone();
                            let (mat_sender, mat_receiver) = MainContext::channel(glib::PRIORITY_DEFAULT);
                            super::video::attach_pipeline_callback(&pipeline, mat_sender, self.get_config().clone()).unwrap();
                            mat_receiver.attach(None, move |mat| {
                                sender.send(SlaveVideoMsg::SetPixbuf(Some(mat.as_pixbuf()))).unwrap();
                                Continue(true)
                            });
                            pipeline.set_state(gst::State::Playing).unwrap();
                            self.set_pipeline(Some(pipeline));
                            send!(parent_sender, SlaveMsg::PollingChanged(true));
                        },
                        Err(msg) => {
                            send!(parent_sender, SlaveMsg::ErrorMessage(String::from(msg)));
                            send!(parent_sender, SlaveMsg::PollingChanged(false));
                        },
                    }
                } else {
                    send!(parent_sender, SlaveMsg::ErrorMessage(String::from("拉流 URL 有误，请检查并修改后重试。")));
                    send!(parent_sender, SlaveMsg::PollingChanged(false));
                }
            },
            SlaveVideoMsg::StopPipeline => {
                assert!(self.pipeline != None);
                let mut futures = Vec::<Future<()>>::new();
                let recording = self.is_recording();
                if recording {
                    let promise = Promise::new();
                    let future = promise.future();
                    self.update(SlaveVideoMsg::StopRecord(Some(promise)), parent_sender, sender.clone());
                    futures.push(future);
                }
                let promise = Promise::new();
                futures.push(promise.future());
                let promise = Mutex::new(Some(promise));
                if let Some(pipeline) = self.pipeline.take() {
                    let sinkpad = pipeline.by_name("display").unwrap().static_pad("sink").unwrap();
                    sinkpad.add_probe(gst::PadProbeType::EVENT_BOTH, move |_pad, info| {
                        match &info.data {
                            Some(gst::PadProbeData::Event(event)) => {
                                if let gst::EventView::Eos(_) = event.view() {
                                    promise.lock().unwrap().take().unwrap().success(());
                                    gst::PadProbeReturn::Remove
                                } else {
                                    gst::PadProbeReturn::Pass
                                }
                            },
                            _ => gst::PadProbeReturn::Pass,
                        }
                    });
                    if pipeline.current_state() == gst::State::Playing && pipeline.send_event(gst::event::Eos::new()) {
                        Future::sequence(futures.into_iter()).for_each(clone!(@strong parent_sender, @weak pipeline => move |_| {
                            send!(parent_sender, SlaveMsg::PollingChanged(false));
                            pipeline.set_state(gst::State::Null).unwrap();
                        }));
                        glib::timeout_add_local_once(self.preferences.borrow().get_pipeline_timeout().clone(), clone!(@weak pipeline, @strong parent_sender => move || {
                            send!(parent_sender, SlaveMsg::PollingChanged(false));
                            if recording {
                                send!(parent_sender, SlaveMsg::RecordingChanged(false));
                            }
                            send!(parent_sender, SlaveMsg::ShowToastMessage(String::from("等待管道响应超，已将其强制终止。")));
                            pipeline.set_state(gst::State::Null).unwrap();
                        }));
                    } else {
                        send!(parent_sender, SlaveMsg::PollingChanged(false));
                        send!(parent_sender, SlaveMsg::RecordingChanged(false));
                        pipeline.set_state(gst::State::Null).unwrap();
                    }
                }
            },
            SlaveVideoMsg::SaveScreenshot(pathbuf) => {
                assert!(self.pixbuf != None);
                if let Some(pixbuf) = &self.pixbuf {
                    let format = pathbuf.extension().unwrap().to_str().and_then(ImageFormat::from_extension).unwrap();
                    match pixbuf.savev(&pathbuf, &format.to_string().to_lowercase(), &[]) {
                        Ok(_) => send!(parent_sender, SlaveMsg::ShowToastMessage(format!("截图保存成功：{}", pathbuf.to_str().unwrap()))),
                        Err(err) => send!(parent_sender, SlaveMsg::ShowToastMessage(format!("截图保存失败：{}", err.to_string()))),
                    }
                }
            },
            SlaveVideoMsg::RequestFrame => {
                if let Some(pipeline) = &self.pipeline {
                    pipeline.by_name("display").unwrap().dynamic_cast::<gst_app::AppSink>() .unwrap().send_event(gst::event::CustomDownstream::new(gst::Structure::new("resend", &[])));
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
                    set_icon_name: Some("face-uncertain-symbolic"),
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
