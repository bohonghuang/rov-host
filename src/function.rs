/* function.rs
 *
 * Copyright 2021-2022 Bohong Huang
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
