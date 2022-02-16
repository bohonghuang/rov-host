use std::{ffi::c_void, str::FromStr, sync::{Arc, Mutex}};

use glib::{Sender, clone, EnumClass};
use gtk::{gdk_pixbuf::{Colorspace, Pixbuf}, prelude::*};
use gstreamer as gst;
use gstreamer_app as gst_app;
use gst::{Element, Pad, PadProbeType, Pipeline, element_error, prelude::*, PadProbeReturn, PadProbeData, EventView};

use opencv as cv;
use cv::{core::VecN, types::VectorOfMat};
use cv::{prelude::*, Result, imgproc, core::Size};

use serde::{Serialize, Deserialize};

use strum_macros::{EnumIter, EnumString as EnumFromString, Display as EnumToString};

use crate::async_glib::{Future, Promise};

use super::slave_config::SlaveConfigModel;

#[derive(EnumIter, EnumToString, EnumFromString, PartialEq, Clone, Copy, Debug, Serialize, Deserialize)]
pub enum ImageFormat {
    JPEG, PNG, TIFF, BMP
}

impl ImageFormat {
    pub fn from_extension(extension: &str) -> Option<ImageFormat> {
        match extension {
            "jpg" | "jpeg" => Some(ImageFormat::JPEG),
            "png" => Some(ImageFormat::PNG),
            "tiff" => Some(ImageFormat::TIFF),
            "bmp" => Some(ImageFormat::BMP),
            _ => None, 
        }
    }
    pub fn extension(&self) -> &'static str {
        match self {
            ImageFormat::JPEG => "jpg",
            ImageFormat::PNG => "png",
            ImageFormat::TIFF => "tiff",
            ImageFormat::BMP => "bmp",
        }
    }
}

#[derive(EnumIter, EnumToString, EnumFromString, PartialEq, Clone, Debug)]
pub enum VideoAlgorithm {
    CLAHE, Algorithm1, Algorithm2, Algorithm3, Algorithm4
}

#[derive(EnumIter, EnumFromString, PartialEq, Clone, Debug, Serialize, Deserialize, Copy)]
pub enum VideoEncoder {
    H264Software, H265Software, VP8Software, VP9Software, H264HardwareNvidia, H265HardwareNvidia
}

impl VideoEncoder {
    pub fn gst_record_elements(&self, filename: &str) -> Result<Vec<Element>, &'static str> {
        let mut elements = Vec::new();
        let queue_to_file = gst::ElementFactory::make("queue", None).map_err(|_| "Missing element: queue")?;
        elements.push(queue_to_file);
        match self {
            VideoEncoder::H264Software => {
                let encoder = gst::ElementFactory::make("x264enc", None).map_err(|_| "Missing element: x264enc")?;
                // encoder.set_property_from_value("tune", &FlagsClass::new(encoder.property_type("tune").unwrap()).unwrap().to_value(0).unwrap());
                elements.push(encoder);
            },
            VideoEncoder::H265Software => {
                let videoconvert = gst::ElementFactory::make("videoconvert", None).map_err(|_| "Missing element: videoconvert")?;
                elements.push(videoconvert);
                let encoder = gst::ElementFactory::make("x265enc", None).map_err(|_| "Missing element: x265enc")?;
                elements.push(encoder);
                let h265parse = gst::ElementFactory::make("h265parse", None).map_err(|_| "Missing element: h265parse")?;
                elements.push(h265parse);
            },
            VideoEncoder::VP9Software => {
                let videoconvert = gst::ElementFactory::make("videoconvert", None).map_err(|_| "Missing element: videoconvert")?;
                elements.push(videoconvert);
                let encoder = gst::ElementFactory::make("vp9enc", None).map_err(|_| "Missing element: vp9enc")?;
                elements.push(encoder);
            },
            VideoEncoder::H264HardwareNvidia => {
                let videoconvert = gst::ElementFactory::make("videoconvert", None).map_err(|_| "Missing element: videoconvert")?;
                elements.push(videoconvert);
                let encoder = gst::ElementFactory::make("nvh264enc", None).map_err(|_| "Missing element: nvh264enc")?;
                elements.push(encoder);
            },
            VideoEncoder::H265HardwareNvidia => {
                let videoconvert = gst::ElementFactory::make("videoconvert", None).map_err(|_| "Missing element: videoconvert")?;
                elements.push(videoconvert);
                let encoder = gst::ElementFactory::make("nvh265enc", None).map_err(|_| "Missing element: nvh265enc")?;
                elements.push(encoder);
                let h265parse = gst::ElementFactory::make("h265parse", None).map_err(|_| "Missing element: h265parse")?;
                elements.push(h265parse);
            },
            VideoEncoder::VP8Software => {
                let videoconvert = gst::ElementFactory::make("videoconvert", None).map_err(|_| "Missing element: videoconvert")?;
                elements.push(videoconvert);
                let encoder = gst::ElementFactory::make("vp8enc", None).map_err(|_| "Missing element: vp8enc")?;
                elements.push(encoder);
            },
        };
        let matroskamux = gst::ElementFactory::make("matroskamux", None).map_err(|_| "Missing element: matroskamux")?;
        elements.push(matroskamux);
        let filesink = gst::ElementFactory::make("filesink", None).map_err(|_| "Missing element: filesink")?;
        filesink.set_property("location", filename);
        elements.push(filesink);
        Ok(elements)
    }
}

