"""
dl-driver TensorFlow Integration

Provides TensorFlow tf.data.Dataset integration with dl-driver's:
- DLIO configuration support  
- Multi-backend capabilities (file://, s3://, az://, direct://)
- Format-aware loading (NPZ, HDF5, TFRecord)
- Workload metrics and performance tracking
- Enterprise-grade features while leveraging s3dlio's mature async backend

This module extends s3dlio's TensorFlow/JAX classes with dl-driver's configuration
and multi-format capabilities for seamless MLCommons DLIO integration.
"""

from __future__ import annotations

import os
import yaml
from pathlib import Path
from typing import Dict, Any, Optional, Union, Iterator, Callable
from urllib.parse import urlparse

# Import TensorFlow and JAX
try:
    import tensorflow as tf
    HAVE_TF = True
except ImportError:
    HAVE_TF = False
    tf = None

try:
    import jax
    import jax.numpy as jnp
    HAVE_JAX = True
except ImportError:
    HAVE_JAX = False
    jax = None
    jnp = None

try:
    import numpy as np
    HAVE_NUMPY = True
except ImportError:
    HAVE_NUMPY = False
    np = None

# Import s3dlio TensorFlow/JAX classes
try:
    import s3dlio
    from s3dlio.jax_tf import S3JaxIterable, make_tf_dataset
    HAVE_S3DLIO = True
except ImportError:
    HAVE_S3DLIO = False
    S3JaxIterable = None
    make_tf_dataset = None


class DlioTensorFlowError(Exception):
    """Exception raised by dl-driver TensorFlow integration."""
    pass


