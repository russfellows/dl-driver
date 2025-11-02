// SPDX-FileCopyrightText: 2025 Russ Fellows <russ.fellows@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Directory tree structure generation for hierarchical dataset organization.
//!
//! Provides three modes of directory organization:
//! 1. **Flat** (Mode 1): All files in single directory (current DLIO/dl-driver default)
//! 2. **DLIO-style sharding** (Mode 2): Flat subdirectory sharding via `num_subfolders_train`
//! 3. **Hierarchical** (Mode 3): Multi-level nested directories (sai3-bench style)
//!
//! Works transparently with both filesystems (file://, direct://) and object stores (s3://, az://, gs://).
//! For object stores, directories are logical (implicit in object keys), no mkdir needed.

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuration for hierarchical directory structure generation (Mode 3)
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct DirectoryStructureConfig {
    /// Number of subdirectories per directory level
    pub width: usize,
    
    /// Number of levels in the directory tree (1 = flat, 2+ = nested)
    pub depth: usize,
    
    /// Number of files to create in each directory
    #[serde(default)]
    pub files_per_dir: usize,
    
    /// File distribution strategy: "bottom" (only at deepest level) or "all" (at every level)
    #[serde(default = "default_distribution")]
    pub distribution: String,
    
    /// Directory naming pattern with two %d placeholders for depth and width
    /// Example: "dldriver.d%d_w%d.dir" produces:
    ///   - "dldriver.d1_w1.dir", "dldriver.d1_w2.dir" (level 1)
    ///   - "dldriver.d2_w1.dir", "dldriver.d2_w2.dir" (level 2, under each parent)
    #[serde(default = "default_dir_mask")]
    pub dir_mask: String,
}

fn default_distribution() -> String {
    "bottom".to_string()
}

fn default_dir_mask() -> String {
    "dldriver.d%d_w%d.dir".to_string()
}

/// A node in the directory tree representing a single directory
#[derive(Debug, Clone, PartialEq)]
pub struct DirectoryNode {
    /// Depth level (1-indexed): 1 = root level, 2 = second level, etc.
    pub depth: usize,
    
    /// Width index (1-indexed): 1 = first child, 2 = second child, etc.
    pub width: usize,
    
    /// Full path relative to anchor (includes all parent directories)
    pub full_path: String,
    
    /// Just this directory's name (without parents)
    pub dir_name: String,
    
    /// Whether this directory should contain files (based on distribution strategy)
    pub has_files: bool,
}

/// Directory tree structure with all paths pre-computed
#[derive(Debug, Clone, PartialEq)]
pub struct DirectoryTree {
    config: DirectoryStructureConfig,
    
    /// All directories in the tree, indexed by "depth_width" key
    directories: HashMap<String, DirectoryNode>,
    
    /// List of all directory paths for enumeration
    all_paths: Vec<String>,
    
    /// Directories by level (for level-specific operations)
    by_level: HashMap<usize, Vec<String>>,
    
    /// Total number of directories
    total_directories: usize,
    
    /// Total number of files
    total_files: usize,
}

impl DirectoryTree {
    /// Create a new directory tree from configuration
    pub fn new(config: DirectoryStructureConfig) -> Result<Self> {
        if config.width == 0 {
            bail!("Directory width must be at least 1");
        }
        if config.depth == 0 {
            bail!("Directory depth must be at least 1");
        }
        
        let mut tree = DirectoryTree {
            config: config.clone(),
            directories: HashMap::new(),
            all_paths: Vec::new(),
            by_level: HashMap::new(),
            total_directories: 0,
            total_files: 0,
        };
        
        tree.generate()?;
        Ok(tree)
    }
    
    /// Generate the complete directory tree structure
    fn generate(&mut self) -> Result<()> {
        // Calculate total directories: width^1 + width^2 + ... + width^depth
        self.total_directories = 0;
        for level in 1..=self.config.depth {
            let dirs_at_level = self.config.width.pow(level as u32);
            self.total_directories += dirs_at_level;
        }
        
        // Generate all directory nodes
        for level in 1..=self.config.depth {
            let dirs_at_level = self.config.width.pow(level as u32);
            let mut level_paths = Vec::new();
            
            for width_idx in 1..=dirs_at_level {
                let node = self.create_node(level, width_idx)?;
                let key = format!("{}_{}", level, width_idx);
                level_paths.push(node.full_path.clone());
                self.all_paths.push(node.full_path.clone());
                self.directories.insert(key, node);
            }
            
            self.by_level.insert(level, level_paths);
        }
        
        // Calculate total files based on distribution strategy
        self.total_files = match self.config.distribution.as_str() {
            "bottom" => {
                // Files only at deepest level
                let leaf_dirs = self.config.width.pow(self.config.depth as u32);
                leaf_dirs * self.config.files_per_dir
            }
            "all" => {
                // Files at all levels
                self.total_directories * self.config.files_per_dir
            }
            _ => bail!("Invalid distribution strategy: '{}'. Must be 'bottom' or 'all'", self.config.distribution),
        };
        
        Ok(())
    }
    
