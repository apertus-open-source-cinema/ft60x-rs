#![feature(get_mut_unchecked)]

type Result<T> = std::result::Result<T, failure::Error>;

pub mod ft60x;
pub mod ft60x_config;
pub mod ringbuf;
