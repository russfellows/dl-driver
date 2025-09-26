# SPDX-FileCopyrightText: 2025 Russ Fellows <russ.fellows@gmail.com>
# SPDX-License-Identifier: GPL-3.0-or-later

"""
dl-driver PyTorch Integration

Provides PyTorch DataLoader integration with dl-driver's:
- DLIO configuration support
- Multi-backend capabilities (file://, s3://, az://, direct://)
- Format-aware loading (NPZ, HDF5, TFRecord)
- Workload metrics and performance tracking
- Enterprise-grade features while leveraging s3dlio's mature async backend

This module extends s3dlio's PyTorch classes with dl-driver's configuration
and multi-format capabilities for seamless MLCommons DLIO integration.
"""

from __future__ import annotations

import os
import yaml
from pathlib import Path
from typing import Dict, Any, Optional, Union, Iterator, Tuple
from urllib.parse import urlparse

import torch
from torch.utils.data import IterableDataset, DataLoader

# Import s3dlio PyTorch classes
try:
    import s3dlio
    from s3dlio.torch import S3IterableDataset, S3MapDataset
    HAVE_S3DLIO = True
except ImportError:
    HAVE_S3DLIO = False
    S3IterableDataset = None
    S3MapDataset = None


class DlioDataLoaderError(Exception):
    """Exception raised by dl-driver PyTorch integration."""
    pass


