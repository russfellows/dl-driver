# SPDX-FileCopyrightText: 2025 Russ Fellows <russ.fellows@gmail.com>
# SPDX-License-Identifier: GPL-3.0-or-later

"""
dl-driver Framework Integrations

This module provides high-level framework integrations for dl-driver that combine:
- s3dlio's mature async Rust backend with PyTorch and TensorFlow Python wrappers
- dl-driver's DLIO configuration support and multi-backend capabilities
- Enterprise-grade features and MLCommons DLIO compatibility

Supported Frameworks:
- PyTorch: Native DataLoader integration with s3dlio backend
- TensorFlow: tf.data.Dataset creation with s3dlio streaming
- JAX: NumPy array iteration with JAX-compatible output

Usage:
    # PyTorch integration
    from dl_driver.frameworks.pytorch import create_pytorch_dataloader
    loader = create_pytorch_dataloader("config.yaml")
    
    # TensorFlow integration  
    from dl_driver.frameworks.tensorflow import create_tensorflow_dataset
    dataset = create_tensorflow_dataset("config.yaml")
    
    # Direct URI usage
    from dl_driver.frameworks import create_dataloader
    loader = create_dataloader("s3://bucket/data/", framework="pytorch")
"""

from typing import Optional, Dict, Any, Union

try:
    from .pytorch import (
        DlioPyTorchDataset,
        DlioPyTorchDataLoader, 
        create_pytorch_dataloader,
        create_pytorch_dataset
    )
    HAVE_PYTORCH = True
except ImportError:
    HAVE_PYTORCH = False

try:
    from .tensorflow import (
        DlioTensorFlowDataset,
        DlioJaxDataset,
        create_tensorflow_dataset,
        create_jax_iterable,
        create_tensorflow_dataset_from_uri
    )
    HAVE_TENSORFLOW = True
except ImportError:
    HAVE_TENSORFLOW = False


class FrameworkError(Exception):
    """Exception raised by framework integration."""
    pass


def create_dataloader(
    data_source: str,
    framework: str = "pytorch",
    config_dict: Optional[Dict[str, Any]] = None,
    **kwargs
) -> Union['torch.utils.data.DataLoader', 'tf.data.Dataset', Any]:
    """
    Universal dataloader factory for different ML frameworks.
    
    Args:
        data_source: Either path to DLIO config file or data folder URI
        framework: Target framework ("pytorch", "tensorflow", "jax")
        config_dict: Optional configuration dictionary override
        **kwargs: Framework-specific configuration options
        
    Returns:
        Framework-specific dataloader object
        
    Examples:
        # PyTorch from config file
        loader = create_dataloader("config.yaml", framework="pytorch")
        
        # TensorFlow from URI
        dataset = create_dataloader("s3://bucket/data/", framework="tensorflow", batch_size=64)
        
        # JAX from config with overrides
        iterable = create_dataloader("config.yaml", framework="jax", writable=True)
    """
    if framework.lower() == "pytorch":
        if not HAVE_PYTORCH:
            raise FrameworkError("PyTorch integration not available. Install PyTorch and s3dlio.")
        
        # Determine if data_source is config file or URI
        if data_source.endswith(('.yaml', '.yml', '.json')):
            return create_pytorch_dataloader(data_source, **kwargs)
        else:
            from .pytorch import DlioPyTorchDataLoader
            return DlioPyTorchDataLoader.from_uri(data_source, **kwargs)
    
    elif framework.lower() == "tensorflow":
        if not HAVE_TENSORFLOW:
            raise FrameworkError("TensorFlow integration not available. Install TensorFlow and s3dlio.")
        
        if data_source.endswith(('.yaml', '.yml', '.json')):
            return create_tensorflow_dataset(data_source, **kwargs)
        else:
            return create_tensorflow_dataset_from_uri(data_source, **kwargs)
    
    elif framework.lower() == "jax":
        if not HAVE_TENSORFLOW:  # JAX uses TensorFlow integration backend
            raise FrameworkError("JAX integration not available. Install JAX, NumPy and s3dlio.")
        
        if data_source.endswith(('.yaml', '.yml', '.json')):
            return create_jax_iterable(data_source, **kwargs)
        else:
            # Create JAX iterable from URI
            config_dict = {
                'data_folder': data_source,
                **kwargs
            }
            jax_dataset = DlioJaxDataset(config_dict=config_dict)
            return jax_dataset.create_iterable()
    
    else:
        raise FrameworkError(f"Unsupported framework: {framework}. Use 'pytorch', 'tensorflow', or 'jax'.")


def list_available_frameworks() -> Dict[str, bool]:
    """
    List available framework integrations.
    
    Returns:
        Dictionary mapping framework names to availability status
    """
    return {
        'pytorch': HAVE_PYTORCH,
        'tensorflow': HAVE_TENSORFLOW, 
        'jax': HAVE_TENSORFLOW,  # JAX uses TensorFlow backend
    }


def get_framework_info() -> Dict[str, Any]:
    """
    Get detailed information about framework integration capabilities.
    
    Returns:
        Dictionary with framework integration details
    """
    info = {
        'available_frameworks': list_available_frameworks(),
        'supported_backends': ['file', 's3', 'azure', 'directio'],
        'supported_formats': ['npz', 'hdf5', 'tfrecord'],
        'features': [
            'DLIO configuration support',
            'Multi-backend URI handling',
            'Format-aware loading',
            'Async Rust backend via s3dlio',
            'Framework-specific optimizations',
            'Enterprise-grade performance'
        ]
    }
    
    # Add framework-specific details if available
    if HAVE_PYTORCH:
        info['pytorch'] = {
            'classes': ['DlioPyTorchDataset', 'DlioPyTorchDataLoader'],
            'return_types': ['tensor', 'bytes', 'reader'],
            'features': ['IterableDataset', 'MapDataset', 'Distributed sharding']
        }
    
    if HAVE_TENSORFLOW:
        info['tensorflow'] = {
            'classes': ['DlioTensorFlowDataset', 'DlioJaxDataset'],
            'return_types': ['tf.Tensor', 'np.ndarray'],
            'features': ['tf.data.Dataset', 'JAX compatibility', 'Deterministic ops']
        }
    
    return info


# Export key classes and functions
__all__ = [
    'create_dataloader',
    'list_available_frameworks', 
    'get_framework_info',
    'FrameworkError',
]

# Conditionally export framework-specific items
if HAVE_PYTORCH:
    __all__.extend([
        'DlioPyTorchDataset',
        'DlioPyTorchDataLoader',
        'create_pytorch_dataloader', 
        'create_pytorch_dataset',
    ])

if HAVE_TENSORFLOW:
    __all__.extend([
        'DlioTensorFlowDataset',
        'DlioJaxDataset',
        'create_tensorflow_dataset',
        'create_jax_iterable',
        'create_tensorflow_dataset_from_uri',
    ])