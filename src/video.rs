use std::borrow::BorrowMut;
use std::cell::RefCell;
use std::ffi::c_void;
use std::marker::Send;
use std::net::Ipv4Addr;
use std::ops::Div;
use std::rc::Rc;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use cv::core::{MatExpr, Scalar, VecN, MatExprResult};
use cv::types::VectorOfMat;
use fragile::Fragile;

use gtk::gdk::Display;
use gtk::gio::{Action, Icon, Menu, MenuItem, SimpleAction};
use opencv as cv;
use cv::{highgui, prelude::*, videoio, Result, imgproc, imgcodecs, core::Size};

use gtk::{AboutDialog, Align, HeaderBar, IconLookupFlags, IconTheme, Label, MenuButton, ToggleButton, show_about_dialog};
use gtk::gdk_pixbuf::{Colorspace, Pixbuf};
use gtk::{Orientation, prelude::*};
use gtk::{Application, ApplicationWindow, Button, Box as GtkBox, Image};

use glib::{Error, Continue, MainContext, PRIORITY_DEFAULT, Sender, clone};

use gstreamer as gst;
use gstreamer_app as gst_app;
use gstreamer_rtsp as gst_rtsp;
use gst::{Element, Event, Pad, PadProbeType, Pipeline, element_error, prelude::*, PadProbeReturn, PadProbeData, EventType, Caps, EventView};

use crate::slave::{SlaveConfigModel, VideoAlgorithm};

use strum::IntoEnumIterator;
use strum_macros::{EnumIter, EnumString as EnumFromString, Display as EnumToString};

#[derive(EnumIter, EnumFromString, PartialEq, Clone, Debug)]
pub enum VideoEncoder {
    Copy, H264, H265, WebM
}

impl ToString for VideoEncoder {
    fn to_string(&self) -> String {
        match self {
            VideoEncoder::Copy => "不编码",
            VideoEncoder::H264 => "H.264",
            VideoEncoder::H265 => "H.265",
            VideoEncoder::WebM => "WebM",
        }.to_string()
    }
}

#[derive(EnumIter, EnumFromString, PartialEq, Clone, Debug)]
pub enum VideoDecoder {
    H264Software, H264HardwareNvidia, H264HardwareNvidiaStateless, H265Software, H265HardwareNvidia
}

impl ToString for VideoDecoder {
    fn to_string(&self) -> String {
        match self {
            VideoDecoder::H264Software => "H.264 (CPU)",
            VideoDecoder::H264HardwareNvidia => "H.264 (NVIDIA)",
            VideoDecoder::H264HardwareNvidiaStateless => "H.264 (NVIDIA 无状态)",
            VideoDecoder::H265Software => "H.265 (CPU)",
            VideoDecoder::H265HardwareNvidia => "H.265 (NVIDIA)",
        }.to_string()
    }
}

impl VideoDecoder {
    pub fn gst_elements(&self) -> Result<(Vec<Element>, Vec<Element>), &'static str> {
        match self {
            decoder_h264 @ (VideoDecoder::H264Software | VideoDecoder::H264HardwareNvidia | VideoDecoder::H264HardwareNvidiaStateless) => {
                let rtph264depay = gst::ElementFactory::make("rtph264depay", None).map_err(|_| "Missing element: rtph264depay")?;
                let h264parse = gst::ElementFactory::make("h264parse", None).map_err(|_| "Missing element: h264parse")?;
                let decoder_name = match decoder_h264 {
                    VideoDecoder::H264Software => "avdec_h264",
                    VideoDecoder::H264HardwareNvidia => "nvh264dec",
                    VideoDecoder::H264HardwareNvidiaStateless => "nvh264sldec",
                    _ => todo!(),
                };
                dbg!(decoder_name);
                let decoder = gst::ElementFactory::make(decoder_name, Some("video_decoder")).map_err(|_| "The configured video decoder is unavailable currently")?;
                Ok((if decoder_h264 == &VideoDecoder::H264HardwareNvidia { vec![rtph264depay, h264parse] } else { vec![rtph264depay] }, vec![decoder]))
            },
            decoder_h265 @ (VideoDecoder::H265Software | VideoDecoder::H265HardwareNvidia) => {
                let rtph265depay = gst::ElementFactory::make("rtph265depay", None).map_err(|_| "Missing element: rtph265depay")?;
                let h265parse = gst::ElementFactory::make("h265parse", None).map_err(|_| "Missing element: h265parse")?;
                let decoder_name = match decoder_h265 {
                    VideoDecoder::H265Software => "avdec_h265",
                    VideoDecoder::H265HardwareNvidia => "nvh265dec",
                    _ => todo!(),
                };
                dbg!(decoder_name);
                let decoder = gst::ElementFactory::make(decoder_name, Some("video_decoder")).map_err(|_| "The configured video decoder is unavailable currently")?;
                Ok((vec![rtph265depay, h265parse], vec![decoder]))
            },
        }
    }
}

