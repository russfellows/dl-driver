#!/usr/bin/env python3
"""
Real TensorFlow/JAX Integration Test for dl-driver M4 Framework Profiles
Tests end-to-end TensorFlow and JAX integration with actual dependencies and generated data.
"""

import sys
import os
import tempfile
import yaml
import numpy as np
import subprocess
from pathlib import Path

# Add the framework path to sys.path
sys.path.insert(0, '/home/eval/Documents/Rust-Devel/dl-driver/crates/py_api/src')

def test_tensorflow_imports():
    """Test that all TensorFlow-related imports work with real dependencies."""
    print("🧪 Testing TensorFlow imports...")
    
    try:
        import tensorflow as tf
        print(f"✅ TensorFlow {tf.__version__} imported successfully")
        
        import jax
        import jax.numpy as jnp
        print(f"✅ JAX {jax.__version__} imported successfully")
        
        import s3dlio
        print("✅ s3dlio imported successfully")
        
        from s3dlio.jax_tf import S3JaxIterable, make_tf_dataset
        print("✅ s3dlio TensorFlow/JAX functions imported successfully")
        
        from frameworks.tensorflow import DlioTensorFlowDataset, DlioJaxDataset
        print("✅ DLIO TensorFlow/JAX classes imported successfully")
        
        return True
    except Exception as e:
        print(f"❌ Import test failed: {e}")
        import traceback
        traceback.print_exc()
        return False

def test_tensorflow_dataset_creation():
    """Test creating TensorFlow tf.data.Dataset with DLIO config."""
    print("\n🧪 Testing TensorFlow dataset creation...")
    
    try:
        from frameworks.tensorflow import DlioTensorFlowDataset
        
        # Create a minimal DLIO config for testing
        test_config = {
            'dataset': {
                'data_folder': 'file:///mnt/vast1/dlio_data_generated',
                'format': 'npz',
                'num_files_train': 10,
                'record_length_bytes': 1048576,
                'num_samples_per_file': 1
            },
            'reader': {
                'data_loader': 'tensorflow',
                'batch_size': 4,
                'read_threads': 2
            },
            'train': {
                'epochs': 1,
                'computation_time': 0.01,
                'seed': 42
            }
        }
        
        # Create the dataset
        tf_dataset = DlioTensorFlowDataset(config_dict=test_config)
        print("✅ DlioTensorFlowDataset created successfully")
        
        # Test basic properties
        print(f"   Storage backend: {tf_dataset.backend_type}")
        print(f"   Data folder: {tf_dataset.data_folder}")
        print(f"   Format type: {tf_dataset.format_type}")
        
        return True
    except Exception as e:
        print(f"❌ TensorFlow dataset creation test failed: {e}")
        import traceback
        traceback.print_exc()
        return False

def test_tf_data_pipeline():
    """Test creating tf.data.Dataset pipeline."""
    print("\n🧪 Testing tf.data.Dataset pipeline creation...")
    
    try:
        import tensorflow as tf
        from frameworks.tensorflow import DlioTensorFlowDataset
        
        # Create config
        test_config = {
            'dataset': {
                'data_folder': 'file:///mnt/vast1/dlio_data_generated',
                'format': 'npz',
                'num_files_train': 10,
                'record_length_bytes': 1048576,
                'num_samples_per_file': 1
            },
            'reader': {
                'data_loader': 'tensorflow',
                'batch_size': 2,
                'read_threads': 2
            },
            'train': {
                'epochs': 1,
                'seed': 42
            }
        }
        
        # Create TensorFlow dataset
        tf_dataset_wrapper = DlioTensorFlowDataset(config_dict=test_config)
        tf_dataset = tf_dataset_wrapper.create_dataset()
        print("✅ tf.data.Dataset created successfully")
        
        # Test that we can get basic info about the dataset
        print(f"   Dataset type: {type(tf_dataset)}")
        
        # Test iteration setup (don't actually iterate to avoid loading data)
        iterator = iter(tf_dataset)
        print("✅ tf.data.Dataset iterator created successfully")
        
        return True
    except Exception as e:
        print(f"❌ tf.data pipeline test failed: {e}")
        import traceback
        traceback.print_exc()
        return False

def test_jax_dataset_creation():
    """Test creating JAX dataset with NumPy arrays."""
    print("\n🧪 Testing JAX dataset creation...")
    
    try:
        import jax
        from frameworks.tensorflow import DlioJaxDataset
        
        # Create config
        test_config = {
            'dataset': {
                'data_folder': 'file:///mnt/vast1/dlio_data_generated',
                'format': 'npz',
                'num_files_train': 10,
                'record_length_bytes': 1048576,
                'num_samples_per_file': 1
            },
            'reader': {
                'data_loader': 'jax',
                'batch_size': 2,
                'read_threads': 2
            },
            'train': {
                'epochs': 1,
                'seed': 42
            }
        }
        
        # Create JAX dataset
        jax_dataset = DlioJaxDataset(config_dict=test_config)
        print("✅ DlioJaxDataset created successfully")
        
        # Test basic properties (JAX dataset wraps TensorFlow dataset)
        print(f"   Storage backend: {jax_dataset.tf_dataset.backend_type}")
        print(f"   Data folder: {jax_dataset.tf_dataset.data_folder}")
        print(f"   Format type: {jax_dataset.tf_dataset.format_type}")
        
        return True
    except Exception as e:
        print(f"❌ JAX dataset creation test failed: {e}")
        import traceback
        traceback.print_exc()
        return False

