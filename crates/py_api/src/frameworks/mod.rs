// SPDX-FileCopyrightText: 2025 Russ Fellows <russ.fellows@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Framework integration module for dl-driver Python API
//!
//! This module provides Python framework integrations that combine s3dlio's
//! async Rust backend with dl-driver's DLIO configuration and multi-backend support.

// Note: The actual Python integration files are in the src/ directory
// as .py files since they implement Python classes and functions.
// This mod.rs file allows the Rust crate to reference the Python module.

// Re-export main types for Rust code that needs to interact with the Python layer
// (Future: When we add Rust-side validation or helpers)