impl Default for VideoEncoder {
    fn default() -> Self { Self::Copy }
}

impl Default for VideoDecoder {
    fn default() -> Self { Self::H264Software }
}

pub fn create_queue_to_file(filename: &str) -> Result<[gst::Element; 3], &'static str> {
    let queue_to_file = gst::ElementFactory::make("queue", None).map_err(|_| "Missing element: queue")?;
    let filesink = gst::ElementFactory::make("filesink", None).map_err(|_| "Missing element: filesink")?;
    filesink.set_property("location", filename);
    let matroskamux = gst::ElementFactory::make("matroskamux", None).map_err(|_| "Missing element: matroskamux")?;
    Ok([queue_to_file, matroskamux, filesink])
}

pub fn connect_elements_to_pipeline(pipeline: &Pipeline, elements: &[Element]) -> Result<Pad, &'static str> {
    let output_tee = pipeline.by_name("output_tee").ok_or("Cannot find output tee")?;
    if let Some(element) = elements.first() {
        pipeline.add(element).map_err(|_| "Cannot add an element")?; // 必须先添加，再连接
    }
    for elements in elements.windows(2) {
        if let [a, b] = elements {
            pipeline.add(b).map_err(|_| "Cannot add an element")?;
            a.link(b).map_err(|_| "Cannot link elements")?;
            
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

pub fn create_pipeline(port: u16, decoder: VideoDecoder) -> Result<gst::Pipeline, &'static str> {
    let pipeline = gst::Pipeline::new(None);
    let udpsrc = gst::ElementFactory::make("udpsrc", None).map_err(|_| "Missing element: udpsrc")?;
    let appsink = gst::ElementFactory::make("appsink", Some("display")).map_err(|_| "Missing element: appsink")?;
    udpsrc.set_property("port", port as i32);
    let convert_caps = gst::caps::Caps::from_str("video/x-raw, format=RGB").map_err(|_| "Cannot create Caps")?;
    appsink.set_property("caps", convert_caps);
    let caps = gst::caps::Caps::from_str("application/x-rtp, media=(string)video").map_err(|_| "Cannot create Caps")?;
    udpsrc.set_property("caps", caps);
    let output_tee = gst::ElementFactory::make("tee", Some("output_tee")).map_err(|_| "Missing element: tee")?;
    let queue_to_app = gst::ElementFactory::make("queue", None).map_err(|_| "Missing element: queue")?;
    let videoconvert = gst::ElementFactory::make("videoconvert", None).map_err(|_| "Missing element: videoconvert")?;
    let (depay_elements, decoder_elements) = decoder.gst_elements()?;
    
    pipeline.add_many(&[&udpsrc, &appsink, &output_tee, &videoconvert, &queue_to_app]).map_err(|_| "Cannot create pipeline")?;
    for depay_element in &depay_elements {
        pipeline.add(depay_element).map_err(|_| "Cannot add a depay element")?;
    }
    for decoder_element in &decoder_elements {
        pipeline.add(decoder_element).map_err(|_| "Cannot add a decoder element")?;
    }
    for element in depay_elements.windows(2) {
        if let [a, b] = element {
            a.link(b).map_err(|_| "Cannot link elements inside depay_elements")?;
        }
    }
    for element in decoder_elements.windows(2) {
        if let [a, b] = element {
            a.link(b).map_err(|_| "Cannot link elements inside decoder_elements")?;
        }
    }
    match (depay_elements.first(), depay_elements.last()) {
        (Some(first), Some(last)) => {
            udpsrc.link(first).map_err(|_| "Cannot link udpsrc to first depay element")?;
            last.link(&output_tee).map_err(|_| "Cannot link last depay element to tee")?;
        },
        _ => return Err("depay_elements is empty"),
    }
    match (decoder_elements.first(), decoder_elements.last()) {
        (Some(first), Some(last)) => {
            queue_to_app.link(first).map_err(|_| "Cannot link queue to first decoder element")?;
            last.link(&videoconvert).map_err(|_| "Cannot link last decoder element to videoconvert")?;
        },
        _ => return Err("decoder_elements is empty"),
    }
    videoconvert.link(&appsink).map_err(|_| "Cannot link videoconvert to appsink")?;
    output_tee.request_pad_simple("src_%u").unwrap().link(&queue_to_app.static_pad("sink").unwrap()).map_err(|_| "Cannot link output_tee to queue")?;
    Ok(pipeline)
}

fn correct_underwater_color(src: Mat) -> Mat {
    // cv::Mat image;
    let mut image = Mat::default();
    // src.convertTo(image, CV_32FC3);
    src.convert_to(&mut image, cv::core::CV_32FC3, 1.0, 0.0).expect("Cannot convert src");
    // image /= 255.0f;
    let image = (image / 255.0).into_result().unwrap();
    // cv::split(image, tempVector);
    let mut channels = cv::types::VectorOfMat::new();
    cv::core::split(&image, &mut channels).expect("Cannot split image");
    // cv::Mat b = tempVector[0], g = tempVector[1], r = tempVector[2];
    // tempVector.clear();
    // cv::Scalar mean, std;
    let [mut mean, mut std] = [cv::core::Scalar::default(); 2];
    // cv::meanStdDev(image, mean, std);
    let image_original_size = image;
    let mut image = Mat::default();
    cv::imgproc::resize(&image_original_size, &mut image, Size::new(128, 128), 0.0, 0.0, imgproc::INTER_NEAREST).expect("Cannot resize image");
    cv::core::mean_std_dev(&image, &mut mean, &mut std, &cv::core::no_array()).expect("Cannot calculate mean std");
    const U: f64 = 3.0;
    // #define b_std std[0]
    // #define g_std std[1]
    // #define r_std std[2]
    // #define b_mean mean[0]
    // #define g_mean mean[1]
    // #define r_mean mean[2]
    // float b_max = b_mean + u * b_std;
    // float g_max = g_mean + u * g_std;
    // float r_max = r_mean + u * r_std;
    // float b_min = b_mean - u * b_std;
    // float g_min = g_mean - u * g_std;
    // float r_min = r_mean - u * r_std;
    let min_max = mean.iter().zip(std.iter()).map(|(mean, std)| (mean - U * std, mean + U * std));
    // cv::Mat b_cr = (b - b_min) / (b_max - b_min) * 255.0f;
    // cv::Mat g_cr = (g - g_min) / (g_max - g_min) * 255.0f;
    // cv::Mat r_cr = (r - r_min) / (r_max - r_min) * 255.0f;
    let channels = channels.iter().zip(min_max).map(|(channel, (min, max))| (channel - VecN::from(min)) / (max - min) * 255.0).map(|x| x.into_result().and_then(|x| x.to_mat()).unwrap());
    // tempVector.push_back(b_cr);
    // tempVector.push_back(g_cr);
    // tempVector.push_back(r_cr);
    let channels = VectorOfMat::from_iter(channels);
    // cv::Mat image_cr;
    // cv::merge(tempVector, image_cr);
    let mut image = Mat::default();
    cv::core::merge(&channels, &mut image).expect("Cannot merge channels");
    // tempVector.clear();
    // cv::Mat result;
    // image_cr.convertTo(result, CV_8UC3);
    let mut result = Mat::default();
    image.convert_to(&mut result, cv::core::CV_8UC3, 1.0, 0.0).expect("Cannot convert result");
    // return result;
    result
}

fn apply_clahe(mut mat: Mat) -> Mat {
    let mut channels = VectorOfMat::new();
    cv::core::split(&mat, &mut channels).expect("Cannot split image");
    if let Ok(mut clahe) = imgproc::create_clahe(2.0, Size::new(8, 8)) {
        for mut channel in channels.iter() {
            clahe.apply(&channel.clone(), &mut channel).expect("Cannot apply CLAHE");
        }
    }
    cv::core::merge(&channels, &mut mat).expect("Cannot merge channels");
    mat
}

pub fn attach_pipeline_callback(pipeline: &Pipeline, sender: Sender<Mat>, config: Arc<Mutex<SlaveConfigModel>>) -> Result<(), &'static str> {
    let video_decoder = pipeline.by_name("video_decoder").unwrap();
    let video_decoder_pad = video_decoder.static_pad("src").ok_or("Cannot get static pad of last decoder element")?;
    let frame_size: Arc<Mutex<Option<(i32, i32)>>> = Arc::new(Mutex::new(None));
    video_decoder_pad.add_probe(PadProbeType::EVENT_BOTH, clone!(@strong frame_size => move |pad, info| {
        match &info.data {
            Some(PadProbeData::Event(event)) => {
                if let EventView::Caps(caps) = event.view() {
                    let caps = caps.caps();
                    if let Some(structure) = caps.structure(0) {
                        match (structure.get("width"), structure.get("height")) {
                            (Ok(width), Ok(height)) => {
                                *frame_size.lock().unwrap() = Some((width, height));
                            },
                            _ => (),
                        }
                    }
                }
            },
            _ => (),
        }
        PadProbeReturn::Ok
    }));
    let appsink = pipeline.by_name("display").unwrap().dynamic_cast::<gst_app::AppSink>()
        .expect("Sink element is expected to be an appsink!");
    appsink.set_callbacks(
        gst_app::AppSinkCallbacks::builder()
            .new_sample(clone!(@strong frame_size => move |appsink| {
                println!("Appsink");
                let (width, height) = frame_size.lock().unwrap().ok_or(gst::FlowError::Flushing)?;
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
                    Mat::new_rows_cols_with_data(height, width, cv::core::CV_8UC3, map.as_ptr() as *mut c_void, cv::core::Mat_AUTO_STEP)
                }.map_err(|_| gst::FlowError::CustomError)?.clone();
                dbg!((width, height));
                let mut mat = _mat;// Mat::default();
                // imgproc::cvt_color(&_mat, &mut mat, imgproc::COLOR_YUV2RGBA_I420, 3).expect("Cannot convert frame color!");
                let mat = match config.lock() {
                    Ok(config) => {
                        match config.video_algorithms.first() {
                            Some(VideoAlgorithm::CLAHE) => {
                                /*Mat imageEnhance;  
                                    Mat kernel = (Mat_<float>(3, 3) << 0, -1, 0, 0, 5, 0, 0, -1, 0);  
                                filter2D(image, imageEnhance, CV_8UC3, kernel); */
                                // let kernel = unsafe{ Mat::new_size(Size::new(3, 3), cv::core::CV_8UC1) }.unwrap();
                                // imgproc::filter_2d(&mat.clone(), &mut mat, 1, &kernel, cv::core::Point::new(-1, -1), 0.0, cv::core::BORDER_DEFAULT).expect("Cannot apply filter");
                                correct_underwater_color(mat)
                            },
                            _ => mat,
                        }
                    },
                    Err(_) => {
                        eprintln!("Cannot lock video config!");
                        mat
                    },
                };
                sender.send(mat).expect("Cannot send Mat frame!");
                // glib::MainContext::default().invoke(move || {
                //     let bytes = glib::Bytes::from(mat.data_bytes().unwrap());
                //     let pixbuf = Pixbuf::from_bytes(&bytes, Colorspace::Rgb, false, 8, VIDEO_WIDTH, VIDEO_HEIGHT, 1);
                //     image.get().set_from_pixbuf(Some(&pixbuf));
                // });
                Ok(gst::FlowSuccess::Ok)
            }))
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
        let width = self.cols();
        let height = self.rows();
        // let bytes = glib::Bytes::from(self.data_bytes().unwrap());
        // let pixbuf = Pixbuf::from_bytes(&bytes, Colorspace::Rgb, false, 8, width, height, 1);
        let size = (width * height * 3) as usize;
        let pixbuf = Pixbuf::new(Colorspace::Rgb, false, 8, width, height).unwrap();
        unsafe {
            pixbuf.pixels()[..size].copy_from_slice(self.data_bytes().unwrap());
        }
        pixbuf
    }
}
