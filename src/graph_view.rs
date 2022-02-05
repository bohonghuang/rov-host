/* graph_view.rs modified from https://gitlab.gnome.org/World/Health/-/blob/master/src/views/graph_view.rs
 *
 * Copyright 2020-2021 Rasmus Thomsen <oss@cogitri.dev>
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
use gtk::{gdk, gio::subclass::prelude::*, glib, pango, prelude::*};
use std::{convert::TryInto, rc::Rc, cell::RefCell};

use self::imp::FnBoxedPoint;

/// A [Point] describes a single datapoint in a [GraphView]
#[derive(Debug, Clone, PartialEq)]
pub struct Point {
    pub time: f32,
    pub value: f32,
}

static HALF_X_PADDING: f32 = 20.0;
static HALF_Y_PADDING: f32 = 20.0;

mod imp {
    use super::{Point, HALF_X_PADDING, HALF_Y_PADDING};
    use gtk::{
        gdk::prelude::*,
        glib::{self, clone},
        pango,
        prelude::*,
        subclass::prelude::*,
    };
    use std::{cell::RefCell, convert::TryInto, f64::consts::PI, rc::Rc};

    #[derive(Clone, glib::Boxed)]
    #[boxed_type(name = "FnBoxedPoint")]
    #[allow(clippy::type_complexity)]
    pub struct FnBoxedPoint(pub Rc<RefCell<Option<Box<dyn Fn(&Point) -> String>>>>);
    
    impl FnBoxedPoint {
        pub fn new(func: Option<Box<dyn Fn(&Point) -> String>>) -> Self {
            Self(Rc::new(RefCell::new(func)))
        }
    }

    #[derive(Debug)]
    pub struct HoverPoint {
        pub point: Point,
        pub x: f32,
        pub y: f32,
    }

    pub struct GraphViewMut {
        pub height: f32,
        pub width: f32,
        pub points: Vec<Point>,
        pub scale_x: f32,
        pub scale_y: f32,
        pub upper_value: f32,
        pub lower_value: f32,
    }

    pub struct GraphView {
        pub inner: RefCell<GraphViewMut>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for GraphView {
        const NAME: &'static str = "HealthGraphView";
        type ParentType = gtk::Widget;
        type Type = super::GraphView;

        fn new() -> Self {
            Self {
                inner: RefCell::new(GraphViewMut {
                    height: 0.0,
                    points: Vec::new(),
                    scale_x: 0.0,
                    scale_y: 0.0,
                    width: 0.0,
                    upper_value: 100.0,
                    lower_value: -100.0,
                }),
            }
        }

        fn class_init(klass: &mut Self::Class) {
            klass.set_layout_manager_type::<gtk::BinLayout>();
        }
    }

    impl WidgetImpl for GraphView {
        fn snapshot(&self, widget: &Self::Type, snapshot: &gtk::Snapshot) {
            let mut inner = self.inner.borrow_mut();

            inner.height = widget.height() as f32 - HALF_Y_PADDING * 2.0;
            inner.width = widget.width() as f32 - HALF_X_PADDING * 2.0;
            
            if inner.points.is_empty() {
                inner.scale_x = inner.width;
                inner.scale_y = inner.height / 10000.0;

            } else {
                // If we have more than one points, we don't want an empty point at the end of the graph
                inner.scale_x = if inner.points.len() > 1 {
                    inner.width / (inner.points.len() - 1) as f32
                } else {
                    inner.width as f32
                };
                inner.scale_y = inner.height / (inner.upper_value - inner.lower_value);
            };

            let cr = snapshot.append_cairo(&gtk::graphene::Rect::new(
                0.0,
                0.0,
                widget.width() as f32,
                widget.height() as f32,
            ));
            let style_context = widget.style_context();
            let background_color = style_context.lookup_color("insensitive_fg_color").unwrap();

            GdkCairoContextExt::set_source_rgba(&cr, &background_color);
            /*
                Draw outlines
            */
            cr.save().unwrap();
            cr.set_line_width(0.5);
            cr.set_dash(&[10.0, 5.0], 0.0);

            for i in 0..4 {
                let mul = inner.height / 4.0;
                cr.move_to(
                    f64::from(inner.width + HALF_Y_PADDING),
                    f64::from(mul * i as f32 + HALF_Y_PADDING),
                );
                cr.line_to(
                    f64::from(HALF_X_PADDING),
                    f64::from(mul * i as f32 + HALF_Y_PADDING),
                );
                let layout = widget.create_pango_layout(Some(
                    &(inner.lower_value + (inner.upper_value - inner.lower_value) / 4.0 * (4 - i) as f32).to_string(),
                ));
                let (_, extents) = layout.extents();

                cr.rel_move_to(0.0, pango::units_to_double(extents.height()) * -1.0);
                pangocairo::show_layout(&cr, &layout);
            }

            cr.stroke().expect("Couldn't stroke on Cairo Context");
            cr.restore().unwrap();

            /*
                Draw X Ticks (dates)
            */

            cr.save().unwrap();

            for (i, point) in inner.points.iter().enumerate() {
                let layout = widget.create_pango_layout(None);
                let (_, extents) = layout.extents();

                cr.move_to(
                    f64::from(i as f32 * inner.scale_x + HALF_X_PADDING)
                        - pango::units_to_double(extents.width()) / 2.0,
                    f64::from(inner.height + HALF_Y_PADDING * 1.5)
                        - pango::units_to_double(extents.height()) / 2.0,
                );
                pangocairo::show_layout(&cr, &layout);
            }

            cr.stroke().expect("Couldn't stroke on Cairo Context");
            cr.restore().unwrap();

            if inner.points.is_empty() {
                return;
            }

            /*
                Draw a point for each datapoint
            */
            cr.save().unwrap();

            let graph_color = style_context.lookup_color("accent_bg_color").unwrap();
            GdkCairoContextExt::set_source_rgba(&cr, &graph_color);
            cr.set_line_width(4.0);
            for (i, point) in inner.points.iter().enumerate() {
                let x = f64::from(i as f32 * inner.scale_x + HALF_X_PADDING);
                let y = f64::from(inner.height - (point.value - inner.lower_value) * inner.scale_y + HALF_Y_PADDING);

                cr.move_to(x, y);
                cr.arc(x, y, 1.0, 0.0, 2.0 * PI);
            }

            cr.stroke().expect("Couldn't stroke on Cairo Context");
            cr.restore().unwrap();

            /*
                Draw the graph itself
            */
            cr.save().unwrap();

            GdkCairoContextExt::set_source_rgba(&cr, &graph_color);
            cr.move_to(
                f64::from(HALF_X_PADDING),
                f64::from(
                    inner.height - (inner.points.get(0).unwrap().value - inner.lower_value) * inner.scale_y
                        + HALF_Y_PADDING,
                ),
            );

            for (i, point) in inner.points.iter().enumerate() {
                let next_value = if (i + 1) >= inner.points.len() {
                    break;
                } else {
                    inner.points.get(i + 1).unwrap().value - inner.lower_value
                };
                let smoothness_factor = 0.5;

                cr.curve_to(
                    f64::from((i as f32 + smoothness_factor) * inner.scale_x + HALF_X_PADDING),
                    f64::from(inner.height - (point.value - inner.lower_value) * inner.scale_y + HALF_Y_PADDING),
                    f64::from(
                        ((i + 1) as f32 - smoothness_factor) * inner.scale_x + HALF_X_PADDING,
                    ),
                    f64::from(inner.height - next_value * inner.scale_y + HALF_Y_PADDING),
                    f64::from((i + 1) as f32 * inner.scale_x + HALF_X_PADDING),
                    f64::from(inner.height - next_value * inner.scale_y + HALF_Y_PADDING),
                );
            }

            cr.line_to(
                f64::from(inner.width + HALF_X_PADDING),
                f64::from(
                    inner.height - (inner.points.last().unwrap().value - inner.lower_value) * inner.scale_y
                        + HALF_Y_PADDING,
                ),
            );
            cr.stroke_preserve()
                .expect("Couldn't stroke on Cairo Context");

            cr.set_line_width(0.0);
            cr.line_to(
                f64::from(inner.width + HALF_X_PADDING),
                f64::from(inner.height + HALF_Y_PADDING),
            );
            cr.line_to(
                f64::from(HALF_X_PADDING),
                f64::from(inner.height + HALF_Y_PADDING),
            );
            cr.close_path();

            cr.set_source_rgba(
                f64::from(graph_color.red()),
                f64::from(graph_color.green()),
                f64::from(graph_color.blue()),
                0.65,
            );
            cr.stroke_preserve()
                .expect("Couldn't stroke on Cairo Context");
            cr.fill().expect("Couldn't fill Cairo Context");
            cr.restore().unwrap();
        }
    }

    impl ObjectImpl for GraphView {
        fn constructed(&self, obj: &Self::Type) {
            self.parent_constructed(obj);

            obj.set_hexpand(true);
            obj.set_vexpand(true);
            let gesture_controller = gtk::GestureClick::new();
            gesture_controller.set_touch_only(true);
            gesture_controller.connect_pressed(
                clone!(@weak obj => move |c, _, x, y| obj.on_motion_event(x, y, true, c)),
            );
            obj.add_controller(&gesture_controller);

            let motion_controller = gtk::EventControllerMotion::new();
            motion_controller.connect_enter(
                clone!(@weak obj => move|c, x, y| obj.on_motion_event(x, y, false, c)),
            );
            motion_controller.connect_motion(
                clone!(@weak obj => move|c, x, y| obj.on_motion_event(x, y, false, c)),
            );
            obj.add_controller(&motion_controller);
        }
        fn properties() -> &'static [glib::ParamSpec] {
            use once_cell::sync::Lazy;
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecFloat::new(
                        "upper-value",
                        "upper-value",
                        "upper-value",
                        f32::MIN,
                        f32::MAX,
                        0.0,
                        glib::ParamFlags::READWRITE,
                    ),
                    glib::ParamSpecFloat::new(
                        "lower-value",
                        "lower-value",
                        "lower-value",
                        f32::MIN,
                        f32::MAX,
                        0.0,
                        glib::ParamFlags::READWRITE,
                    )
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(
            &self,
            obj: &Self::Type,
            _id: usize,
            value: &glib::Value,
            pspec: &glib::ParamSpec,
        ) {
            match pspec.name() {
                "upper-value" => {
                    self.inner.borrow_mut().upper_value = value.get().unwrap();
                    obj.queue_draw();
                }
                "lower-value" => {
                    self.inner.borrow_mut().lower_value = value.get().unwrap();
                    obj.queue_draw();
                }
                _ => unimplemented!(),
            }
        }

        fn property(&self, _obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "upper-value" => self.inner.borrow().upper_value.to_value(),
                "lower-value" => self.inner.borrow().lower_value.to_value(),
                _ => unimplemented!(),
            }
        }
    }
}

