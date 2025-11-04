// SPDX-FileCopyrightText: 2025 Russ Fellows <russ.fellows@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later

// crates/core/src/plugins/mod.rs
use anyhow::Result;
use async_trait::async_trait;
use crate::dlio_compat::DlioConfig;

#[async_trait]
pub trait Plugin: Send + Sync + std::any::Any {
    async fn initialize(&mut self, _cfg: &DlioConfig) -> Result<()> { Ok(()) }
    async fn after_step(&mut self, _step: u32) -> Result<()> { Ok(()) }
    async fn after_epoch(&mut self, _epoch: u32) -> Result<()> { Ok(()) }
    async fn finalize(&mut self) -> Result<()> { Ok(()) }
    
    /// Support for downcasting to concrete plugin types
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
}

pub struct PluginManager {
    plugins: Vec<Box<dyn Plugin>>,
}

impl std::fmt::Debug for PluginManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PluginManager")
            .field("plugin_count", &self.plugins.len())
            .finish()
    }
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginManager {
    pub fn new() -> Self { 
        Self { plugins: Vec::new() } 
    }
    
    pub fn push(&mut self, p: Box<dyn Plugin>) { 
        self.plugins.push(p); 
    }

    pub async fn initialize(&mut self, cfg: &DlioConfig) -> Result<()> {
        for p in self.plugins.iter_mut() { 
            p.initialize(cfg).await?; 
        }
        Ok(())
    }
    
    pub async fn after_step(&mut self, step: u32) -> Result<()> {
        for p in self.plugins.iter_mut() { 
            p.after_step(step).await?; 
        }
        Ok(())
    }
    
    pub async fn after_epoch(&mut self, epoch: u32) -> Result<()> {
        for p in self.plugins.iter_mut() { 
            p.after_epoch(epoch).await?; 
        }
        Ok(())
    }
    
    pub async fn finalize(&mut self) -> Result<()> {
        for p in self.plugins.iter_mut() { 
            p.finalize().await?; 
        }
        Ok(())
    }
    
    /// Get mutable reference to CheckpointPlugin for state restoration
    pub fn get_checkpoint_plugin_mut(&mut self) -> Option<&mut CheckpointPlugin> {
        for plugin in self.plugins.iter_mut() {
            // Use as_any_mut trait method for downcasting
            if let Some(checkpoint_plugin) = plugin.as_any_mut().downcast_mut::<CheckpointPlugin>() {
                return Some(checkpoint_plugin);
            }
        }
        None
    }
}

// CheckpointPlugin implementation for M5
pub mod checkpoint;
pub use checkpoint::CheckpointPlugin;