impl ToString for VideoEncoder {
    fn to_string(&self) -> String {
        match self {
            VideoEncoder::H264Software => "H.264 (CPU)",
            VideoEncoder::H264HardwareNvidia => "H.264 (NVIDIA)",
            VideoEncoder::H265Software => "H.265 (CPU)",
            VideoEncoder::H265HardwareNvidia => "H.265 (NVIDIA)",
            VideoEncoder::VP8Software => "VP8 (CPU)",
            VideoEncoder::VP9Software => "VP9 (CPU)",
        }.to_string()
    }
}

#[derive(EnumIter, EnumFromString, PartialEq, Clone, Debug, Serialize, Deserialize, Copy)]
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
    pub fn gst_record_elements(&self, filename: &str) -> Result<Vec<Element>, &'static str> {
        let parse = match self {
            VideoDecoder::H264Software | VideoDecoder::H264HardwareNvidia | VideoDecoder::H264HardwareNvidiaStateless => gst::ElementFactory::make("h264parse", None).map_err(|_| "Missing element: h264parse")?,
            VideoDecoder::H265Software | VideoDecoder::H265HardwareNvidia => gst::ElementFactory::make("h265parse", None).map_err(|_| "Missing element: h265parse")?,
        };
        let queue_to_file = gst::ElementFactory::make("queue", None).map_err(|_| "Missing element: queue")?;
        let filesink = gst::ElementFactory::make("filesink", None).map_err(|_| "Missing element: filesink")?;
        filesink.set_property("location", filename);
        let matroskamux = gst::ElementFactory::make("matroskamux", None).map_err(|_| "Missing element: matroskamux")?;
        Ok(vec![queue_to_file, parse, matroskamux, filesink])
    }
    
    pub fn gst_main_elements(&self) -> Result<(Vec<Element>, Vec<Element>), &'static str> {
        match self {
            decoder_h264 @ (VideoDecoder::H264Software | VideoDecoder::H264HardwareNvidia | VideoDecoder::H264HardwareNvidiaStateless) => {
                let rtph264depay = gst::ElementFactory::make("rtph264depay", None).map_err(|_| "Missing element: rtph264depay")?;
                let h264parse = gst::ElementFactory::make("h264parse", None).map_err(|_| "Missing element: h264parse")?;
                let decoder_name = match decoder_h264 {
                    VideoDecoder::H264Software => "avdec_h264",
                    VideoDecoder::H264HardwareNvidia => "nvh264dec", // TODO: 未知原因，使用该解码器时，使用有延迟的编码器将导致录制与拉流无法同时启动
                    VideoDecoder::H264HardwareNvidiaStateless => "nvh264sldec",
                    _ => unreachable!(),
                };
                let decoder = gst::ElementFactory::make(decoder_name, Some("video_decoder")).map_err(|_| "The configured video decoder is unavailable currently")?;
                Ok((vec![rtph264depay], if decoder_h264 == &VideoDecoder::H264Software { vec![decoder] } else { vec![h264parse, decoder] }))
            },
            decoder_h265 @ (VideoDecoder::H265Software | VideoDecoder::H265HardwareNvidia) => {
                let rtph265depay = gst::ElementFactory::make("rtph265depay", None).map_err(|_| "Missing element: rtph265depay")?;
                let h265parse = gst::ElementFactory::make("h265parse", None).map_err(|_| "Missing element: h265parse")?;
                let decoder_name = match decoder_h265 {
                    VideoDecoder::H265Software => "avdec_h265",
                    VideoDecoder::H265HardwareNvidia => "nvh265dec",
                    _ => unreachable!(),
                };
                let decoder = gst::ElementFactory::make(decoder_name, Some("video_decoder")).map_err(|_| "The configured video decoder is unavailable currently")?;
                Ok((vec![rtph265depay], vec![h265parse, decoder]))
            },
        }
    }
}

