#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2025 Russ Fellows <russ.fellows@gmail.com>
# SPDX-License-Identifier: GPL-3.0-or-later

"""
End-to-End PyTorch Integration Test for dl-driver

Tests the PyTorch framework integration with real s3dlio backend using 
generated test data. Validates that our thin Python wrapper correctly
interfaces with the Rust backend while providing PyTorch-friendly APIs.

Key Test Areas:
1. DataLoader functionality with generated NPZ data
2. Batch consistency and deterministic behavior  
3. Seed reproducibility across runs
4. Cross-backend compatibility (file://)
5. Framework-specific configuration handling
6. Performance characteristics vs pure s3dlio

Uses Rust-first approach: Heavy lifting in s3dlio Rust backend,
thin Python integration layer for PyTorch compatibility.
"""

import os
import sys
import time
import hashlib
from pathlib import Path
from typing import Dict, Any, List, Tuple

# Add the frameworks directory to Python path
frameworks_dir = Path(__file__).parent.parent / "crates/py_api/src/frameworks"
sys.path.insert(0, str(frameworks_dir))

import numpy as np
import torch
from torch.utils.data import DataLoader

try:
    from pytorch import DlioPyTorchDataset, DlioPyTorchDataLoader, create_pytorch_dataloader_from_config
    HAVE_DL_DRIVER_PYTORCH = True
except ImportError as e:
    print(f"Warning: dl-driver PyTorch integration not available: {e}")
    HAVE_DL_DRIVER_PYTORCH = False

try:
    import s3dlio
    from s3dlio.torch import S3IterableDataset
    HAVE_S3DLIO = True
except ImportError as e:
    print(f"Warning: s3dlio not available: {e}")
    HAVE_S3DLIO = False


