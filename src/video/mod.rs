use std::cell::RefCell;
use std::ffi::c_void;
use std::rc::Rc;
use std::str::FromStr;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use fragile::Fragile;

use gtk4::gdk::Display;
use gtk4::gio::{Action, Icon, Menu, MenuItem, SimpleAction};
use opencv::{highgui, prelude::*, videoio, Result, imgproc, imgcodecs, core::Size};

use gtk4::{AboutDialog, Align, HeaderBar, IconLookupFlags, IconTheme, Label, MenuButton, PageSetupUnixDialogBuilder, ToggleButton, show_about_dialog};
use gtk4::gdk_pixbuf::{Colorspace, Pixbuf};
use gtk4::{Orientation, prelude::*};
use gtk4::{Application, ApplicationWindow, Button, Box as GtkBox, Image};

use glib::{Error, Continue, MainContext, PRIORITY_DEFAULT, Sender, clone};

use gstreamer as gst;
use gstreamer_app as gst_app;
use gstreamer_rtsp as gst_rtsp;
use gst::{Event, element_error, prelude::*};

const VIDEO_WIDTH: i32 = 640;
const VIDEO_HEIGHT: i32 = 480;

fn create_pipeline(port: i32) -> Result<gst::Pipeline, &'static str> {
    let pipeline = gst::Pipeline::new(None);
    let udpsrc = gst::ElementFactory::make("udpsrc", None).map_err(|_| "Missing element: udpsrc")?;
    let appsink = gst::ElementFactory::make("appsink", Some("display")).map_err(|_| "Missing element: appsink")?;
    udpsrc.set_property("port", port).map_err(|_| "Cannot set udpsrc port")?;
    let caps = gst::caps::Caps::from_str("application/x-rtp, media=(string)video, encoding-name=(string)H264").map_err(|_| "Cannot create Caps")?;
    udpsrc.set_property("caps", caps).map_err(|_| "Cannot set udpsrc caps")?;
    let rtph264depay = gst::ElementFactory::make("rtph264depay", None).map_err(|_| "Missing element: rtph264depay")?;
    let h264parse = gst::ElementFactory::make("h264parse", None).map_err(|_| "Missing element: h264parse")?;
    let output_tee = gst::ElementFactory::make("tee", None).map_err(|_| "Missing element: tee")?;
    let queue_to_file = gst::ElementFactory::make("queue", None).map_err(|_| "Missing element: queue")?;
    let queue_to_app = gst::ElementFactory::make("queue", None).map_err(|_| "Missing element: queue")?;
    let avdec_h264 = gst::ElementFactory::make("avdec_h264", None).map_err(|_| "Missing element: avdec_h264")?;
    let videoconvert = gst::ElementFactory::make("videoconvert", None).map_err(|_| "Missing element: videoconvert")?;
    let matroskamux = gst::ElementFactory::make("matroskamux", None).map_err(|_| "Missing element: matroskamux")?;
    let filesink = gst::ElementFactory::make("filesink", None).map_err(|_| "Missing element: filesink")?;
    filesink.set_property("location", format!("/tmp/rec_{}.mkv", port));
    pipeline.add_many(&[&udpsrc, &appsink, &rtph264depay, &h264parse, &output_tee, &avdec_h264, &videoconvert, &matroskamux, &filesink, &queue_to_app, &queue_to_file]).map_err(|_| "Cannot create pipeline")?;
    udpsrc.link(&rtph264depay).map_err(|_| "Cannot link udpsrc to rtph264depay")?;
    rtph264depay.link(&h264parse).map_err(|_| "Cannot link rtph264depay to h264parse")?;
    h264parse.link(&output_tee).map_err(|_| "Cannot link h264parse to tee")?;
    
    queue_to_app.link(&avdec_h264).map_err(|_| "Cannot link queue to avdec_h264")?;
    avdec_h264.link(&videoconvert).map_err(|_| "Cannot link avdec_h264 to videoconvert")?;
    videoconvert.link(&appsink).map_err(|_| "Cannot link videoconvert to appsink")?;
    
    queue_to_file.link(&matroskamux).map_err(|_| "Cannot link queue to matroskamux")?;
    matroskamux.link(&filesink).map_err(|_| "Cannot link matroskamux to filesink")?;

    output_tee.request_pad_simple("src_%u").unwrap().link(&queue_to_app.static_pad("sink").unwrap()).map_err(|_| "Cannot link output_tee to queue")?;
    output_tee.request_pad_simple("src_%u").unwrap().link(&queue_to_file.static_pad("sink").unwrap()).map_err(|_| "Cannot link output_tee to matroskamux")?;

    thread::spawn(move || {
        thread::sleep(Duration::from_secs(10));
        filesink.send_event(gst::event::Eos::new()); 
    });
    
    Ok(pipeline)
}

fn attach_pipeline_to_image(port: i32, image: Arc<Fragile<Image>>) -> Result<gst::StateChangeSuccess, gst::StateChangeError> {
    let pipeline = create_pipeline(port).unwrap();
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
                    imgproc::cvt_color(&_mat, &mut mat, imgproc::COLOR_YUV2RGBA_I420, 3);
                    let image = image.clone();
                    glib::MainContext::default().invoke(move || {
                        let bytes = glib::Bytes::from(mat.data_bytes().unwrap());
                        let pixbuf = Pixbuf::from_bytes(&bytes, Colorspace::Rgb, false, 8, VIDEO_WIDTH, VIDEO_HEIGHT, 1);
                        image.get().set_from_pixbuf(Some(&pixbuf));
                    });
                    println!("Appsink callback on: {:?}", thread::current().id());
                    Ok(gst::FlowSuccess::Ok)
                })
                .build());
    pipeline.set_state(gst::State::Playing)
    // let bus = pipeline
    //     .bus()
    //     .expect("Pipeline without bus. Shouldn't happen!");
}
