// SPDX-FileCopyrightText: 2025 Russ Fellows <russ.fellows@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Profiles for realistic AI/ML framework workload patterns
//! 
//! This module provides pre-configured profiles that match the I/O characteristics
//! of popular ML frameworks like PyTorch, TensorFlow, and JAX. Each profile 
//! configures s3dlio LoaderOptions and PoolConfig with realistic defaults.

use s3dlio::data_loader::{LoaderOptions, PoolConfig};
use s3dlio::{ReaderMode, LoadingMode};
use std::time::Duration;

/// Pre-configured profile containing s3dlio options for realistic workloads
#[derive(Debug, Clone)]
pub struct ProfileConfig {
    pub loader: LoaderOptions,
    pub pool: PoolConfig,
    pub description: String,
}

/// Available framework profiles
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    TorchLike,
    TfLike,
    JaxLike,
    Custom,
}

impl std::str::FromStr for Profile {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "torch-like" | "pytorch" | "torch" => Ok(Profile::TorchLike),
            "tf-like" | "tensorflow" | "tf" => Ok(Profile::TfLike),
            "jax-like" | "jax" => Ok(Profile::JaxLike),
            "custom" => Ok(Profile::Custom),
            _ => Err(format!("Unknown profile: {}", s)),
        }
    }
}

impl std::fmt::Display for Profile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Profile::TorchLike => write!(f, "torch-like"),
            Profile::TfLike => write!(f, "tf-like"),
            Profile::JaxLike => write!(f, "jax-like"),
            Profile::Custom => write!(f, "custom"),
        }
    }
}

/// Create a PyTorch-like workload profile
/// 
/// PyTorch DataLoader characteristics:
/// - Moderate worker count for efficient parallelism
/// - Higher prefetch for smooth pipeline feeding
/// - Shuffled access for training randomization
/// - Balanced in-flight settings for memory efficiency
pub fn torch_like(batch_size: usize) -> ProfileConfig {
    let pool = PoolConfig {
        pool_size: 8,
        readahead_batches: 16,
        batch_timeout: Duration::from_secs(30),
        max_inflight: 128,
    };

    let loader = LoaderOptions {
        batch_size,
        prefetch: 16,
        shuffle: true,
        num_workers: 8,
        reader_mode: ReaderMode::Sequential, // Will update when Random mode is available
        loading_mode: LoadingMode::AsyncPool(pool.clone()),
        seed: 42,
        ..Default::default()
    };

    ProfileConfig {
        loader,
        pool,
        description: "PyTorch-like: 8 workers, 16 prefetch, shuffled access for training efficiency".to_string(),
    }
}

/// Create a TensorFlow-like workload profile
/// 
/// TensorFlow tf.data characteristics:
/// - Fewer workers but efficient data pipeline
/// - Sequential access patterns common
/// - Higher in-flight capacity for throughput
/// - Longer timeouts for stable transfers
pub fn tf_like(batch_size: usize) -> ProfileConfig {
    let pool = PoolConfig {
        pool_size: 4,
        readahead_batches: 8,
        batch_timeout: Duration::from_secs(60),
        max_inflight: 64,
    };

    let loader = LoaderOptions {
        batch_size,
        prefetch: 8,
        shuffle: false, // Often sequential in TF pipelines
        num_workers: 4,
        reader_mode: ReaderMode::Sequential,
        loading_mode: LoadingMode::AsyncPool(pool.clone()),
        seed: 42,
        ..Default::default()
    };

    ProfileConfig {
        loader,
        pool,
        description: "TensorFlow-like: 4 workers, 8 prefetch, sequential access for pipeline efficiency".to_string(),
    }
}

/// Create a JAX-like workload profile
/// 
/// JAX data loading characteristics:
/// - Fewer workers, focus on efficient transfers
/// - Sequential patterns for large-scale training
/// - High throughput for XLA efficiency
/// - Designed for large-scale distributed training
pub fn jax_like(batch_size: usize) -> ProfileConfig {
    let pool = PoolConfig {
        pool_size: 2,
        readahead_batches: 4,
        batch_timeout: Duration::from_secs(120),
        max_inflight: 32,
    };

    let loader = LoaderOptions {
        batch_size,
        prefetch: 4,
        shuffle: false, // Often sequential for large-scale training
        num_workers: 2,
        reader_mode: ReaderMode::Sequential,
        loading_mode: LoadingMode::AsyncPool(pool.clone()),
        seed: 42,
        ..Default::default()
    };

    ProfileConfig {
        loader,
        pool,
        description: "JAX-like: 2 workers, 4 prefetch, sequential access for large-scale training efficiency".to_string(),
    }
}