impl Default for VideoEncoder {
    fn default() -> Self { Self::H264Software }
}

impl Default for VideoDecoder {
    fn default() -> Self { Self::H264Software }
}

pub fn connect_elements_to_pipeline(pipeline: &Pipeline, tee_name: &str, elements: &[Element]) -> Result<(Element, Pad), &'static str> {
    let output_tee = pipeline.by_name(tee_name).ok_or("Cannot find output tee")?;
    if let Some(element) = elements.first() {
        pipeline.add(element).map_err(|_| "Cannot add an element")?; // 必须先添加，再连接
    }
    let teepad = output_tee.request_pad_simple("src_%u").ok_or("Cannot request pad")?;
    for elements in elements.windows(2) {
        if let [a, b] = elements {
            pipeline.add(b).map_err(|_| "Cannot add an element")?;
            a.link(b).map_err(|_| "Cannot link elements")?;
        }
    }
    let sinkpad = elements.first().unwrap().static_pad("sink").unwrap();
    teepad.link(&sinkpad).map_err(|_| "Cannot link output_tee pad to sink pad")?;
    output_tee.sync_state_with_parent().unwrap();
    for element in elements {
        element.sync_state_with_parent().unwrap();
    }
    Ok((output_tee, teepad))
}

pub fn disconnect_elements_to_pipeline(pipeline: &Pipeline, (output_tee, teepad): &(Element, Pad), elements: &[Element]) -> Result<Future<()>, &'static str> {
    let first_sinkpad = elements.first().unwrap().static_pad("sink").unwrap();
    teepad.unlink(&first_sinkpad).map_err(|_| "Cannot unlink elements")?;
    output_tee.remove_pad(teepad).map_err(|_| "Cannot remove pad")?;
    let last_sinkpad = elements.last().unwrap().sink_pads().into_iter().next().unwrap();
    let elements = elements.to_vec();
    let promise = Promise::new();
    let future = promise.future();
    let promise = Mutex::new(Some(promise));
    last_sinkpad.add_probe(PadProbeType::EVENT_BOTH, move |_pad, info| {
        match &info.data {
            Some(PadProbeData::Event(event)) => {
                if let EventView::Eos(_) = event.view() {
                    promise.lock().unwrap().take().unwrap().success(());
                    PadProbeReturn::Remove
                } else {
                    PadProbeReturn::Ok
                }
            },
            _ => PadProbeReturn::Ok,
        }
        
    });
    first_sinkpad.send_event(gst::event::Eos::new());
    let future = future.map(clone!(@strong pipeline => move |_| {
        pipeline.remove_many(&elements.iter().collect::<Vec<_>>()).map_err(|_| "Cannot remove elements").unwrap();
        for element in elements.iter() {
            element.set_state(gst::State::Null).unwrap();
        }
    }));
    Ok(future)
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
    let tee_raw = gst::ElementFactory::make("tee", Some("tee_raw")).map_err(|_| "Missing element: tee")?;
    let tee_decoded = gst::ElementFactory::make("tee", Some("tee_decoded")).map_err(|_| "Missing element: tee")?;
    let queue_to_decode = gst::ElementFactory::make("queue", None).map_err(|_| "Missing element: queue")?;
    let queue_to_app = gst::ElementFactory::make("queue", None).map_err(|_| "Missing element: queue")?;
    let videoconvert = gst::ElementFactory::make("videoconvert", None).map_err(|_| "Missing element: videoconvert")?;
    let (depay_elements, decoder_elements) = decoder.gst_main_elements()?;
    
    pipeline.add_many(&[&udpsrc, &appsink, &tee_decoded, &tee_raw, &videoconvert, &queue_to_app, &queue_to_decode]).map_err(|_| "Cannot create pipeline")?;
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
            last.link(&tee_raw).map_err(|_| "Cannot link last depay element to tee")?;
        },
        _ => return Err("depay_elements is empty"),
    }
    match (decoder_elements.first(), decoder_elements.last()) {
        (Some(first), Some(last)) => {
            queue_to_decode.link(first).map_err(|_| "Cannot link queue to first decoder element")?;
            last.link(&tee_decoded).unwrap();
        },
        _ => return Err("decoder_elements is empty"),
    }
    queue_to_app.link(&videoconvert).map_err(|_| "Cannot link last decoder element to videoconvert")?;
    queue_to_app.set_property_from_value("leaky", &EnumClass::new(queue_to_app.property_type("leaky").unwrap()).unwrap().to_value(2).unwrap());
    // appsink.set_property("sync", true);
    videoconvert.link(&appsink).map_err(|_| "Cannot link videoconvert to appsink")?;
    tee_raw.request_pad_simple("src_%u").unwrap().link(&queue_to_decode.static_pad("sink").unwrap()).map_err(|_| "Cannot link tee to queue")?;
    tee_decoded.request_pad_simple("src_%u").unwrap().link(&queue_to_app.static_pad("sink").unwrap()).map_err(|_| "Cannot link tee to queue")?;
    Ok(pipeline)
}