    /// Create a single directory node
    fn create_node(&self, depth: usize, global_width_idx: usize) -> Result<DirectoryNode> {
        // Calculate local width relative to parent (resets for each parent)
        let local_width = ((global_width_idx - 1) % self.config.width) + 1;
        
        // Generate directory name using mask with LOCAL width
        let dir_name = {
            let temp = self.config.dir_mask.replacen("%d", &format!("{}", depth), 1);
            temp.replacen("%d", &format!("{}", local_width), 1)
        };
        
        // Build full path by traversing parent hierarchy
        let full_path = if depth == 1 {
            // Root level - no parents
            dir_name.clone()
        } else {
            // Calculate parent's global index
            let parent_global_idx = ((global_width_idx - 1) / self.config.width) + 1;
            let parent_key = format!("{}_{}", depth - 1, parent_global_idx);
            
            if let Some(parent) = self.directories.get(&parent_key) {
                format!("{}/{}", parent.full_path, dir_name)
            } else {
                // Parent not generated yet - build path recursively
                let parent_node = self.create_node(depth - 1, parent_global_idx)?;
                format!("{}/{}", parent_node.full_path, dir_name)
            }
        };
        
        // Determine if this directory should have files
        let has_files = match self.config.distribution.as_str() {
            "bottom" => depth == self.config.depth,
            "all" => true,
            _ => false,
        };
        
        Ok(DirectoryNode {
            depth,
            width: local_width,
            full_path,
            dir_name,
            has_files,
        })
    }
    
    /// Get all directory paths
    pub fn all_paths(&self) -> &[String] {
        &self.all_paths
    }
    
    /// Get directory paths at a specific level
    pub fn paths_at_level(&self, level: usize) -> Option<&[String]> {
        self.by_level.get(&level).map(|v| v.as_slice())
    }
    
    /// Get total number of directories
    pub fn total_directories(&self) -> usize {
        self.total_directories
    }
    
    /// Get total number of files
    pub fn total_files(&self) -> usize {
        self.total_files
    }
    
    /// Get configuration
    pub fn config(&self) -> &DirectoryStructureConfig {
        &self.config
    }
}

/// Serializable tree manifest for coordination between agents
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeManifest {
    /// All directory paths in the tree (relative to anchor)
    pub all_directories: Vec<String>,
    
    /// Directories grouped by depth level (1-indexed)
    pub by_level: HashMap<usize, Vec<String>>,
    
    /// For distributed execution: which agent owns which directories
    #[serde(default)]
    pub agent_assignments: HashMap<usize, Vec<String>>,
    
    /// Configuration hash for validation
    pub config_hash: String,
    
    /// Total directory count
    pub total_dirs: usize,
    
    /// Total file count
    pub total_files: usize,
    
    /// Files per directory
    pub files_per_dir: usize,
    
    /// Distribution strategy
    pub distribution: String,
    
    /// File index ranges per directory: Vec<(dir_path, (start_idx, end_idx))>
    #[serde(default)]
    pub file_ranges: Vec<(String, (usize, usize))>,
}

impl TreeManifest {
    /// Create manifest from a DirectoryTree
    pub fn from_tree(tree: &DirectoryTree) -> Self {
        let config_hash = Self::hash_config(&tree.config);
        
        let mut manifest = TreeManifest {
            all_directories: tree.all_paths.clone(),
            by_level: tree.by_level.clone(),
            agent_assignments: HashMap::new(),
            config_hash,
            total_dirs: tree.total_directories,
            total_files: tree.total_files,
            files_per_dir: tree.config.files_per_dir,
            distribution: tree.config.distribution.clone(),
            file_ranges: Vec::new(),
        };
        
        manifest.compute_file_ranges();
        manifest
    }
    