class DlioPyTorchDataset(IterableDataset):
    """
    dl-driver PyTorch Dataset that wraps s3dlio with DLIO configuration support.
    
    Features:
    - DLIO YAML configuration parsing
    - Multi-backend URI support (file://, s3://, az://, direct://)
    - Format detection and handling (NPZ, HDF5, TFRecord)
    - Framework-specific configuration profiles
    - Seamless s3dlio integration with async Rust backend
    """
    
    def __init__(
        self,
        config_path: Optional[str] = None,
        config_dict: Optional[Dict[str, Any]] = None,
        data_folder: Optional[str] = None,
        pytorch_config: Optional[Dict[str, Any]] = None,
        **kwargs
    ):
        """
        Initialize dl-driver PyTorch Dataset.
        
        Args:
            config_path: Path to DLIO YAML configuration file
            config_dict: DLIO configuration as dictionary
            data_folder: Override data_folder from config (URI with scheme)
            pytorch_config: PyTorch-specific configuration override
            **kwargs: Additional s3dlio options (prefetch, num_workers, etc.)
        """
        super().__init__()
        
        if not HAVE_S3DLIO:
            raise DlioDataLoaderError(
                "s3dlio package is required for PyTorch integration. "
                "Install with: pip install s3dlio"
            )
        
        # Parse configuration
        self.config = self._parse_config(config_path, config_dict)
        
        # Override data folder if provided
        if data_folder:
            self.config['data_folder'] = data_folder
        
        # Extract data folder and validate (check both top-level and DLIO structure)
        self.data_folder = self.config.get('data_folder')
        if not self.data_folder and 'dataset' in self.config:
            self.data_folder = self.config['dataset'].get('data_folder')
        if not self.data_folder:
            raise DlioDataLoaderError("data_folder must be specified in config or as parameter")
        
        # Parse URI scheme for backend detection
        self.backend_type = self._detect_backend(self.data_folder)
        
        # Get PyTorch-specific configuration
        self.pytorch_config = self._get_pytorch_config(pytorch_config)
        
        # Get format configuration
        self.format_type = self._detect_format()
        
        # Build s3dlio options
        self.s3dlio_options = self._build_s3dlio_options(**kwargs)
        
        # Initialize underlying s3dlio dataset
        self._s3dlio_dataset = None
        self._initialize_s3dlio_dataset()
    
    def _parse_config(
        self, 
        config_path: Optional[str], 
        config_dict: Optional[Dict[str, Any]]
    ) -> Dict[str, Any]:
        """Parse DLIO configuration from file or dictionary."""
        if config_path:
            if not os.path.exists(config_path):
                raise DlioDataLoaderError(f"Configuration file not found: {config_path}")
            
            with open(config_path, 'r') as f:
                if config_path.endswith('.yaml') or config_path.endswith('.yml'):
                    config = yaml.safe_load(f)
                else:
                    raise DlioDataLoaderError(f"Unsupported config format: {config_path}")
        elif config_dict:
            config = config_dict.copy()
        else:
            raise DlioDataLoaderError("Either config_path or config_dict must be provided")
        
        return config
    
    def _detect_backend(self, data_folder: str) -> str:
        """Detect storage backend from URI scheme."""
        parsed = urlparse(data_folder)
        scheme = parsed.scheme.lower()
        
        if scheme in ['s3', 's3a']:
            return 's3'
        elif scheme in ['az', 'azure', 'abfs']:
            return 'azure'  
        elif scheme == 'direct':
            return 'directio'
        elif scheme == 'file' or not scheme:
            return 'file'
        else:
            raise DlioDataLoaderError(f"Unsupported URI scheme: {scheme}")
    
    def _detect_format(self) -> str:
        """Detect data format from configuration or file extensions."""
        # Check explicit format in config
        format_type = self.config.get('format', '').lower()
        if format_type in ['npz', 'hdf5', 'tfrecord']:
            return format_type
        
        # Try to detect from file format configuration
        file_format = self.config.get('file_format', '').lower()
        if file_format in ['npz', 'hdf5', 'tfrecord']:
            return file_format
        
        # Default to NPZ (most common for ML workloads)
        return 'npz'
    
    def _get_pytorch_config(self, pytorch_config: Optional[Dict[str, Any]]) -> Dict[str, Any]:
        """Extract and merge PyTorch-specific configuration."""
        # Start with defaults
        config = {
            'batch_size': 32,
            'num_workers': 4,
            'shuffle': True,
            'seed': 42,
            'prefetch_factor': 2,
            'return_type': 'tensor',  # tensor, bytes, or reader
        }
        
        # Update from DLIO config framework section
        if 'framework' in self.config:
            framework_config = self.config['framework']
            if 'pytorch' in framework_config:
                config.update(framework_config['pytorch'])
        
        # Update from direct pytorch_config in DLIO
        if 'pytorch_config' in self.config:
            config.update(self.config['pytorch_config'])
        
        # Override with provided pytorch_config
        if pytorch_config:
            config.update(pytorch_config)
        
        return config
    
    def _build_s3dlio_options(self, **kwargs) -> Dict[str, Any]:
        """Build s3dlio LoaderOptions from dl-driver configuration."""
        options = {}
        
        # Map dl-driver config to s3dlio options
        if 'batch_size' in self.pytorch_config:
            options['batch_size'] = self.pytorch_config['batch_size']
        
        if 'shuffle' in self.pytorch_config:
            options['shuffle'] = self.pytorch_config['shuffle']
        
        if 'seed' in self.pytorch_config:
            options['seed'] = self.pytorch_config['seed']
        
        # Map DLIO configuration
        if 'num_readers' in self.config:
            options['num_workers'] = self.config['num_readers']
        elif 'num_workers' in self.pytorch_config:
            options['num_workers'] = self.pytorch_config['num_workers']
        
        if 'prefetch_buffer' in self.config:
            options['prefetch'] = self.config['prefetch_buffer']
        elif 'prefetch_factor' in self.pytorch_config:
            options['prefetch'] = self.pytorch_config['prefetch_factor']
        
        # Override with direct kwargs
        options.update(kwargs)
        
        return options
    
    def _initialize_s3dlio_dataset(self):
        """Initialize the underlying s3dlio dataset."""
        try:
            # Configure return type
            return_type = self.pytorch_config.get('return_type', 'tensor')
            
            # Create s3dlio dataset based on backend
            if self.backend_type in ['s3', 'azure', 'file', 'directio']:
                # Use IterableDataset for streaming
                self._s3dlio_dataset = S3IterableDataset.from_prefix(
                    uri=self.data_folder,
                    return_type=return_type,
                    **self.s3dlio_options
                )
            else:
                raise DlioDataLoaderError(f"Backend not supported: {self.backend_type}")
                
        except Exception as e:
            raise DlioDataLoaderError(f"Failed to initialize s3dlio dataset: {e}")
    
    def __iter__(self) -> Iterator[Any]:
        """Iterate over dataset samples."""
        if not self._s3dlio_dataset:
            raise DlioDataLoaderError("Dataset not initialized")
        
        try:
            for item in self._s3dlio_dataset:
                # Post-process based on format type
                yield self._process_sample(item)
        except Exception as e:
            raise DlioDataLoaderError(f"Error during iteration: {e}")
    
    def _process_sample(self, item: Any) -> Any:
        """Process sample based on format type and return requirements."""
        # For now, pass through s3dlio processing
        # Future: Add format-specific parsing (NPZ, HDF5, TFRecord)
        return item
    
    @property
    def config_info(self) -> Dict[str, Any]:
        """Return configuration information for debugging."""
        return {
            'data_folder': self.data_folder,
            'backend_type': self.backend_type,
            'format_type': self.format_type,
            'pytorch_config': self.pytorch_config,
            's3dlio_options': self.s3dlio_options,
        }


