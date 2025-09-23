//! dl-driver Python API
//!
//! This crate provides Python bindings for dl-driver's framework integrations.
//! It combines s3dlio's mature async Rust backend with dl-driver's DLIO configuration
//! support and multi-backend capabilities.
//!
//! Features:
//! - PyTorch DataLoader integration via s3dlio
//! - TensorFlow tf.data.Dataset creation via s3dlio  
//! - JAX NumPy array streaming
//! - DLIO configuration file support
//! - Multi-backend URI handling (file://, s3://, az://, direct://)
//! - Format-aware loading (NPZ, HDF5, TFRecord)

pub mod frameworks;
