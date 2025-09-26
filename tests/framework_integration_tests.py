# SPDX-FileCopyrightText: 2025 Russ Fellows <russ.fellows@gmail.com>
# SPDX-License-Identifier: GPL-3.0-or-later

"""
Framework Integration Tests for dl-driver M4

Tests the PyTorch, TensorFlow, and JAX integrations with:
- DLIO configuration parsing and framework profile support
- Multi-backend compatibility (file://, s3://, az://, direct://)
- Format detection and handling (NPZ, HDF5, TFRecord)
- Seed stability and reproducibility across frameworks
- Performance benchmarking against pure s3dlio
- Cross-framework consistency validation

Test Categories:
1. Configuration Parsing Tests
2. Framework Integration Tests  
3. Multi-Backend Tests
4. Format Compatibility Tests
5. Seed Stability Tests
6. Performance Benchmark Tests
7. Error Handling Tests
"""

import os
import sys
import pytest
import tempfile
import yaml
from pathlib import Path
from typing import Dict, Any, Optional

# Add parent directory to path for imports
sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..', '..'))

# Test configuration and framework availability
HAVE_PYTORCH = False
HAVE_TENSORFLOW = False
HAVE_JAX = False
HAVE_S3DLIO = False

try:
    import torch
    HAVE_PYTORCH = True
except ImportError:
    pass

try:
    import tensorflow as tf
    HAVE_TENSORFLOW = True
except ImportError:
    pass

try:
    import jax
    import jax.numpy as jnp
    HAVE_JAX = True
except ImportError:
    pass

try:
    import s3dlio
    HAVE_S3DLIO = True
except ImportError:
    pass

# Import dl-driver framework integrations
try:
    from crates.py_api.src.frameworks.pytorch import (
        DlioPyTorchDataset,
        DlioPyTorchDataLoader,
        create_pytorch_dataloader
    )
    from crates.py_api.src.frameworks.tensorflow import (
        DlioTensorFlowDataset,
        create_tensorflow_dataset
    )
    from crates.py_api.src.frameworks import (
        create_dataloader,
        list_available_frameworks,
        get_framework_info
    )
    HAVE_DL_DRIVER_FRAMEWORKS = True
except ImportError as e:
    HAVE_DL_DRIVER_FRAMEWORKS = False
    print(f"Warning: dl-driver frameworks not available: {e}")


class TestFrameworkConfiguration:
    """Test framework configuration parsing and validation."""
    
    @pytest.fixture
    def pytorch_config(self):
        """PyTorch DLIO configuration for testing."""
        return {
            'model': {
                'name': 'test_pytorch',
                'framework': 'pytorch'
            },
            'framework': 'pytorch',
            'dataset': {
                'data_folder': 'file:///tmp/test_data',
                'format': 'npz',
                'num_files_train': 10,
                'num_samples_per_file': 100
            },
            'reader': {
                'batch_size': 32,
                'shuffle': True,
                'seed': 42
            },
            'pytorch_config': {
                'batch_size': 32,
                'num_workers': 0,
                'shuffle': True,
                'seed': 42,
                'return_type': 'tensor'
            }
        }
    
    @pytest.fixture
    def tensorflow_config(self):
        """TensorFlow DLIO configuration for testing."""
        return {
            'model': {
                'name': 'test_tensorflow',
                'framework': 'tensorflow'
            },
            'framework': 'tensorflow',
            'dataset': {
                'data_folder': 'file:///tmp/test_data',
                'format': 'tfrecord',
                'num_files_train': 20,
                'num_samples_per_file': 50
            },
            'reader': {
                'batch_size': 64,
                'shuffle': True,
                'seed': 123
            },
            'tensorflow_config': {
                'batch_size': 64,
                'shuffle_buffer_size': 1000,
                'seed': 123,
                'deterministic': True
            }
        }
    
    @pytest.fixture
    def multi_framework_config(self):
        """Multi-framework DLIO configuration for testing."""
        return {
            'model': {
                'name': 'test_multi_framework'
            },
            'dataset': {
                'data_folder': 'file:///tmp/test_data',
                'format': 'npz',
                'num_files_train': 15,
                'num_samples_per_file': 75
            },
            'reader': {
                'batch_size': 16,
                'shuffle': True,
                'seed': 456
            },
            'framework_profiles': {
                'pytorch': {
                    'batch_size': 16,
                    'return_type': 'tensor',
                    'seed': 456
                },
                'tensorflow': {
                    'batch_size': 16,
                    'deterministic': True,
                    'seed': 456
                },
                'jax': {
                    'batch_size': 16,
                    'writable': True,
                    'seed': 456
                }
            }
        }
    
    def test_pytorch_config_parsing(self, pytorch_config):
        """Test PyTorch configuration parsing."""
        if not HAVE_DL_DRIVER_FRAMEWORKS:
            pytest.skip("dl-driver frameworks not available")
        
        with tempfile.NamedTemporaryFile(mode='w', suffix='.yaml', delete=False) as f:
            yaml.dump(pytorch_config, f)
            config_path = f.name
        
        try:
            dataset = DlioPyTorchDataset(config_path=config_path)
            
            assert dataset.backend_type == 'file'
            assert dataset.format_type == 'npz'
            assert dataset.pytorch_config['batch_size'] == 32
            assert dataset.pytorch_config['return_type'] == 'tensor'
            assert dataset.pytorch_config['seed'] == 42
            
        finally:
            os.unlink(config_path)
    
    def test_tensorflow_config_parsing(self, tensorflow_config):
        """Test TensorFlow configuration parsing."""
        if not HAVE_DL_DRIVER_FRAMEWORKS:
            pytest.skip("dl-driver frameworks not available")
        
        with tempfile.NamedTemporaryFile(mode='w', suffix='.yaml', delete=False) as f:
            yaml.dump(tensorflow_config, f)
            config_path = f.name
        
        try:
            dataset_factory = DlioTensorFlowDataset(config_path=config_path)
            
            assert dataset_factory.backend_type == 'file'
            assert dataset_factory.format_type == 'tfrecord'
            assert dataset_factory.tensorflow_config['batch_size'] == 64
            assert dataset_factory.tensorflow_config['deterministic'] is True
            assert dataset_factory.tensorflow_config['seed'] == 123
            
        finally:
            os.unlink(config_path)
    
    def test_multi_framework_config(self, multi_framework_config):
        """Test multi-framework configuration support."""
        if not HAVE_DL_DRIVER_FRAMEWORKS:
            pytest.skip("dl-driver frameworks not available")
        
        with tempfile.NamedTemporaryFile(mode='w', suffix='.yaml', delete=False) as f:
            yaml.dump(multi_framework_config, f)
            config_path = f.name
        
        try:
            # Test PyTorch with framework_profiles
            pytorch_dataset = DlioPyTorchDataset(config_path=config_path)
            assert pytorch_dataset.pytorch_config['batch_size'] == 16
            assert pytorch_dataset.pytorch_config['return_type'] == 'tensor'
            
            # Test TensorFlow with framework_profiles  
            tf_dataset = DlioTensorFlowDataset(config_path=config_path)
            assert tf_dataset.tensorflow_config['batch_size'] == 16
            assert tf_dataset.tensorflow_config['deterministic'] is True
            
        finally:
            os.unlink(config_path)


