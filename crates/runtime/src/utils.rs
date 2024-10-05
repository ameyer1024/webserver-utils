
pub mod parse_mut {
    pub fn take_char(input: &mut &str) -> Option<char> {
        let mut chars = input.chars();
        let next = chars.next();
        *input = chars.as_str();
        next
    }
    pub fn take_while<'a>(input: &mut &'a str, f: impl Fn(char) -> bool) -> &'a str {
        for (i, c) in input.char_indices() {
            if !f(c) {
                let (found, rest) = input.split_at(i);
                *input = rest;
                return found;
            }
        }
        std::mem::replace(input, "")
    }
}

pub mod parse {
    pub fn take_char(input: &str) -> (Option<char>, &str) {
        let mut chars = input.chars();
        let next = chars.next();
        (next, chars.as_str())
    }
    pub fn take_while(input: &str, f: impl Fn(char) -> bool) -> (&str, &str) {
        for (i, c) in input.char_indices() {
            if !f(c) { return input.split_at(i); }
        }
        (input, "")
    }
}


// Adapted from anyhow's debug formatter
struct Indented<'a, T> {
    inner: &'a mut T,
    indent: usize,
    prefix: Option<&'a str>,
}
impl<'a, T> Indented<'a, T> where T: std::fmt::Write {
    fn new(inner: &'a mut T, indent: usize, prefix: Option<&'a str>) -> Self {
        Indented {
            inner,
            indent,
            prefix,
        }
    }
}
impl<'a, T> std::fmt::Write for Indented<'a, T> where T: std::fmt::Write {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        let mut first_segment = true;
        for line in s.split('\n') {
            if let Some(prefix) = self.prefix.take() {
                write!(self.inner, "{:>width$}", prefix, width=self.indent)?;
            } else if !first_segment {
                // If we have reached a newline; print out an equivalent and then indentation
                self.inner.write_char('\n')?;
                write!(self.inner, "{:>width$}", "", width=self.indent)?;
            }
            self.inner.write_str(line)?;
            first_segment = false;
        }
        Ok(())
    }
}

pub fn format_error<E, W>(f: &mut W, error: &E) -> Result<(), std::fmt::Error> where W: std::fmt::Write, E: std::error::Error {
    use std::fmt::Write;
    write!(f, "{}", error)?;

    if let Some(cause) = error.source() {
        write!(f, "\n\nCaused by:")?;
        let mut next_cause = Some(cause);
        let mut n = 0;
        while let Some(cause) = next_cause {
            writeln!(f)?;
            let prefix = format!("{}: ", n);
            let mut indented = Indented::new(f, 7, Some(&prefix));
            write!(indented, "{}", cause)?;
            next_cause = cause.source();
            n += 1;
        }
    }
    Ok(())
}

pub fn format_error_disp<'a, E>(e: &'a E) -> impl std::fmt::Display + 'a where E: std::error::Error {
    struct Disp<'a, E>(&'a E);
    impl<E> std::fmt::Display for Disp<'_, E> where E: std::error::Error {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            format_error(f, self.0)
        }
    }
    Disp(e)
}


/*
Adapted from https://github.com/oliver-giersch/closure

MIT License

Copyright (c) 2018 Oliver Giersch

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
*/

#[macro_export]
#[doc(hidden)]
macro_rules! _enclose {
    (@inner [move $($ids:ident).+ $(, $($tail:tt)*)?] $($closure:tt)*) => {
        let $crate::__extract_last_ident!($($ids).+) = $($ids).+;
        $crate::_enclose!(@inner [$($($tail)*)?] $($closure)*)
    };
    (@inner [move mut $($ids:ident).+ $(, $($tail:tt)*)?] $($closure:tt)*) => {
        let $crate::__extract_last_ident!(mut $($ids).+) = $($ids).+;
        $crate::_enclose!(@inner [$($($tail)*)?] $($closure)*)
    };
    (@inner [ref $($ids:ident).+ $(, $($tail:tt)*)?] $($closure:tt)*) => {
        let $crate::__extract_last_ident!($($ids).+) = & $($ids).+;
        $crate::_enclose!(@inner [$($($tail)*)?] $($closure)*)
    };
    (@inner [ref mut $($ids:ident).+ $(, $($tail:tt)*)?] $($closure:tt)*) => {
        let $crate::__extract_last_ident!($($ids).+) = &mut $($ids).+;
        $crate::_enclose!(@inner [$($($tail)*)?] $($closure)*)
    };
    (@inner [$fn:ident $($ids:ident).+ $(, $($tail:tt)*)?] $($closure:tt)*) => {
        let $crate::__extract_last_ident!($($ids).+) = $($ids).+.$fn();
        $crate::_enclose!(@inner [$($($tail)*)?] $($closure)*)
    };
    (@inner [$fn:ident mut $($ids:ident).+ $(, $($tail:tt)*)?] $($closure:tt)*) => {
        let $crate::__extract_last_ident!(mut $($ids).+) = $($ids).+.$fn();
        $crate::_enclose!(@inner [$($($tail)*)?] $($closure)*)
    };
    (@inner [] $($closure:tt)*) => {
        $crate::__assert_move_closure!($($closure)*);
        $($closure)*
    };
    // macro entry point (accepts anything)
    ([$($args:tt)*] $($closure:tt)*) => {
        { $crate::_enclose! { @inner [$($args)*] $($closure)* } }
    };
}

#[doc(inline)]
pub use crate::_enclose as enclose;


#[macro_export]
#[doc(hidden)]
macro_rules! __extract_last_ident {
    ($last:ident) => { $last };
    (mut $last:ident) => { mut $last };
    ($ignore:ident.$($tail:ident).+) => { $crate::__extract_last_ident!($($tail).+) };
    (mut $ignore:ident.$($tail:ident).+) => { $crate::__extract_last_ident!(mut $($tail).+) };
}


#[macro_export]
#[doc(hidden)]
macro_rules! __assert_move_closure {
    (async move $($tt:tt)*) => { };
    (async $($tt:tt)*) => { ::core::compile_error!("async block must be `move`") };
    (move $($tt:tt)*) => { };
    (|$($tt:tt)*) => { ::core::compile_error!("closure must be `move`") };
    (||$($tt:tt)*) => { ::core::compile_error!("closure must be `move`") };
}