class DlioPyTorchDataLoader:
    """
    High-level PyTorch DataLoader factory for dl-driver workflows.
    
    Provides a simple interface for creating PyTorch DataLoaders from DLIO
    configurations while leveraging s3dlio's performance optimizations.
    """
    
    @classmethod
    def from_config(
        cls,
        config_path: str,
        pytorch_config: Optional[Dict[str, Any]] = None,
        dataloader_kwargs: Optional[Dict[str, Any]] = None
    ) -> DataLoader:
        """
        Create PyTorch DataLoader from DLIO configuration file.
        
        Args:
            config_path: Path to DLIO YAML configuration
            pytorch_config: PyTorch-specific overrides
            dataloader_kwargs: Additional DataLoader arguments
            
        Returns:
            Configured PyTorch DataLoader
        """
        # Create dataset
        dataset = DlioPyTorchDataset(
            config_path=config_path,
            pytorch_config=pytorch_config
        )
        
        # Build DataLoader kwargs
        loader_kwargs = {
            'batch_size': dataset.pytorch_config.get('batch_size', 32),
            'num_workers': 0,  # Let s3dlio handle concurrency
            'pin_memory': dataset.pytorch_config.get('pin_memory', False),
            'drop_last': dataset.pytorch_config.get('drop_last', False),
        }
        
        if dataloader_kwargs:
            loader_kwargs.update(dataloader_kwargs)
        
        return DataLoader(dataset, **loader_kwargs)
    
    @classmethod
    def from_uri(
        cls,
        data_folder: str,
        batch_size: int = 32,
        **kwargs
    ) -> DataLoader:
        """
        Create PyTorch DataLoader directly from data folder URI.
        
        Args:
            data_folder: Data folder URI (s3://, file://, etc.)
            batch_size: Batch size for DataLoader
            **kwargs: Additional configuration options
            
        Returns:
            Configured PyTorch DataLoader
        """
        config_dict = {
            'data_folder': data_folder,
            'pytorch_config': {
                'batch_size': batch_size,
                **kwargs
            }
        }
        
        dataset = DlioPyTorchDataset(config_dict=config_dict)
        
        return DataLoader(
            dataset,
            batch_size=batch_size,
            num_workers=0,  # s3dlio handles concurrency
            **kwargs
        )


# Convenience functions for common usage patterns
def create_pytorch_dataloader(
    config_path: str,
    **kwargs
) -> DataLoader:
    """
    Convenience function to create PyTorch DataLoader from DLIO config.
    
    Args:
        config_path: Path to DLIO YAML configuration
        **kwargs: Additional configuration overrides
        
    Returns:
        Configured PyTorch DataLoader
    """
    return DlioPyTorchDataLoader.from_config(config_path, **kwargs)


def create_pytorch_dataset(
    config_path: str,
    **kwargs
) -> DlioPyTorchDataset:
    """
    Convenience function to create dl-driver PyTorch Dataset.
    
    Args:
        config_path: Path to DLIO YAML configuration  
        **kwargs: Additional configuration overrides
        
    Returns:
        Configured dl-driver PyTorch Dataset
    """
    return DlioPyTorchDataset(config_path=config_path, **kwargs)