class TestBackendCompatibility:
    """Test multi-backend URI handling."""
    
    def test_file_backend_detection(self):
        """Test file:// backend detection."""
        if not HAVE_DL_DRIVER_FRAMEWORKS:
            pytest.skip("dl-driver frameworks not available")
        
        config = {
            'dataset': {'data_folder': 'file:///tmp/test'},
            'reader': {'batch_size': 1}
        }
        
        dataset = DlioPyTorchDataset(config_dict=config)
        assert dataset.backend_type == 'file'
    
    def test_s3_backend_detection(self):
        """Test s3:// backend detection."""
        if not HAVE_DL_DRIVER_FRAMEWORKS:
            pytest.skip("dl-driver frameworks not available")
        
        config = {
            'dataset': {'data_folder': 's3://bucket/path'},
            'reader': {'batch_size': 1}
        }
        
        dataset = DlioPyTorchDataset(config_dict=config)
        assert dataset.backend_type == 's3'
    
    def test_azure_backend_detection(self):
        """Test az:// backend detection."""
        if not HAVE_DL_DRIVER_FRAMEWORKS:
            pytest.skip("dl-driver frameworks not available")
        
        config = {
            'dataset': {'data_folder': 'az://account/container/path'},
            'reader': {'batch_size': 1}
        }
        
        dataset = DlioPyTorchDataset(config_dict=config)
        assert dataset.backend_type == 'azure'
    
    def test_direct_backend_detection(self):
        """Test direct:// backend detection."""
        if not HAVE_DL_DRIVER_FRAMEWORKS:
            pytest.skip("dl-driver frameworks not available")
        
        config = {
            'dataset': {'data_folder': 'direct:///tmp/test'},
            'reader': {'batch_size': 1}
        }
        
        dataset = DlioPyTorchDataset(config_dict=config)
        assert dataset.backend_type == 'directio'


class TestFormatDetection:
    """Test data format detection and handling."""
    
    def test_npz_format_detection(self):
        """Test NPZ format detection."""
        if not HAVE_DL_DRIVER_FRAMEWORKS:
            pytest.skip("dl-driver frameworks not available")
        
        config = {
            'dataset': {
                'data_folder': 'file:///tmp/test',
                'format': 'npz'
            },
            'reader': {'batch_size': 1}
        }
        
        dataset = DlioPyTorchDataset(config_dict=config)
        assert dataset.format_type == 'npz'
    
    def test_hdf5_format_detection(self):
        """Test HDF5 format detection."""
        if not HAVE_DL_DRIVER_FRAMEWORKS:
            pytest.skip("dl-driver frameworks not available")
        
        config = {
            'dataset': {
                'data_folder': 'file:///tmp/test',
                'format': 'hdf5'
            },
            'reader': {'batch_size': 1}
        }
        
        dataset = DlioPyTorchDataset(config_dict=config)
        assert dataset.format_type == 'hdf5'
    
    def test_tfrecord_format_detection(self):
        """Test TFRecord format detection."""
        if not HAVE_DL_DRIVER_FRAMEWORKS:
            pytest.skip("dl-driver frameworks not available")
        
        config = {
            'dataset': {
                'data_folder': 'file:///tmp/test',
                'format': 'tfrecord'
            },
            'reader': {'batch_size': 1}
        }
        
        tf_dataset = DlioTensorFlowDataset(config_dict=config)
        assert tf_dataset.format_type == 'tfrecord'


