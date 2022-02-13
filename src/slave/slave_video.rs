use std::{cell::RefCell, path::PathBuf, rc::Rc, sync::{Arc, Mutex}, fmt::Debug};

use glib::{MainContext, Sender};
use gstreamer as gst;
use gst::{Pipeline, prelude::*};
use gtk::{Box as GtkBox, Stack, gdk_pixbuf::Pixbuf, prelude::*, Picture};
use adw::StatusPage;
use relm4::{send, MicroWidgets, MicroModel};
use relm4_macros::micro_widget;

use derivative::*;

use crate::{preferences::PreferencesModel, slave::video::{MatExt, ImageFormat}};
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
    pub record_handle: Option<(gst::Pad, Vec<gst::Element>)>,
    pub preferences: Rc<RefCell<PreferencesModel>>, 
}

pub enum SlaveVideoMsg {
    StartPipeline,
    StopPipeline,
    SetPixbuf(Option<Pixbuf>),
    StartRecord(PathBuf),
    StopRecord,
    ConfigUpdated(SlaveConfigModel),
    SaveScreenshot(PathBuf),
}

impl MicroModel for SlaveVideoModel {
    type Msg = SlaveVideoMsg;
    type Widgets = SlaveVideoWidgets;
    type Data = Sender<SlaveMsg>;

    fn update(&mut self, msg: SlaveVideoMsg, parent_sender: &Sender<SlaveMsg>, sender: Sender<SlaveVideoMsg>) {
        match msg {
            SlaveVideoMsg::SetPixbuf(pixbuf) => {
                if self.get_pixbuf().is_none() {
                    send!(parent_sender, SlaveMsg::PollingChanged(true)); // 主要是更新截图按钮的状态
                }
                self.set_pixbuf(pixbuf)
            },
            SlaveVideoMsg::StartRecord(pathbuf) => {
                if let Some(pipeline) = &self.pipeline {
                    let elements = self.config.lock().unwrap().video_decoder.gst_record_elements(&pathbuf.to_str().unwrap()).unwrap();
                    let pad = super::video::connect_elements_to_pipeline(pipeline, &elements).unwrap();
                    pipeline.set_state(gst::State::Playing).unwrap(); // 添加元素后会自动暂停，需要手动重新开始播放
                    dbg!(pipeline.current_state());
                    self.record_handle = Some((pad, Vec::from(elements)));
                    send!(parent_sender, SlaveMsg::RecordingChanged(true));
                }
            },
            SlaveVideoMsg::StopRecord => {
                if let Some(pipeline) = &self.pipeline {
                    if let Some((teepad, elements)) = &self.record_handle {
                        super::video::disconnect_elements_to_pipeline(pipeline, teepad, elements).unwrap();
                        send!(parent_sender, SlaveMsg::RecordingChanged(false));
                    }
                    self.set_record_handle(None);
                }
            },
            SlaveVideoMsg::ConfigUpdated(config) => {
                *self.get_mut_config().lock().unwrap() = config;
            },
            SlaveVideoMsg::StartPipeline => {
                assert!(self.pipeline == None);
                let video_port = self.get_config().lock().unwrap().get_video_port().clone();
                let video_decoder = self.get_config().lock().unwrap().get_video_decoder().clone();
                match super::video::create_pipeline(video_port, video_decoder) {
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
                        send!(parent_sender, SlaveMsg::VideoPipelineError(String::from(msg)));
                    },
                }
            },
            SlaveVideoMsg::StopPipeline => {
                assert!(self.pipeline != None);
                if self.record_handle.is_some() {
                    self.update(SlaveVideoMsg::StopRecord, parent_sender, sender.clone());
                }
                if let Some(pipeline) = &self.pipeline {
                    pipeline.set_state(gst::State::Null).unwrap();
                    self.pipeline = None;
                }
                send!(parent_sender, SlaveMsg::PollingChanged(false));
            },
            SlaveVideoMsg::SaveScreenshot(pathbuf) => {
                assert!(self.pixbuf != None);
                if let Some(pixbuf) = &self.pixbuf {
                    let format = pathbuf.extension().unwrap().to_str().and_then(ImageFormat::from_extension).unwrap();
                    pixbuf.savev(&pathbuf, &format.to_string().to_lowercase(), &[]).unwrap();
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