glib::wrapper! {
    /// A View for visualizing the development of data over time.
    pub struct GraphView(ObjectSubclass<imp::GraphView>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl GraphView {
    pub fn new() -> Self {
        glib::Object::new(&[]).expect("Failed to create GraphView")
    }

    pub fn limit(&self) -> Option<f32> {
        let val = self.property::<f32>("limit");
        if val < 0.0 {
            None
        } else {
            Some(val)
        }
    }

    pub fn limit_label(&self) -> Option<String> {
        self.property("limit-label")
    }

    /// Set the function that should be called when the user hovers over a point.
    ///
    /// # Arguments
    /// * `hover_func` - A function that takes a `Point` and renders it to a string that is displayed as tooltip on the graph.
    pub fn set_hover_func(&self, hover_func: Option<Box<dyn Fn(&Point) -> String>>) {
        self.set_property("hover-func", FnBoxedPoint::new(hover_func))
    }

    /// Set the limit (e.g. step goal) that is marked in the graph.
    pub fn set_limit(&self, limit: Option<f32>) {
        self.set_property("limit", limit.unwrap_or(-1.0))
    }

    /// Set the label that should be displayed on the limit label.
    pub fn set_limit_label(&self, limit_label: Option<String>) {
        self.set_property("limit-label", limit_label)
    }

    /// Sets the points that should be rendered in the graph view.
    pub fn set_points(&self, points: Vec<Point>) {
        let layout = self.create_pango_layout(Some("Graph"));
        let (_, extents) = layout.extents();
        let datapoint_width = pango::units_to_double(extents.width()) + f64::from(HALF_X_PADDING);

        // self.set_size_request(
        //     (datapoint_width as usize * points.len())
        //         .try_into()
        //         .unwrap(),
        //     -1,
        // );

        let mut inner = self.imp().inner.borrow_mut();

        inner.points = points;
        self.queue_draw();
    }
    
    pub fn set_upper_value(&self, upper_value: f32) {
        self.set_property("upper-value", upper_value)
    }

    pub fn upper_value(&self) -> f32 {
        self.property("upper-value")
    }

    pub fn set_lower_value(&self, lower_value: f32) {
        self.set_property("lower-value", lower_value);
    }
    
    pub fn lower_value(&self) -> f32 {
        self.property("lower-value")
    }

    fn on_motion_event(
        &self,
        x: f64,
        y: f64,
        allow_touch: bool,
        controller: &impl IsA<gtk::EventController>,
    ) {
        let mut inner = self.imp().inner.borrow_mut();

        // Don't handle touch events, we do that via Gtk.GestureClick.
        if !allow_touch {
            if let Some(device) = controller.current_event_device() {
                if device.source() == gdk::InputSource::Touchscreen {
                    return;
                }
            }
        }
    }
}

// #[derive(Clone, glib::Boxed)]
// #[boxed_type(name = "FnBoxedTuple")]
// #[allow(clippy::type_complexity)]
// pub struct FnBoxedTuple(pub Rc<RefCell<Option<Box<dyn Fn(&Tuple) -> String>>>>);

// impl FnBoxedTuple {
//     pub fn new(func: Option<Box<dyn Fn(&Tuple) -> String>>) -> Self {
//         Self(Rc::new(RefCell::new(func)))
//     }
// }