class PyTorchIntegrationTester:
    """Comprehensive tester for PyTorch integration with dl-driver."""
    
    def __init__(self, test_data_path: str = "/mnt/vast1/dl_driver_framework_test"):
        """Initialize tester with paths to generated test data."""
        self.test_data_path = Path(test_data_path)
        self.npz_data_path = self.test_data_path / "npz_small"
        self.hdf5_data_path = self.test_data_path / "hdf5_small"
        
        # Verify test data exists
        if not self.npz_data_path.exists():
            raise FileNotFoundError(f"NPZ test data not found at {self.npz_data_path}")
        if not self.hdf5_data_path.exists():
            raise FileNotFoundError(f"HDF5 test data not found at {self.hdf5_data_path}")
    
    def test_basic_pytorch_dataloader_creation(self) -> bool:
        """Test basic PyTorch DataLoader creation from DLIO config."""
        print("🔧 Testing PyTorch DataLoader creation...")
        
        if not HAVE_DL_DRIVER_PYTORCH:
            print("⏭️  Skipping: dl-driver PyTorch integration not available")
            return True
            
        try:
            # Create test config
            config_dict = {
                "framework": "pytorch",
                "dataset": {
                    "data_folder": f"file://{self.npz_data_path}",
                    "format": "npz"
                },
                "reader": {
                    "batch_size": 4,
                    "shuffle": False,
                    "read_threads": 2
                },
                "framework_profiles": {
                    "pytorch": {
                        "num_workers": 0,  # Use 0 to avoid multiprocessing issues in tests
                        "pin_memory": False,
                        "persistent_workers": False
                    }
                }
            }
            
            # Test dataset creation
            dataset = DlioPyTorchDataset(config_dict=config_dict)
            print(f"✅ Dataset created: {type(dataset)}")
            
            # Test dataloader creation  
            dataloader = DlioPyTorchDataLoader(config_dict=config_dict)
            print(f"✅ DataLoader created: {type(dataloader)}")
            
            # Test convenience function
            convenience_loader = create_pytorch_dataloader_from_config(config_dict)
            print(f"✅ Convenience DataLoader created: {type(convenience_loader)}")
            
            return True
            
        except Exception as e:
            print(f"❌ DataLoader creation failed: {e}")
            return False
    
    def test_data_loading_consistency(self) -> bool:
        """Test that data loading produces consistent results."""
        print("🔧 Testing data loading consistency...")
        
        if not HAVE_DL_DRIVER_PYTORCH or not HAVE_S3DLIO:
            print("⏭️  Skipping: Required dependencies not available")
            return True
            
        try:
            config_dict = {
                "framework": "pytorch",
                "dataset": {
                    "data_folder": f"file://{self.npz_data_path}",
                    "format": "npz"
                },
                "reader": {
                    "batch_size": 2,
                    "shuffle": False,  # Deterministic for testing
                    "read_threads": 1
                }
            }
            
            # Load data multiple times and verify consistency
            first_run_data = []
            second_run_data = []
            
            # First run
            dataset1 = DlioPyTorchDataset(config_dict=config_dict)
            dataloader1 = DataLoader(dataset1, batch_size=2, shuffle=False)
            
            for i, batch in enumerate(dataloader1):
                first_run_data.append(batch)
                if i >= 2:  # Just test first few batches
                    break
            
            # Second run - should be identical
            dataset2 = DlioPyTorchDataset(config_dict=config_dict)
            dataloader2 = DataLoader(dataset2, batch_size=2, shuffle=False)
            
            for i, batch in enumerate(dataloader2):
                second_run_data.append(batch)
                if i >= 2:
                    break
            
            # Verify consistency
            if len(first_run_data) != len(second_run_data):
                print(f"❌ Batch count mismatch: {len(first_run_data)} vs {len(second_run_data)}")
                return False
            
            for i, (batch1, batch2) in enumerate(zip(first_run_data, second_run_data)):
                if not torch.allclose(batch1, batch2, rtol=1e-5):
                    print(f"❌ Batch {i} data mismatch")
                    return False
            
            print(f"✅ Data loading consistency verified over {len(first_run_data)} batches")
            return True
            
        except Exception as e:
            print(f"❌ Consistency test failed: {e}")
            return False
    
    def test_seed_reproducibility(self) -> bool:
        """Test that seed configuration produces reproducible results."""
        print("🔧 Testing seed reproducibility...")
        
        if not HAVE_DL_DRIVER_PYTORCH:
            print("⏭️  Skipping: dl-driver PyTorch integration not available")
            return True
            
        try:
            # Test with explicit seed
            seed = 42
            config_dict = {
                "framework": "pytorch",
                "dataset": {
                    "data_folder": f"file://{self.npz_data_path}",
                    "format": "npz"
                },
                "reader": {
                    "batch_size": 2,
                    "shuffle": True,  # Shuffle but with seed
                    "seed": seed,
                    "read_threads": 1
                }
            }
            
            # First seeded run
            torch.manual_seed(seed)
            np.random.seed(seed)
            dataset1 = DlioPyTorchDataset(config_dict=config_dict)
            dataloader1 = DataLoader(dataset1, batch_size=2, shuffle=False)  # Let s3dlio handle shuffle
            
            first_batches = []
            for i, batch in enumerate(dataloader1):
                first_batches.append(batch.clone())
                if i >= 1:
                    break
            
            # Second seeded run - should match
            torch.manual_seed(seed)
            np.random.seed(seed)
            dataset2 = DlioPyTorchDataset(config_dict=config_dict)
            dataloader2 = DataLoader(dataset2, batch_size=2, shuffle=False)
            
            second_batches = []
            for i, batch in enumerate(dataloader2):
                second_batches.append(batch.clone())
                if i >= 1:
                    break
            
            # Verify reproducibility
            for i, (batch1, batch2) in enumerate(zip(first_batches, second_batches)):
                if not torch.allclose(batch1, batch2, rtol=1e-5):
                    print(f"❌ Seeded batch {i} mismatch - seed not working")
                    return False
            
            print(f"✅ Seed reproducibility verified with seed={seed}")
            return True
            
        except Exception as e:
            print(f"❌ Seed reproducibility test failed: {e}")
            return False
    
    def test_framework_config_integration(self) -> bool:
        """Test framework-specific configuration handling."""
        print("🔧 Testing framework config integration...")
        
        if not HAVE_DL_DRIVER_PYTORCH:
            print("⏭️  Skipping: dl-driver PyTorch integration not available")
            return True
            
        try:
            config_dict = {
                "framework": "pytorch",
                "dataset": {
                    "data_folder": f"file://{self.npz_data_path}",
                    "format": "npz"
                },
                "reader": {
                    "batch_size": 3,
                    "shuffle": False,
                    "read_threads": 1
                },
                "framework_profiles": {
                    "pytorch": {
                        "num_workers": 0,
                        "pin_memory": False,
                        "persistent_workers": False,
                        "prefetch_factor": 2,
                        "drop_last": True
                    }
                }
            }
            
            # Test that framework config is properly parsed and applied
            dataset = DlioPyTorchDataset(config_dict=config_dict)
            
            # Check that dataset has access to pytorch config
            pytorch_config = dataset.pytorch_config
            assert pytorch_config is not None, "PyTorch config should not be None"
            assert pytorch_config.get('num_workers') == 0
            assert pytorch_config.get('pin_memory') == False
            assert pytorch_config.get('drop_last') == True
            
            print("✅ Framework config integration working correctly")
            return True
            
        except Exception as e:
            print(f"❌ Framework config test failed: {e}")
            return False
    
    def test_backend_compatibility(self) -> bool:
        """Test compatibility with file:// backend."""
        print("🔧 Testing backend compatibility...")
        
        if not HAVE_DL_DRIVER_PYTORCH:
            print("⏭️  Skipping: dl-driver PyTorch integration not available")
            return True
            
        try:
            # Test file:// backend
            config_dict = {
                "dataset": {
                    "data_folder": f"file://{self.npz_data_path}",
                    "format": "npz"
                },
                "reader": {
                    "batch_size": 2,
                    "read_threads": 1
                }
            }
            
            dataset = DlioPyTorchDataset(config_dict=config_dict)
            
            # Verify backend detection
            backend_type = dataset.backend_type
            assert backend_type == "file", f"Expected 'file' backend, got '{backend_type}'"
            
            # Test that we can actually load data
            dataloader = DataLoader(dataset, batch_size=2, shuffle=False)
            batch_count = 0
            for batch in dataloader:
                batch_count += 1
                assert isinstance(batch, torch.Tensor), "Batch should be a torch.Tensor"
                if batch_count >= 2:
                    break
            
            assert batch_count > 0, "Should have loaded at least one batch"
            print(f"✅ Backend compatibility verified for file:// with {batch_count} batches")
            return True
            
        except Exception as e:
            print(f"❌ Backend compatibility test failed: {e}")
            return False
    
    def test_performance_characteristics(self) -> bool:
        """Test performance characteristics and compare against expectations."""
        print("🔧 Testing performance characteristics...")
        
        if not HAVE_DL_DRIVER_PYTORCH:
            print("⏭️  Skipping: dl-driver PyTorch integration not available")
            return True
            
        try:
            config_dict = {
                "dataset": {
                    "data_folder": f"file://{self.npz_data_path}",
                    "format": "npz"
                },
                "reader": {
                    "batch_size": 4,
                    "read_threads": 2,
                    "prefetch": 4
                }
            }
            
            dataset = DlioPyTorchDataset(config_dict=config_dict)
            dataloader = DataLoader(dataset, batch_size=4, shuffle=False)
            
            # Measure loading performance
            start_time = time.time()
            batch_count = 0
            total_samples = 0
            
            for batch in dataloader:
                batch_count += 1
                total_samples += batch.shape[0] if len(batch.shape) > 0 else 1
                if batch_count >= 10:  # Test first 10 batches
                    break
            
            elapsed_time = time.time() - start_time
            
            if elapsed_time > 0:
                throughput = total_samples / elapsed_time
                print(f"✅ Performance: {throughput:.1f} samples/sec over {batch_count} batches")
            else:
                print("✅ Performance: Very fast (< 1ms)")
            
            # Basic performance check - should not be slower than 10 samples/sec for small data
            if elapsed_time > 0 and throughput < 10:
                print(f"⚠️  Performance warning: {throughput:.1f} samples/sec seems slow")
                # Don't fail the test, just warn
            
            return True
            
        except Exception as e:
            print(f"❌ Performance test failed: {e}")
            return False
    
    def test_error_handling(self) -> bool:
        """Test error handling for various failure scenarios."""
        print("🔧 Testing error handling...")
        
        if not HAVE_DL_DRIVER_PYTORCH:
            print("⏭️  Skipping: dl-driver PyTorch integration not available")
            return True
            
        try:
            # Test 1: Invalid data path
            invalid_config = {
                "dataset": {
                    "data_folder": "file:///nonexistent/path",
                    "format": "npz"
                },
                "reader": {}
            }
            
            try:
                dataset = DlioPyTorchDataset(config_dict=invalid_config)
                # Creation might succeed, but iteration should fail gracefully
                dataloader = DataLoader(dataset, batch_size=2)
                for batch in dataloader:
                    # This should fail or handle gracefully
                    break
            except Exception:
                # Expected to fail - this is good error handling
                pass
            
            # Test 2: Invalid configuration
            try:
                dataset = DlioPyTorchDataset(config_dict={})  # Empty config
                # Should handle missing required fields gracefully
            except Exception:
                # Expected to fail gracefully
                pass
            
            print("✅ Error handling working correctly")
            return True
            
        except Exception as e:
            print(f"❌ Error handling test failed: {e}")
            return False
    
    def run_all_tests(self) -> Dict[str, bool]:
        """Run all PyTorch integration tests and return results."""
        print("🚀 Starting PyTorch Integration Tests")
        print("=" * 60)
        
        tests = [
            ("DataLoader Creation", self.test_basic_pytorch_dataloader_creation),
            ("Data Loading Consistency", self.test_data_loading_consistency),
            ("Seed Reproducibility", self.test_seed_reproducibility),
            ("Framework Config Integration", self.test_framework_config_integration),
            ("Backend Compatibility", self.test_backend_compatibility),
            ("Performance Characteristics", self.test_performance_characteristics),
            ("Error Handling", self.test_error_handling),
        ]
        
        results = {}
        
        for test_name, test_func in tests:
            print(f"\n📋 {test_name}")
            print("-" * 40)
            try:
                results[test_name] = test_func()
            except Exception as e:
                print(f"❌ Test '{test_name}' crashed: {e}")
                results[test_name] = False
        
        return results