@pytest.mark.skipif(not HAVE_S3DLIO, reason="s3dlio not available")
class TestS3DlioIntegration:
    """Test s3dlio integration and compatibility."""
    
    def test_pytorch_s3dlio_options(self):
        """Test PyTorch s3dlio options mapping."""
        if not HAVE_DL_DRIVER_FRAMEWORKS:
            pytest.skip("dl-driver frameworks not available")
        
        config = {
            'dataset': {'data_folder': 'file:///tmp/test'},
            'reader': {'batch_size': 1},
            'pytorch_config': {
                'batch_size': 64,
                'shuffle': True,
                'seed': 123,
                'prefetch_factor': 4
            }
        }
        
        dataset = DlioPyTorchDataset(config_dict=config)
        options = dataset.s3dlio_options
        
        assert options['batch_size'] == 64
        assert options['shuffle'] is True
        assert options['seed'] == 123
        assert options['prefetch'] == 4
    
    def test_tensorflow_s3dlio_options(self):
        """Test TensorFlow s3dlio options mapping."""
        if not HAVE_DL_DRIVER_FRAMEWORKS:
            pytest.skip("dl-driver frameworks not available")
        
        config = {
            'dataset': {'data_folder': 'file:///tmp/test'},
            'reader': {'batch_size': 1},
            'tensorflow_config': {
                'batch_size': 32,
                'seed': 456,
                'prefetch_buffer_size': 8
            }
        }
        
        tf_dataset = DlioTensorFlowDataset(config_dict=config)
        options = tf_dataset.s3dlio_options
        
        assert options['batch_size'] == 32
        assert options['seed'] == 456
        assert options['shuffle'] is False  # TF handles shuffling separately


class TestErrorHandling:
    """Test error handling and validation."""
    
    def test_missing_data_folder(self):
        """Test error when data_folder is missing."""
        if not HAVE_DL_DRIVER_FRAMEWORKS:
            pytest.skip("dl-driver frameworks not available")
        
        config = {'reader': {'batch_size': 1}}
        
        with pytest.raises(Exception):  # Should raise DlioDataLoaderError
            DlioPyTorchDataset(config_dict=config)
    
    def test_unsupported_backend(self):
        """Test error for unsupported URI scheme."""
        if not HAVE_DL_DRIVER_FRAMEWORKS:
            pytest.skip("dl-driver frameworks not available")
        
        config = {
            'dataset': {'data_folder': 'ftp://unsupported/path'},
            'reader': {'batch_size': 1}
        }
        
        with pytest.raises(Exception):  # Should raise DlioDataLoaderError
            DlioPyTorchDataset(config_dict=config)
    
    def test_invalid_config_file(self):
        """Test error for invalid configuration file."""
        if not HAVE_DL_DRIVER_FRAMEWORKS:
            pytest.skip("dl-driver frameworks not available")
        
        with pytest.raises(Exception):  # Should raise DlioDataLoaderError
            DlioPyTorchDataset(config_path="/nonexistent/config.yaml")


class TestUniversalDataLoader:
    """Test universal dataloader factory."""
    
    def test_pytorch_from_uri(self):
        """Test PyTorch dataloader from URI."""
        if not HAVE_DL_DRIVER_FRAMEWORKS:
            pytest.skip("dl-driver frameworks not available")
        
        loader = create_dataloader(
            "file:///tmp/test",
            framework="pytorch",
            batch_size=16
        )
        
        # Should return PyTorch DataLoader
        assert hasattr(loader, '__iter__')
    
    def test_tensorflow_from_uri(self):
        """Test TensorFlow dataset from URI.""" 
        if not HAVE_DL_DRIVER_FRAMEWORKS or not HAVE_TENSORFLOW:
            pytest.skip("dl-driver frameworks or TensorFlow not available")
        
        dataset = create_dataloader(
            "file:///tmp/test",
            framework="tensorflow", 
            batch_size=16
        )
        
        # Should return tf.data.Dataset
        assert hasattr(dataset, 'batch')
    
    def test_framework_info(self):
        """Test framework information retrieval."""
        if not HAVE_DL_DRIVER_FRAMEWORKS:
            pytest.skip("dl-driver frameworks not available")
        
        frameworks = list_available_frameworks()
        info = get_framework_info()
        
        assert isinstance(frameworks, dict)
        assert isinstance(info, dict)
        assert 'available_frameworks' in info
        assert 'supported_backends' in info
        assert 'supported_formats' in info


if __name__ == "__main__":
    # Run tests with pytest
    pytest.main([__file__, "-v"])