fn correct_underwater_color(src: Mat) -> Mat {
    let mut image = Mat::default();
    src.convert_to(&mut image, cv::core::CV_32FC3, 1.0, 0.0).expect("Cannot convert src");
    let image = (image / 255.0).into_result().unwrap();
    let mut channels = cv::types::VectorOfMat::new();
    cv::core::split(&image, &mut channels).expect("Cannot split image");
    let [mut mean, mut std] = [cv::core::Scalar::default(); 2];
    let image_original_size = image;
    let mut image = Mat::default();
    cv::imgproc::resize(&image_original_size, &mut image, Size::new(128, 128), 0.0, 0.0, imgproc::INTER_NEAREST).expect("Cannot resize image");
    cv::core::mean_std_dev(&image, &mut mean, &mut std, &cv::core::no_array()).expect("Cannot calculate mean std");
    const U: f64 = 3.0;
    let min_max = mean.iter().zip(std.iter()).map(|(mean, std)| (mean - U * std, mean + U * std));
    let channels = channels.iter().zip(min_max).map(|(channel, (min, max))| (channel - VecN::from(min)) / (max - min) * 255.0).map(|x| x.into_result().and_then(|x| x.to_mat()).unwrap());
    let channels = VectorOfMat::from_iter(channels);
    let mut image = Mat::default();
    cv::core::merge(&channels, &mut image).expect("Cannot merge channels");
    let mut result = Mat::default();
    image.convert_to(&mut result, cv::core::CV_8UC3, 1.0, 0.0).expect("Cannot convert result");
    result
}

#[allow(dead_code)]
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
    video_decoder_pad.add_probe(PadProbeType::EVENT_BOTH, clone!(@strong frame_size => move |_pad, info| {
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
            // .new_event(move |appsink| {
            //     Ok(gst::FlowSuccess::Ok)
            // }) // gstreamer 1.19
            .new_sample(clone!(@strong frame_size => move |appsink| {
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
                let mat = unsafe {
                    Mat::new_rows_cols_with_data(height, width, cv::core::CV_8UC3, map.as_ptr() as *mut c_void, cv::core::Mat_AUTO_STEP)
                }.map_err(|_| gst::FlowError::CustomError)?.clone();
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