/// Get profile configuration by name
pub fn get_profile(profile: Profile, batch_size: usize) -> ProfileConfig {
    match profile {
        Profile::TorchLike => torch_like(batch_size),
        Profile::TfLike => tf_like(batch_size),
        Profile::JaxLike => jax_like(batch_size),
        Profile::Custom => {
            // For custom profiles, return torch-like as default
            // Users can override via YAML config or CLI flags
            torch_like(batch_size)
        }
    }
}

/// List all available profiles with descriptions
pub fn list_profiles() -> Vec<(Profile, String)> {
    vec![
        (Profile::TorchLike, torch_like(32).description),
        (Profile::TfLike, tf_like(32).description),
        (Profile::JaxLike, jax_like(32).description),
        (Profile::Custom, "Custom: User-defined configuration via YAML or CLI flags".to_string()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profile_from_str() {
        assert_eq!("torch-like".parse::<Profile>().unwrap(), Profile::TorchLike);
        assert_eq!("pytorch".parse::<Profile>().unwrap(), Profile::TorchLike);
        assert_eq!("tf-like".parse::<Profile>().unwrap(), Profile::TfLike);
        assert_eq!("tensorflow".parse::<Profile>().unwrap(), Profile::TfLike);
        assert_eq!("jax-like".parse::<Profile>().unwrap(), Profile::JaxLike);
        assert_eq!("jax".parse::<Profile>().unwrap(), Profile::JaxLike);
        assert_eq!("custom".parse::<Profile>().unwrap(), Profile::Custom);
        
        assert!("invalid".parse::<Profile>().is_err());
    }

    #[test]
    fn test_torch_like_profile() {
        let config = torch_like(32);
        assert_eq!(config.loader.batch_size, 32);
        assert_eq!(config.loader.num_workers, 8);
        assert_eq!(config.loader.prefetch, 16);
        assert_eq!(config.loader.reader_mode, ReaderMode::Sequential);
        assert!(config.loader.shuffle);
        
        assert_eq!(config.pool.pool_size, 8);
        assert_eq!(config.pool.readahead_batches, 16);
    }

    #[test]
    fn test_tf_like_profile() {
        let config = tf_like(64);
        assert_eq!(config.loader.batch_size, 64);
        assert_eq!(config.loader.num_workers, 4);
        assert_eq!(config.loader.prefetch, 8);
        assert_eq!(config.loader.reader_mode, ReaderMode::Sequential);
        assert!(!config.loader.shuffle);
        
        assert_eq!(config.pool.pool_size, 4);
        assert_eq!(config.pool.readahead_batches, 8);
    }

    #[test]
    fn test_jax_like_profile() {
        let config = jax_like(128);
        assert_eq!(config.loader.batch_size, 128);
        assert_eq!(config.loader.num_workers, 2);
        assert_eq!(config.loader.prefetch, 4);
        assert_eq!(config.loader.reader_mode, ReaderMode::Sequential);
        assert!(!config.loader.shuffle);
        
        assert_eq!(config.pool.pool_size, 2);
        assert_eq!(config.pool.readahead_batches, 4);
    }

    #[test]
    fn test_get_profile() {
        let torch_config = get_profile(Profile::TorchLike, 16);
        assert_eq!(torch_config.loader.batch_size, 16);
        assert_eq!(torch_config.loader.num_workers, 8);
        
        let tf_config = get_profile(Profile::TfLike, 32);
        assert_eq!(tf_config.loader.batch_size, 32);
        assert_eq!(tf_config.loader.num_workers, 4);
    }

    #[test]
    fn test_list_profiles() {
        let profiles = list_profiles();
        assert_eq!(profiles.len(), 4);
        assert!(profiles.iter().any(|(p, _)| *p == Profile::TorchLike));
        assert!(profiles.iter().any(|(p, _)| *p == Profile::TfLike));
        assert!(profiles.iter().any(|(p, _)| *p == Profile::JaxLike));
        assert!(profiles.iter().any(|(p, _)| *p == Profile::Custom));
    }
}