    /// Compute file distribution across directories
    pub fn compute_file_ranges(&mut self) {
        self.file_ranges.clear();
        let mut global_idx = 0;
        
        // Determine max depth for "bottom" distribution
        let max_depth = if let Some(max_level) = self.by_level.keys().max() {
            *max_level
        } else {
            return;
        };
        
        for dir_path in &self.all_directories {
            let should_have_files = match self.distribution.as_str() {
                "bottom" => {
                    let depth = dir_path.matches('/').count() + 1;
                    depth == max_depth
                }
                "all" => true,
                _ => false,
            };
            
            if should_have_files && self.files_per_dir > 0 {
                let start_idx = global_idx;
                let end_idx = global_idx + self.files_per_dir;
                self.file_ranges.push((dir_path.clone(), (start_idx, end_idx)));
                global_idx = end_idx;
            }
        }
        
        self.total_files = global_idx;
    }
    
    /// Get file name for a global file index
    pub fn get_file_name(&self, global_idx: usize) -> String {
        format!("train_file_{:08}.dat", global_idx)
    }
    
    /// Get file range for a specific directory
    pub fn get_file_range(&self, dir_path: &str) -> Option<&(usize, usize)> {
        self.file_ranges
            .iter()
            .find(|(path, _)| path == dir_path)
            .map(|(_, range)| range)
    }
    
    /// Get full relative path for a global file index
    pub fn get_file_path(&self, global_idx: usize) -> Option<String> {
        for (dir_path, (start, end)) in &self.file_ranges {
            if global_idx >= *start && global_idx < *end {
                let file_name = self.get_file_name(global_idx);
                return Some(format!("{}/{}", dir_path, file_name));
            }
        }
        None
    }
    
    /// Get list of all files in a specific directory
    pub fn get_files_in_directory(&self, dir_path: &str) -> Vec<String> {
        if let Some((start, end)) = self.get_file_range(dir_path) {
            (*start..*end)
                .map(|idx| self.get_file_name(idx))
                .collect()
        } else {
            Vec::new()
        }
    }
    
    /// Assign directories to agents for distributed execution
    pub fn assign_agents(&mut self, num_agents: usize) {
        if num_agents == 0 {
            return;
        }
        
        self.agent_assignments.clear();
        
        for (idx, path) in self.all_directories.iter().enumerate() {
            let agent_id = idx % num_agents;
            self.agent_assignments
                .entry(agent_id)
                .or_insert_with(Vec::new)
                .push(path.clone());
        }
    }
    
    /// Get directories assigned to a specific agent
    pub fn get_agent_dirs(&self, agent_id: usize) -> Vec<String> {
        self.agent_assignments
            .get(&agent_id)
            .cloned()
            .unwrap_or_default()
    }
    
    /// Compute deterministic hash of config for validation
    fn hash_config(config: &DirectoryStructureConfig) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        config.width.hash(&mut hasher);
        config.depth.hash(&mut hasher);
        config.files_per_dir.hash(&mut hasher);
        config.distribution.hash(&mut hasher);
        config.dir_mask.hash(&mut hasher);
        
        format!("{:x}", hasher.finish())
    }
}

/// Directory organization mode for dl-driver datasets
#[derive(Debug, Clone, PartialEq)]
pub enum DirectoryMode {
    /// Mode 1: All files in single flat directory (current DLIO/dl-driver default)
    Flat,
    
    /// Mode 2: DLIO-style flat sharding across subdirectories
    DlioSharding { num_subfolders: usize },
    
    /// Mode 3: Hierarchical nested directory tree (sai3-bench style)
    Hierarchical { tree: DirectoryTree },
}

impl DirectoryMode {
    /// Determine directory mode from DLIO config
    pub fn from_config(
        directory_tree: Option<&DirectoryStructureConfig>,
        num_subfolders_train: Option<usize>,
    ) -> Result<Self> {
        if let Some(tree_config) = directory_tree {
            // Mode 3: Hierarchical
            let tree = DirectoryTree::new(tree_config.clone())?;
            Ok(DirectoryMode::Hierarchical { tree })
        } else if let Some(num_subfolders) = num_subfolders_train {
            if num_subfolders > 1 {
                // Mode 2: DLIO sharding
                Ok(DirectoryMode::DlioSharding { num_subfolders })
            } else {
                // Mode 1: Flat (num_subfolders=1 or 0)
                Ok(DirectoryMode::Flat)
            }
        } else {
            // Mode 1: Flat (nothing specified)
            Ok(DirectoryMode::Flat)
        }
    }
    
    /// Get total number of files based on mode
    pub fn total_files(&self, num_files_train: usize) -> usize {
        match self {
            DirectoryMode::Flat => num_files_train,
            DirectoryMode::DlioSharding { .. } => num_files_train,
            DirectoryMode::Hierarchical { tree } => tree.total_files(),
        }
    }
    
