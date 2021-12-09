use std::cell::RefCell;
use std::ffi::c_void;
use std::marker::Send;
use std::net::Ipv4Addr;
use std::rc::Rc;
use std::str::FromStr;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use fragile::Fragile;

use gtk::gdk::Display;
use gtk::gio::{Action, Icon, Menu, MenuItem, SimpleAction};
use opencv::{highgui, prelude::*, videoio, Result, imgproc, imgcodecs, core::Size};

use gtk::{AboutDialog, Align, HeaderBar, IconLookupFlags, IconTheme, Label, MenuButton, PageSetupUnixDialogBuilder, ToggleButton, show_about_dialog};
use gtk::gdk_pixbuf::{Colorspace, Pixbuf};
use gtk::{Orientation, prelude::*};
use gtk::{Application, ApplicationWindow, Button, Box as GtkBox, Image};

use glib::{Error, Continue, MainContext, PRIORITY_DEFAULT, Sender, clone};

use gstreamer as gst;
use gstreamer_app as gst_app;
use gstreamer_rtsp as gst_rtsp;
use gst::{Element, Event, Pad, PadProbeType, Pipeline, element_error, prelude::*};

const VIDEO_WIDTH: i32 = 1920;
const VIDEO_HEIGHT: i32 = 1080;

pub fn create_queue_to_file(filename: &str) -> Result<[gst::Element; 3], &'static str> {
    let queue_to_file = gst::ElementFactory::make("queue", None).map_err(|_| "Missing element: queue")?;
    let filesink = gst::ElementFactory::make("filesink", None).map_err(|_| "Missing element: filesink")?;
    filesink.set_property("location", filename).unwrap();
    let matroskamux = gst::ElementFactory::make("matroskamux", None).map_err(|_| "Missing element: matroskamux")?;
    Ok([queue_to_file, matroskamux, filesink])
}

pub fn connect_elements_to_pipeline(pipeline: &Pipeline, elements: &[Element]) -> Result<Pad, &'static str> {
    let output_tee = pipeline.by_name("output_tee").ok_or("Cannot find output tee")?;
    if let Some(element) = elements.first() {
        pipeline.add(element).map_err(|_| "Cannot add an element")?; // 必须先添加，再连接
    }
    for elements in elements.windows(2) {
        match elements {
            [a, b] => {
                pipeline.add(b).map_err(|_| "Cannot add an element")?;
                a.link(b).map_err(|_| "Cannot link elements")?;
            },
            _ => ()
        }
    }
    let teepad = output_tee.request_pad_simple("src_%u").ok_or("Cannot request pad")?;
    let sinkpad = elements.first().unwrap().static_pad("sink").unwrap();
    teepad.link(&sinkpad).map_err(|_| "Cannot link output_tee to matroskamux")?;
    Ok(teepad)
}

pub fn disconnect_elements_to_pipeline(pipeline: &Pipeline, teepad: &Pad, elements: &[Element]) -> Result<(), &'static str> {
    let output_tee = pipeline.by_name("output_tee").ok_or("Cannot find output tee")?;
    let sinkpad = elements.first().unwrap().static_pad("sink").unwrap();
    let res = teepad.unlink(&sinkpad).map_err(|_| "Cannot unlink elements");
    pipeline.remove_many(&elements.iter().collect::<Vec<_>>()).map_err(|_| "Cannot remove elements")?;
    res
}

