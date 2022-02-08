use glib::{ObjectExt as GlibObjectExt, ObjectType};

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
