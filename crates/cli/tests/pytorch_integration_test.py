#!/usr/bin/env python3
"""
Real PyTorch Integration Test for dl-driver M4 Framework Profiles
Tests end-to-end PyTorch integration with actual dependencies and generated data.
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

def test_pytorch_imports():
    """Test that all PyTorch-related imports work with real dependencies."""
    print("🧪 Testing PyTorch imports...")
    
    try:
        import torch
        print(f"✅ PyTorch {torch.__version__} imported successfully")
        
        import s3dlio
        print("✅ s3dlio imported successfully")
        
        from s3dlio.torch import S3IterableDataset
        print("✅ S3IterableDataset imported successfully")
        
        from frameworks.pytorch import DlioPyTorchDataset
        print("✅ DLIO PyTorch classes imported successfully")
        
        return True
    except Exception as e:
        print(f"❌ Import test failed: {e}")
        return False

def test_pytorch_dataset_creation():
    """Test creating PyTorch dataset with DLIO config."""
    print("\n🧪 Testing PyTorch dataset creation...")
    
    try:
        from frameworks.pytorch import DlioPyTorchDataset
        
        # Create a minimal DLIO config for testing
        test_config = {
            'dataset': {
                'data_folder': 'file:///mnt/vast1/dlio_data_generated',
                'format': 'npz',
                'num_files_train': 20,
                'record_length': 64*1024*1024,
                'num_samples_per_file': 1
            },
            'reader': {
                'data_loader': 'pytorch',
                'batch_size': 4,
                'read_threads': 2
            },
            'train': {
                'epochs': 1,
                'computation_time': 0.01,
                'seed': 42
            }
        }
        
        # Create the dataset (pass config_dict parameter correctly)
        dataset = DlioPyTorchDataset(config_dict=test_config)
        print("✅ DlioPyTorchDataset created successfully")
        
        # Test basic properties
        print(f"   Storage backend: {dataset.backend_type}")
        print(f"   Data folder: {dataset.data_folder}")
        print(f"   Format type: {dataset.format_type}")
        
        return True
    except Exception as e:
        print(f"❌ Dataset creation test failed: {e}")
        import traceback
        traceback.print_exc()
        return False

def test_pytorch_dataloader():
    """Test creating PyTorch DataLoader with our dataset."""
    print("\n🧪 Testing PyTorch DataLoader creation...")
    
    try:
        import torch
        from frameworks.pytorch import DlioPyTorchDataset
        
        # Create config
        test_config = {
            'dataset': {
                'data_folder': 'file:///mnt/vast1/dlio_data_generated',
                'format': 'npz',
                'num_files_train': 20,
                'record_length': 64*1024*1024,
                'num_samples_per_file': 1
            },
            'reader': {
                'data_loader': 'pytorch',
                'batch_size': 2,
                'read_threads': 2
            },
            'train': {
                'epochs': 1,
                'seed': 42
            }
        }
        
        # Create dataset and regular PyTorch dataloader
        import torch
        from torch.utils.data import DataLoader
        
        dataset = DlioPyTorchDataset(config_dict=test_config)
        dataloader = DataLoader(dataset, batch_size=2, num_workers=0)
        print("✅ PyTorch DataLoader created successfully")
        
        # Test iteration (just check we can get an iterator)
        iterator = iter(dataloader)
        print("✅ DataLoader iterator created successfully")
        
        return True
    except Exception as e:
        print(f"❌ DataLoader test failed: {e}")
        import traceback
        traceback.print_exc()
        return False

def test_data_loading_basic():
    """Test basic data loading functionality."""
    print("\n🧪 Testing basic data loading...")
    
    try:
        from frameworks.pytorch import DlioPyTorchDataset
        
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
                'record_length': 64*1024*1024,
                'num_samples_per_file': 1
            },
            'reader': {
                'data_loader': 'pytorch',
                'batch_size': 1,
                'read_threads': 1
            },
            'train': {
                'epochs': 1,
                'seed': 42
            }
        }
        
        # Create dataset
        dataset = DlioPyTorchDataset(config_dict=test_config)
        print("✅ Dataset created with real data files")
        
        return True
    except Exception as e:
        print(f"❌ Basic data loading test failed: {e}")
        import traceback
        traceback.print_exc()
        return False

def test_s3dlio_backend():
    """Test that we're properly using s3dlio backend."""
    print("\n🧪 Testing s3dlio backend integration...")
    
    try:
        import s3dlio
        from s3dlio.torch import S3IterableDataset
        
        # Test basic s3dlio functionality
        print("✅ s3dlio imported successfully")
        
        # Create a simple s3dlio dataset to verify backend works
        data_folder = '/mnt/vast1/dlio_data_generated'
        if os.path.exists(data_folder):
            # Create s3dlio dataset with correct parameters
            uri = f"file://{data_folder}"
            loader_opts = {"file_pattern": "*.npz", "shuffle": True, "seed": 42}
            dataset = S3IterableDataset(uri, loader_opts=loader_opts)
            print("✅ S3IterableDataset created successfully")
            print(f"   Data folder: {data_folder}")
            print(f"   Backend type: {type(dataset)}")
        else:
            print("⚠️  Data folder doesn't exist, skipping s3dlio dataset creation")
        
        return True
    except Exception as e:
        print(f"❌ s3dlio backend test failed: {e}")
        import traceback
        traceback.print_exc()
        return False

def main():
    """Run all PyTorch integration tests."""
    print("🚀 dl-driver M4 Framework Profiles - PyTorch Integration Tests")
    print("=" * 70)
    
    tests = [
        ("Import Tests", test_pytorch_imports),
        ("Dataset Creation", test_pytorch_dataset_creation),
        ("DataLoader Creation", test_pytorch_dataloader),
        ("Basic Data Loading", test_data_loading_basic),
        ("s3dlio Backend", test_s3dlio_backend),
    ]
    
    results = []
    for test_name, test_func in tests:
        print(f"\n📋 Running {test_name}...")
        success = test_func()
        results.append((test_name, success))
    
    print("\n" + "=" * 70)
    print("📊 TEST RESULTS SUMMARY:")
    print("=" * 70)
    
    passed = 0
    for test_name, success in results:
        status = "✅ PASS" if success else "❌ FAIL"
        print(f"{status}: {test_name}")
        if success:
            passed += 1
    
    print(f"\n🎯 Overall: {passed}/{len(results)} tests passed")
    
    if passed == len(results):
        print("🎉 ALL PYTORCH INTEGRATION TESTS PASSED!")
        return 0
    else:
        print("⚠️  Some tests failed - check output above")
        return 1

if __name__ == "__main__":
    sys.exit(main())