pub fn create_pipeline(port: u16) -> Result<gst::Pipeline, &'static str> {
    let pipeline = gst::Pipeline::new(None);
    let udpsrc = gst::ElementFactory::make("udpsrc", None).map_err(|_| "Missing element: udpsrc")?;
    let appsink = gst::ElementFactory::make("appsink", Some("display")).map_err(|_| "Missing element: appsink")?;
    udpsrc.set_property("port", port as i32).map_err(|_| "Cannot set udpsrc port")?;
    let caps = gst::caps::Caps::from_str("application/x-rtp, media=(string)video, encoding-name=(string)H264").map_err(|_| "Cannot create Caps")?;
    udpsrc.set_property("caps", caps).map_err(|_| "Cannot set udpsrc caps")?;
    let rtph264depay = gst::ElementFactory::make("rtph264depay", None).map_err(|_| "Missing element: rtph264depay")?;
    let h264parse = gst::ElementFactory::make("h264parse", None).map_err(|_| "Missing element: h264parse")?;
    let output_tee = gst::ElementFactory::make("tee", Some("output_tee")).map_err(|_| "Missing element: tee")?;
    let queue_to_app = gst::ElementFactory::make("queue", None).map_err(|_| "Missing element: queue")?;
    let avdec_h264 = gst::ElementFactory::make("avdec_h264", None).map_err(|_| "Missing element: avdec_h264")?;
    let videoconvert = gst::ElementFactory::make("videoconvert", None).map_err(|_| "Missing element: videoconvert")?;
    
    pipeline.add_many(&[&udpsrc, &appsink, &rtph264depay, &h264parse, &output_tee, &avdec_h264, &videoconvert, &queue_to_app]).map_err(|_| "Cannot create pipeline")?;
    
    udpsrc.link(&rtph264depay).map_err(|_| "Cannot link udpsrc to rtph264depay")?;
    rtph264depay.link(&h264parse).map_err(|_| "Cannot link rtph264depay to h264parse")?;
    h264parse.link(&output_tee).map_err(|_| "Cannot link h264parse to tee")?;
    
    queue_to_app.link(&avdec_h264).map_err(|_| "Cannot link queue to avdec_h264")?;
    avdec_h264.link(&videoconvert).map_err(|_| "Cannot link avdec_h264 to videoconvert")?;
    videoconvert.link(&appsink).map_err(|_| "Cannot link videoconvert to appsink")?;
    
    output_tee.request_pad_simple("src_%u").unwrap().link(&queue_to_app.static_pad("sink").unwrap()).map_err(|_| "Cannot link output_tee to queue")?;
    // let elements = create_queue_to_file(format!("/tmp/{}.mkv", port).as_str()).unwrap();
    // let pad = connect_elements_to_pipeline(&pipeline, &elements).unwrap();

    // let pp = pipeline.clone();
    // thread::spawn(move || {
    //     thread::sleep(Duration::from_secs(5));
    //     glib::MainContext::default().invoke(move || {
    //         disconnect_elements_to_pipeline(&pp, &pad, &elements).unwrap();
    //     })
    // });
    Ok(pipeline)
}

pub fn attach_pipeline_callback(pipeline: &Pipeline, sender: Sender<Mat>) -> Result<(), &'static str> {
    let appsink = pipeline.by_name("display").unwrap().dynamic_cast::<gst_app::AppSink>()
        .expect("Sink element is expected to be an appsink!");
    appsink.set_callbacks(gst_app::AppSinkCallbacks::builder()
                .new_sample(move |appsink| {
                    let sample = appsink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                    let buffer = sample.buffer().ok_or_else(|| {
                        element_error!(
                            appsink,
                            gst::ResourceError::Failed,
                            ("Failed to get buffer from appsink")
                        );
                        gst::FlowError::Error
                    })?;
                    let map = buffer.map_readable().map_err(|_| {
                        element_error!(
                            appsink,
                            gst::ResourceError::Failed,
                            ("Failed to map buffer readable")
                        );
                        gst::FlowError::Error
                    })?;
                    let _mat = unsafe {
                        Mat::new_rows_cols_with_data(VIDEO_HEIGHT + VIDEO_HEIGHT / 2, VIDEO_WIDTH, opencv::core::CV_8UC1, map.as_ptr() as *mut c_void, opencv::core::Mat_AUTO_STEP)
                    }.map_err(|_| gst::FlowError::CustomError)?.clone();
                    let mut mat = Mat::default();
                    imgproc::cvt_color(&_mat, &mut mat, imgproc::COLOR_YUV2RGBA_I420, 3).expect("Cannot convert frame color!");
                    sender.send(mat).expect("Cannot send Mat frame!");
                    // glib::MainContext::default().invoke(move || {
                    //     let bytes = glib::Bytes::from(mat.data_bytes().unwrap());
                    //     let pixbuf = Pixbuf::from_bytes(&bytes, Colorspace::Rgb, false, 8, VIDEO_WIDTH, VIDEO_HEIGHT, 1);
                    //     image.get().set_from_pixbuf(Some(&pixbuf));
                    // });
                    println!("Appsink callback on: {:?}", thread::current().id());
                    Ok(gst::FlowSuccess::Ok)
                })
                .build());
    // let bus = pipeline
    //     .bus()
    //     .expect("Pipeline without bus. Shouldn't happen!");
    Ok(())
}

pub trait MatExt {
    fn as_pixbuf(&self) -> Pixbuf;
}

impl MatExt for Mat {
    fn as_pixbuf(&self) -> Pixbuf {
        let bytes = glib::Bytes::from(self.data_bytes().unwrap());
        Pixbuf::from_bytes(&bytes, Colorspace::Rgb, false, 8, VIDEO_WIDTH, VIDEO_HEIGHT, 1)
    }
}