    /// Get file path for a given file index
    /// Returns relative path from data_folder root
    pub fn get_file_path(&self, file_idx: usize, format_ext: &str) -> String {
        match self {
            DirectoryMode::Flat => {
                // Mode 1: train_file_000000.npz
                format!("train_file_{:08}.{}", file_idx, format_ext)
            }
            DirectoryMode::DlioSharding { num_subfolders } => {
                // Mode 2: train/0042/train_file_000123.npz
                let subfolder = file_idx % num_subfolders;
                format!("train/{:04}/train_file_{:08}.{}", subfolder, file_idx, format_ext)
            }
            DirectoryMode::Hierarchical { tree } => {
                // Mode 3: dldriver.d1_w2.dir/dldriver.d2_w5.dir/train_file_00000123.dat
                let manifest = TreeManifest::from_tree(tree);
                manifest.get_file_path(file_idx)
                    .unwrap_or_else(|| format!("train_file_{:08}.{}", file_idx, format_ext))
            }
        }
    }
    
    /// Get list of all directories that need to be created
    /// Returns empty vec for object stores (directories are implicit)
    pub fn get_directories_to_create(&self, backend_uri: &str) -> Vec<String> {
        // Only create directories for file:// and direct:// backends
        let needs_mkdir = backend_uri.starts_with("file://") || backend_uri.starts_with("direct://");
        
        if !needs_mkdir {
            return Vec::new(); // Object stores (s3://, az://, gs://) don't need mkdir
        }
        
        match self {
            DirectoryMode::Flat => {
                // Mode 1: Just the train directory
                vec!["train".to_string()]
            }
            DirectoryMode::DlioSharding { num_subfolders } => {
                // Mode 2: train/0000, train/0001, ..., train/NNNN
                let mut dirs = vec!["train".to_string()];
                for i in 0..*num_subfolders {
                    dirs.push(format!("train/{:04}", i));
                }
                dirs
            }
            DirectoryMode::Hierarchical { tree } => {
                // Mode 3: All paths from tree
                tree.all_paths().to_vec()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_flat_mode() {
        let mode = DirectoryMode::from_config(None, None).unwrap();
        assert_eq!(mode, DirectoryMode::Flat);
        
        let path = mode.get_file_path(42, "npz");
        assert_eq!(path, "train_file_00000042.npz");
        
        let dirs = mode.get_directories_to_create("file:///data");
        assert_eq!(dirs, vec!["train"]);
    }
    
    #[test]
    fn test_dlio_sharding_mode() {
        let mode = DirectoryMode::from_config(None, Some(16)).unwrap();
        
        if let DirectoryMode::DlioSharding { num_subfolders } = mode {
            assert_eq!(num_subfolders, 16);
            
            let path = mode.get_file_path(42, "npz");
            assert_eq!(path, "train/0010/train_file_00000042.npz"); // 42 % 16 = 10
            
            let dirs = mode.get_directories_to_create("file:///data");
            assert_eq!(dirs.len(), 17); // train + 16 subfolders
            assert!(dirs.contains(&"train/0000".to_string()));
            assert!(dirs.contains(&"train/0015".to_string()));
        } else {
            panic!("Expected DlioSharding mode");
        }
    }
    
    #[test]
    fn test_hierarchical_mode() {
        let tree_config = DirectoryStructureConfig {
            width: 2,
            depth: 2,
            files_per_dir: 10,
            distribution: "bottom".to_string(),
            dir_mask: "test.d%d_w%d.dir".to_string(),
        };
        
        let mode = DirectoryMode::from_config(Some(&tree_config), None).unwrap();
        
        if let DirectoryMode::Hierarchical { tree } = &mode {
            assert_eq!(tree.total_directories(), 6); // 2 + 4
            assert_eq!(tree.total_files(), 40); // 4 leaf dirs * 10 files
            
            let path = mode.get_file_path(0, "npz");
            assert!(path.contains("test.d"));
            
            let dirs = mode.get_directories_to_create("file:///data");
            assert_eq!(dirs.len(), 6);
        } else {
            panic!("Expected Hierarchical mode");
        }
    }
    
    #[test]
    fn test_object_store_no_mkdir() {
        let mode = DirectoryMode::from_config(None, Some(16)).unwrap();
        
        // Object stores should return empty list (no mkdir needed)
        assert_eq!(mode.get_directories_to_create("s3://bucket"), Vec::<String>::new());
        assert_eq!(mode.get_directories_to_create("az://container"), Vec::<String>::new());
        assert_eq!(mode.get_directories_to_create("gs://bucket"), Vec::<String>::new());
        
        // Filesystems should return directories
        assert!(!mode.get_directories_to_create("file:///data").is_empty());
        assert!(!mode.get_directories_to_create("direct:///dev/sda1").is_empty());
    }
}