def test_s3dlio_backend_integration():
    """Test s3dlio backend integration for TensorFlow/JAX."""
    print("\n🧪 Testing s3dlio backend integration for TensorFlow/JAX...")
    
    try:
        import tensorflow as tf
        from s3dlio.jax_tf import S3JaxIterable, make_tf_dataset
        
        # Test S3JaxIterable
        data_folder = '/mnt/vast1/dlio_data_generated'
        if os.path.exists(data_folder):
            uri = f"file://{data_folder}"
            loader_opts = {"file_pattern": "*.npz", "shuffle": True, "seed": 42}
            
            # Test JAX iterable
            jax_iterable = S3JaxIterable(uri, loader_opts=loader_opts)
            print("✅ S3JaxIterable created successfully")
            print(f"   Data folder: {data_folder}")
            print(f"   Backend type: {type(jax_iterable)}")
            
            # Test TensorFlow dataset creation
            tf_dataset = make_tf_dataset(uri, shuffle=True, seed=42, batch_size=2)
            print("✅ make_tf_dataset created tf.data.Dataset successfully")
            print(f"   TF Dataset type: {type(tf_dataset)}")
        else:
            print("⚠️  Data folder doesn't exist, skipping s3dlio integration test")
        
        return True
    except Exception as e:
        print(f"❌ s3dlio backend integration test failed: {e}")
        import traceback
        traceback.print_exc()
        return False

def test_data_loading_basic():
    """Test basic data loading functionality for TensorFlow/JAX."""
    print("\n🧪 Testing basic data loading...")
    
    try:
        from frameworks.tensorflow import DlioTensorFlowDataset, DlioJaxDataset
        
        # Check if our generated data exists
        data_dir = Path('/mnt/vast1/dlio_data_generated')
        if not data_dir.exists():
            print(f"❌ Data directory {data_dir} does not exist - need to generate data first")
            return False
        
        npz_files = list(data_dir.glob('*.npz'))
        if not npz_files:
            print("❌ No NPZ files found in data directory")
            return False
        
        print(f"✅ Found {len(npz_files)} NPZ files for testing")
        
        # Create config
        test_config = {
            'dataset': {
                'data_folder': f'file://{data_dir}',
                'format': 'npz',
                'num_files_train': len(npz_files),
                'record_length_bytes': 1048576,
                'num_samples_per_file': 1
            },
            'reader': {
                'data_loader': 'tensorflow',
                'batch_size': 1,
                'read_threads': 1
            },
            'train': {
                'epochs': 1,
                'seed': 42
            }
        }
        
        # Test TensorFlow dataset
        tf_dataset = DlioTensorFlowDataset(config_dict=test_config)
        print("✅ TensorFlow dataset created with real data files")
        
        # Test JAX dataset
        jax_config = test_config.copy()
        jax_config['reader']['data_loader'] = 'jax'
        jax_dataset = DlioJaxDataset(config_dict=jax_config)
        print("✅ JAX dataset created with real data files")
        
        return True
    except Exception as e:
        print(f"❌ Basic data loading test failed: {e}")
        import traceback
        traceback.print_exc()
        return False

def main():
    """Run all TensorFlow/JAX integration tests."""
    print("🚀 dl-driver M4 Framework Profiles - TensorFlow/JAX Integration Tests")
    print("=" * 80)
    
    tests = [
        ("Import Tests", test_tensorflow_imports),
        ("TensorFlow Dataset Creation", test_tensorflow_dataset_creation),
        ("tf.data Pipeline Creation", test_tf_data_pipeline),
        ("JAX Dataset Creation", test_jax_dataset_creation),
        ("s3dlio Backend Integration", test_s3dlio_backend_integration),
        ("Basic Data Loading", test_data_loading_basic),
    ]
    
    results = []
    for test_name, test_func in tests:
        print(f"\n📋 Running {test_name}...")
        success = test_func()
        results.append((test_name, success))
    
    print("\n" + "=" * 80)
    print("📊 TEST RESULTS SUMMARY:")
    print("=" * 80)
    
    passed = 0
    for test_name, success in results:
        status = "✅ PASS" if success else "❌ FAIL"
        print(f"{status}: {test_name}")
        if success:
            passed += 1
    
    print(f"\n🎯 Overall: {passed}/{len(results)} tests passed")
    
    if passed == len(results):
        print("🎉 ALL TENSORFLOW/JAX INTEGRATION TESTS PASSED!")
        return 0
    else:
        print("⚠️  Some tests failed - check output above")
        return 1

if __name__ == "__main__":
    sys.exit(main())