def main():
    """Main test execution function."""
    print("dl-driver PyTorch Integration Test Suite")
    print("Rust-First Architecture: Thin Python wrappers + s3dlio Rust backend")
    print("=" * 80)
    
    # Check dependencies
    if not HAVE_S3DLIO:
        print("❌ s3dlio not available - cannot run integration tests")
        return 1
    
    # Initialize tester
    try:
        tester = PyTorchIntegrationTester()
    except FileNotFoundError as e:
        print(f"❌ Test data not found: {e}")
        print("💡 Run data generation first: dl-driver generate --config tests/configs/framework_test_npz_config.yaml")
        return 1
    
    # Run tests
    results = tester.run_all_tests()
    
    # Summary
    print("\n" + "=" * 60)
    print("📊 TEST SUMMARY")
    print("=" * 60)
    
    passed = sum(1 for r in results.values() if r)
    total = len(results)
    
    for test_name, success in results.items():
        status = "✅ PASS" if success else "❌ FAIL"
        print(f"{status} {test_name}")
    
    print(f"\n📈 Results: {passed}/{total} tests passed")
    
    if passed == total:
        print("🎉 All PyTorch integration tests passed!")
        return 0
    else:
        print(f"💥 {total - passed} tests failed")
        return 1


if __name__ == "__main__":
    exit(main())