class DlioTensorFlowDataset:
    """
    dl-driver TensorFlow Dataset factory that wraps s3dlio with DLIO configuration support.
    
    Features:
    - DLIO YAML configuration parsing
    - Multi-backend URI support (file://, s3://, az://, direct://)
    - Format detection and handling (NPZ, HDF5, TFRecord)
    - Framework-specific configuration profiles
    - Seamless s3dlio integration with async Rust backend
    - tf.data.Dataset generation with proper batching and shuffling
    """
    
    def __init__(
        self,
        config_path: Optional[str] = None,
        config_dict: Optional[Dict[str, Any]] = None,
        data_folder: Optional[str] = None,
        tensorflow_config: Optional[Dict[str, Any]] = None,
        **kwargs
    ):
        """
        Initialize dl-driver TensorFlow Dataset factory.
        
        Args:
            config_path: Path to DLIO YAML configuration file
            config_dict: DLIO configuration as dictionary
            data_folder: Override data_folder from config (URI with scheme)
            tensorflow_config: TensorFlow-specific configuration override
            **kwargs: Additional s3dlio options (prefetch, num_workers, etc.)
        """
        if not HAVE_S3DLIO:
            raise DlioTensorFlowError(
                "s3dlio package is required for TensorFlow integration. "
                "Install with: pip install s3dlio"
            )
        
        if not HAVE_TF:
            raise DlioTensorFlowError(
                "TensorFlow is required for TensorFlow integration. "
                "Install with: pip install tensorflow"
            )
        
        if not HAVE_NUMPY:
            raise DlioTensorFlowError(
                "NumPy is required for TensorFlow integration. "
                "Install with: pip install numpy"
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
            raise DlioTensorFlowError("data_folder must be specified in config or as parameter")
        
        # Parse URI scheme for backend detection
        self.backend_type = self._detect_backend(self.data_folder)
        
        # Get TensorFlow-specific configuration
        self.tensorflow_config = self._get_tensorflow_config(tensorflow_config)
        
        # Get format configuration
        self.format_type = self._detect_format()
        
        # Build s3dlio options
        self.s3dlio_options = self._build_s3dlio_options(**kwargs)
    
    def _parse_config(
        self, 
        config_path: Optional[str], 
        config_dict: Optional[Dict[str, Any]]
    ) -> Dict[str, Any]:
        """Parse DLIO configuration from file or dictionary."""
        if config_path:
            if not os.path.exists(config_path):
                raise DlioTensorFlowError(f"Configuration file not found: {config_path}")
            
            with open(config_path, 'r') as f:
                if config_path.endswith('.yaml') or config_path.endswith('.yml'):
                    config = yaml.safe_load(f)
                else:
                    raise DlioTensorFlowError(f"Unsupported config format: {config_path}")
        elif config_dict:
            config = config_dict.copy()
        else:
            raise DlioTensorFlowError("Either config_path or config_dict must be provided")
        
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
            raise DlioTensorFlowError(f"Unsupported URI scheme: {scheme}")
    
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
    
    def _get_tensorflow_config(self, tensorflow_config: Optional[Dict[str, Any]]) -> Dict[str, Any]:
        """Extract and merge TensorFlow-specific configuration."""
        # Start with defaults
        config = {
            'batch_size': 32,
            'shuffle_buffer_size': 1000,
            'seed': 42,
            'num_parallel_calls': tf.data.AUTOTUNE if HAVE_TF else 4,
            'prefetch_buffer_size': tf.data.AUTOTUNE if HAVE_TF else 8,
            'deterministic': True,
            'writable': False,  # For NumPy array creation
        }
        
        # Update from DLIO config framework section
        if 'framework' in self.config:
            framework_config = self.config['framework']
            if 'tensorflow' in framework_config:
                config.update(framework_config['tensorflow'])
        
        # Update from direct tensorflow_config in DLIO
        if 'tensorflow_config' in self.config:
            config.update(self.config['tensorflow_config'])
        
        # Override with provided tensorflow_config
        if tensorflow_config:
            config.update(tensorflow_config)
        
        return config
    
    def _build_s3dlio_options(self, **kwargs) -> Dict[str, Any]:
        """Build s3dlio LoaderOptions from dl-driver configuration."""
        options = {}
        
        # Map dl-driver config to s3dlio options
        if 'batch_size' in self.tensorflow_config:
            options['batch_size'] = self.tensorflow_config['batch_size']
        
        # TensorFlow doesn't use shuffle at loader level, handled by tf.data
        options['shuffle'] = False
        
        if 'seed' in self.tensorflow_config:
            options['seed'] = self.tensorflow_config['seed']
        
        # Map DLIO configuration
        if 'num_readers' in self.config:
            options['num_workers'] = self.config['num_readers']
        elif 'num_parallel_calls' in self.tensorflow_config:
            calls = self.tensorflow_config['num_parallel_calls']
            if calls != tf.data.AUTOTUNE:
                options['num_workers'] = calls
        
        if 'prefetch_buffer' in self.config:
            options['prefetch'] = self.config['prefetch_buffer']
        elif 'prefetch_buffer_size' in self.tensorflow_config:
            prefetch = self.tensorflow_config['prefetch_buffer_size']
            if prefetch != tf.data.AUTOTUNE:
                options['prefetch'] = prefetch
        
        # Override with direct kwargs
        options.update(kwargs)
        
        return options
    
    def create_dataset(self) -> 'tf.data.Dataset':
        """
        Create tf.data.Dataset using s3dlio backend.
        
        Returns:
            Configured tf.data.Dataset
        """
        try:
            # Create s3dlio dataset factory
            def data_generator():
                """Generator function for tf.data.Dataset.from_generator."""
                # Use s3dlio's JAX iterable for NumPy arrays
                jax_iterable = S3JaxIterable.from_prefix(
                    uri=self.data_folder,
                    writable=self.tensorflow_config.get('writable', False),
                    **self.s3dlio_options
                )
                
                for data_bytes in jax_iterable:
                    # Process sample based on format type
                    yield self._process_sample(data_bytes)
            
            # Determine output signature based on format
            output_signature = self._get_output_signature()
            
            # Create tf.data.Dataset from generator
            dataset = tf.data.Dataset.from_generator(
                data_generator,
                output_signature=output_signature
            )
            
            # Apply TensorFlow-specific optimizations
            dataset = self._apply_tf_optimizations(dataset)
            
            return dataset
            
        except Exception as e:
            raise DlioTensorFlowError(f"Failed to create TensorFlow dataset: {e}")
    
    def _process_sample(self, data: Any) -> Any:
        """Process sample based on format type and return requirements."""
        # For now, return NumPy array as-is
        # Future: Add format-specific parsing (NPZ, HDF5, TFRecord)
        if isinstance(data, np.ndarray):
            return data
        elif isinstance(data, bytes):
            # Convert bytes to uint8 tensor
            return np.frombuffer(data, dtype=np.uint8)
        else:
            return data
    
    def _get_output_signature(self) -> tf.TensorSpec:
        """Get output signature for tf.data.Dataset.from_generator."""
        # Default to variable-length uint8 tensor
        return tf.TensorSpec(shape=(None,), dtype=tf.uint8)
    
    def _apply_tf_optimizations(self, dataset: 'tf.data.Dataset') -> 'tf.data.Dataset':
        """Apply TensorFlow-specific optimizations and configurations."""
        # Apply shuffling if configured
        if self.tensorflow_config.get('shuffle_buffer_size'):
            dataset = dataset.shuffle(
                buffer_size=self.tensorflow_config['shuffle_buffer_size'],
                seed=self.tensorflow_config.get('seed'),
                reshuffle_each_iteration=True
            )
        
        # Apply batching
        if self.tensorflow_config.get('batch_size', 1) > 1:
            dataset = dataset.batch(
                self.tensorflow_config['batch_size'],
                drop_remainder=self.tensorflow_config.get('drop_last', False),
                deterministic=self.tensorflow_config.get('deterministic', True)
            )
        
        # Apply prefetching
        if self.tensorflow_config.get('prefetch_buffer_size'):
            dataset = dataset.prefetch(self.tensorflow_config['prefetch_buffer_size'])
        
        # Set deterministic behavior
        if self.tensorflow_config.get('deterministic', True):
            tf.config.experimental.enable_op_determinism()
        
        return dataset
    
    @property
    def config_info(self) -> Dict[str, Any]:
        """Return configuration information for debugging."""
        return {
            'data_folder': self.data_folder,
            'backend_type': self.backend_type,
            'format_type': self.format_type,
            'tensorflow_config': self.tensorflow_config,
            's3dlio_options': self.s3dlio_options,
        }


class DlioJaxDataset:
    """
    dl-driver JAX Dataset that provides JAX-friendly NumPy array iteration.
    
    Leverages s3dlio's S3JaxIterable with dl-driver configuration support.
    """
    
    def __init__(
        self,
        config_path: Optional[str] = None,
        config_dict: Optional[Dict[str, Any]] = None,
        data_folder: Optional[str] = None,
        jax_config: Optional[Dict[str, Any]] = None,
        **kwargs
    ):
        """
        Initialize dl-driver JAX Dataset.
        
        Args:
            config_path: Path to DLIO YAML configuration file
            config_dict: DLIO configuration as dictionary
            data_folder: Override data_folder from config (URI with scheme)
            jax_config: JAX-specific configuration override
            **kwargs: Additional s3dlio options
        """
        if not HAVE_S3DLIO:
            raise DlioTensorFlowError("s3dlio package is required for JAX integration")
        
        if not HAVE_JAX:
            raise DlioTensorFlowError("JAX is required for JAX integration")
        
        if not HAVE_NUMPY:
            raise DlioTensorFlowError("NumPy is required for JAX integration")
        
        # Reuse TensorFlow dataset initialization logic
        self.tf_dataset = DlioTensorFlowDataset(
            config_path=config_path,
            config_dict=config_dict,
            data_folder=data_folder,
            tensorflow_config=jax_config,
            **kwargs
        )
    
    def create_iterable(self) -> Iterator[Any]:
        """
        Create JAX-friendly iterator yielding NumPy arrays.
        
        Returns:
            Iterator yielding NumPy arrays suitable for jnp.asarray()
        """
        try:
            jax_iterable = S3JaxIterable.from_prefix(
                uri=self.tf_dataset.data_folder,
                writable=self.tf_dataset.tensorflow_config.get('writable', False),
                **self.tf_dataset.s3dlio_options
            )
            
            for data in jax_iterable:
                yield data
                
        except Exception as e:
            raise DlioTensorFlowError(f"Failed to create JAX iterable: {e}")
    
    def __iter__(self):
        """Make this class directly iterable."""
        return self.create_iterable()


# High-level factory functions
def create_tensorflow_dataset(
    config_path: str,
    tensorflow_config: Optional[Dict[str, Any]] = None,
    **kwargs
) -> 'tf.data.Dataset':
    """
    Convenience function to create tf.data.Dataset from DLIO config.
    
    Args:
        config_path: Path to DLIO YAML configuration
        tensorflow_config: TensorFlow-specific configuration overrides
        **kwargs: Additional configuration options
        
    Returns:
        Configured tf.data.Dataset
    """
    dataset_factory = DlioTensorFlowDataset(
        config_path=config_path,
        tensorflow_config=tensorflow_config,
        **kwargs
    )
    return dataset_factory.create_dataset()


def create_jax_iterable(
    config_path: str,
    jax_config: Optional[Dict[str, Any]] = None,
    **kwargs
) -> Iterator[Any]:
    """
    Convenience function to create JAX iterable from DLIO config.
    
    Args:
        config_path: Path to DLIO YAML configuration
        jax_config: JAX-specific configuration overrides
        **kwargs: Additional configuration options
        
    Returns:
        Iterator yielding NumPy arrays for JAX
    """
    jax_dataset = DlioJaxDataset(
        config_path=config_path,
        jax_config=jax_config,
        **kwargs
    )
    return jax_dataset.create_iterable()


def create_tensorflow_dataset_from_uri(
    data_folder: str,
    batch_size: int = 32,
    **kwargs
) -> 'tf.data.Dataset':
    """
    Create tf.data.Dataset directly from data folder URI.
    
    Args:
        data_folder: Data folder URI (s3://, file://, etc.)
        batch_size: Batch size for dataset
        **kwargs: Additional configuration options
        
    Returns:
        Configured tf.data.Dataset
    """
    config_dict = {
        'data_folder': data_folder,
        'tensorflow_config': {
            'batch_size': batch_size,
            **kwargs
        }
    }
    
    dataset_factory = DlioTensorFlowDataset(config_dict=config_dict)
    return dataset_factory.create_dataset()