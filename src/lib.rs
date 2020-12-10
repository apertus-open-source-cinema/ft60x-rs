#![feature(get_mut_unchecked)]

use bitflags::_core::str::Utf8Error;
use std::io;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Error in USB communication")]
    RUSBError(#[from] rusb::Error),
    #[error("IO Error")]
    IOError(#[from] io::Error),
    #[error("UTF8 Error")]
    Utf8Error(#[from] Utf8Error),
    #[error("{0}")]
    GeneralError(String),
}

macro_rules! format_general_err {
    ($($arg:tt)*) => { $crate::Error::GeneralError(format!($($arg)*)) }
}

macro_rules! ensure {
    ($cond:expr) => {
        if !($cond) {
            return Err(format_general_err!("something went wrong"));
        }
    };
    ($cond:expr, $e:expr) => {
        if !($cond) {
            return Err(format_general_err!($e));
        }
    };
    ($cond:expr, $fmt:expr, $($arg:tt)*) => {
        if !($cond) {
            return Err(format_general_err!($fmt, $($arg)*));
        }
    };
}

type Result<T> = std::result::Result<T, Error>;

pub mod ft60x;
pub mod ft60x_config;
pub mod ringbuf;
