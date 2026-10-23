// Index loops read better than iterator chains in the numeric code.
#![allow(clippy::needless_range_loop)]

pub mod bots;
pub mod gui;
pub mod nn;
pub mod rng;
pub mod sim;
pub mod train;
