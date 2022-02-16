use std::sync::{Arc, Mutex};

use glib::{Sender, MainContext, Continue, clone};
use once_cell::sync::OnceCell;

pub struct Future<T> where T: Send {
    callbacks: Arc<Mutex<Vec<Box<dyn FnOnce(Arc<T>) + Send>>>>,
    state: Arc<Mutex<Option<Result<Arc<T>, Arc<dyn ToString + Send + Sync>>>>>,
}

impl<T> Clone for Future<T> where T: Send + Sync {
    fn clone(&self) -> Self {
        Self {
            callbacks: self.callbacks.clone(),
            state: self.state.clone(),
        }
    }
}

impl<T> Future<T> where T: Send + Sync + 'static {
    fn new() -> Self {
        Self {
            callbacks: Default::default(),
            state: Default::default(),
        }
    }

    fn success(&mut self, value: Arc<T>) {
        *self.state.lock().unwrap() = Some(Ok(value.clone()));
        while let Some(callback) = self.callbacks.lock().unwrap().pop() {
            (callback)(value.clone());
        }
    }

    pub fn sequence<I: Iterator<Item = Future<T>> + Send + 'static>(iter: I) -> Future<Vec<Arc<T>>> {
        let seq: Arc<Mutex<Option<Vec<Arc<T>>>>> = Arc::new(Mutex::new(Some(Vec::new())));
        let next: Arc<OnceCell<Box<dyn (Fn(I) -> Future<Vec<Arc<T>>>) + Send + Sync>>> = Default::default();
        next.clone().get_or_init(|| Box::new(move |mut iter| {
            let seq = seq.clone();
            match iter.next() {
                Some(future) => {
                    let next = next.clone();
                    future.flat_map(move |value| {
                        seq.lock().unwrap().as_mut().unwrap().push(value);
                        (next.get().unwrap())(iter)
                    })
                },
                None => seq.lock().unwrap().take().unwrap().into(),
            }
        }))(iter)
    }

    pub fn map<U, F>(&self, f: F) -> Future<U> where U: Send + Sync + 'static, F: FnOnce(Arc<T>) -> U + Send + 'static {
        let promise = Promise::new();
        let future = promise.future();
        self.for_each(move |result| {
            promise.success(f(result));
        });
        future
    }

    pub fn flat_map<U, F>(&self, f: F) -> Future<U> where U: Send + Sync + Clone + 'static, F: FnOnce(Arc<T>) -> Future<U> + Send + 'static {
        let promise = Promise::new();
        let future = promise.future();
        self.for_each(move |result| {
            f(result).for_each(move |result| promise.success(result.as_ref().clone()));
        });
        future
    }

    pub fn for_each<F>(&self, f: F) where F: FnOnce(Arc<T>) + Send + 'static {
        match self.state.lock().unwrap().as_ref() {
            Some(result) => match result {
                Ok(result) => f(result.clone()),
                Err(_) => (),
            },
            None => self.callbacks.lock().unwrap().push(Box::new(f)),
        }
    }
}

impl<T> From<T> for Future<T> where T: Send + Sync + 'static {
    fn from(t: T) -> Self {
        let promise = Promise::new();
        let future = promise.future();
        promise.success(t);
        future
    }
}

pub struct Promise<T> where T: Send + Sync {
    sender: Sender<Arc<T>>,
    future: Future<T>,
}

impl<T> Promise<T> where T: Send + Sync + 'static {
    pub fn new() -> Self {
        let (sender, receiver) = MainContext::channel(glib::PRIORITY_DEFAULT);
        let future = Future::new();
        receiver.attach(None, clone!(@strong future => move |result| {
            future.clone().success(result);
            Continue(false)
        }));
        Promise {
            sender,
            future,
        }
    }
    
    pub fn success(self, value: T) {
        self.sender.send(Arc::new(value)).unwrap();
    }

    pub fn future(&self) -> Future<T> {
        self.future.clone()
    }
}
