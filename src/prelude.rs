use glib::{ObjectExt as GlibObjectExt, ObjectType};
use adw::Carousel;
use gtk::{prelude::*, Window};

pub trait ObjectExt {
    fn put_data<QD: 'static>(&self, key: &str, value: QD);
    fn get_data<QD: 'static>(&self, key: &str) -> Option<&'static QD>;
}

impl<T: ObjectType> ObjectExt for T {
    fn put_data<OD: 'static>(&self, name: &str, data: OD) {
        unsafe {
            self.set_data(name, data)
        }
    }
    fn get_data<QD: 'static>(&self, key: &str) -> Option<&'static QD> {
        unsafe {
            self.data(key).map(|x| x.as_ref())
        }
    }
}

pub trait CarouselExt {
    fn scroll_to_page(&self, page_index: u32, animate: bool);
}

impl CarouselExt for Carousel {
    fn scroll_to_page(&self, page_index: u32, animate: bool) {
        self.scroll_to(&self.nth_page(page_index), animate);
    }
}

pub trait WindowExt {
    fn set_distroy(&self, destroy: bool);
}

impl<T: IsA<Window>> WindowExt for T {
    fn set_distroy(&self, destroy: bool) {
        if destroy {
            self.destroy();
        }
    }
}
