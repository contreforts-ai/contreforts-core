pub mod client;
pub mod config;
pub mod error;
pub mod models;
pub mod sync;

pub use client::ErpNextClient;
pub use config::Config;
pub use error::Error;

pub type Result<T> = std::result::Result<T, Error>;
