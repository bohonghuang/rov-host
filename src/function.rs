pub trait Function1<T1, U> {
    fn apply(self, args: T1) -> U;
}

impl <F, T1, U> Function1<T1, U> for F where F: Fn(T1) -> U {
    fn apply(self, t1: T1) -> U {
        self(t1)
    }
}

pub trait Function2<T1, T2, U> {
    fn apply(self, args: (T1, T2)) -> U;
}

impl <F, T1, T2, U> Function2<T1, T2, U> for F where F: Fn(T1, T2) -> U {
    fn apply(self, (t1, t2): (T1, T2)) -> U {
        self(t1, t2)
    }
}
