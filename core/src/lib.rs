//! Storage data shared by every Bhandara front end: mounted volumes, live
//! disk throughput, the file index (Suchi), eject, and drive health.

mod eject;
pub mod health;
pub mod index;
mod throughput;
mod volumes;

pub use eject::eject;
pub use throughput::{Rates, Throughput};
pub use volumes::{Kind, Level, Volume, volumes};
