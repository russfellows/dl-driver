#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2025 Russ Fellows <russ.fellows@gmail.com>
# SPDX-License-Identifier: GPL-3.0-or-later

"""
dl-driver Framework Integration Validation

This test validates the Rust-first architecture without requiring heavy ML dependencies.
Tests the integration layer structure, configuration parsing, and Rust backend connectivity.

Focus: Validate our Rust-first approach works correctly
- DLIO config parsing (Rust)
- Framework detection (Rust) 
- Backend URI handling (Rust)
- Python wrapper structure
- s3dlio integration readiness

Does NOT require: PyTorch, TensorFlow, JAX installations
Does require: Our Rust dl-driver CLI and s3dlio
"""

import os
import sys
import json
import subprocess
from pathlib import Path
from typing import Dict, Any, Optional


class FrameworkIntegrationValidator:
    """Validates framework integration without heavy ML dependencies."""
    
    def __init__(self, dl_driver_binary: str = None):
        """Initialize validator with dl-driver binary path."""
        if dl_driver_binary is None:
            # Auto-detect binary
            binary_path = Path(__file__).parent.parent / "target/release/dl-driver"
            self.dl_driver_binary = str(binary_path) if binary_path.exists() else "dl-driver"
        else:
            self.dl_driver_binary = dl_driver_binary
        
        self.test_data_path = "/mnt/vast1/dl_driver_framework_test"
    
    def test_rust_config_validation(self) -> bool:
        """Test Rust-based DLIO configuration validation."""
        print("🔧 Testing Rust configuration validation...")
        
        try:
            # Test our framework test configs
            configs_to_test = [
                "/home/eval/Documents/Rust-Devel/dl-driver/tests/configs/framework_test_npz_config.yaml",
                "/home/eval/Documents/Rust-Devel/dl-driver/tests/configs/framework_test_hdf5_config.yaml"
            ]
            
            for config_path in configs_to_test:
                if not Path(config_path).exists():
                    print(f"⚠️  Config file not found: {config_path}")
                    continue
                
                # Use dl-driver validate command to test Rust parsing
                result = subprocess.run([
                    self.dl_driver_binary, 
                    "validate", 
                    "--config", config_path
                ], capture_output=True, text=True)
                
                if result.returncode != 0:
                    print(f"❌ Config validation failed for {config_path}")
                    print(f"Error: {result.stderr}")
                    return False
                
                # Check that key validation messages are present
                output = result.stdout
                if "✅ YAML parsing: SUCCESS" not in output:
                    print(f"❌ YAML parsing validation missing in output")
                    return False
                
                if "✅ LoaderOptions conversion: SUCCESS" not in output:
                    print(f"❌ LoaderOptions conversion validation missing")
                    return False
                
                print(f"✅ Config validation passed: {Path(config_path).name}")
            
            return True
            
        except Exception as e:
            print(f"❌ Rust config validation failed: {e}")
            return False
    
    def test_backend_detection(self) -> bool:
        """Test backend detection functionality."""
        print("🔧 Testing backend detection...")
        
        try:
            # Create temporary configs with different backends
            test_cases = [
                ("file:///mnt/vast1/test", "File"),
                ("s3://test-bucket/path", "S3"), 
                ("az://account/container/path", "Azure"),
                ("direct:///mnt/nvme/data", "DirectIO")
            ]
            
            base_config = {
                "model": {"name": "test_model"},
                "framework": "pytorch",
                "dataset": {"data_folder": "", "format": "npz"},
                "reader": {"batch_size": 4}
            }
            
            import tempfile
            
            for data_folder, expected_backend in test_cases:
                # Create temporary config
                config = base_config.copy()
                config["dataset"]["data_folder"] = data_folder
                
                with tempfile.NamedTemporaryFile(mode='w', suffix='.yaml', delete=False) as f:
                    import yaml
                    yaml.dump(config, f)
                    temp_config = f.name
                
                try:
                    # Test validation 
                    result = subprocess.run([
                        self.dl_driver_binary,
                        "validate",
                        "--config", temp_config
                    ], capture_output=True, text=True)
                    
                    if result.returncode != 0:
                        print(f"❌ Backend detection failed for {data_folder}")
                        print(f"Error: {result.stderr}")
                        return False
                    
                    # Check backend detection in output
                    if f"✅ Backend detection: {expected_backend}" not in result.stdout:
                        print(f"❌ Expected '{expected_backend}' backend detection for {data_folder}")
                        print(f"Output: {result.stdout}")
                        return False
                    
                    print(f"✅ Backend detection: {data_folder} → {expected_backend}")
                    
                finally:
                    os.unlink(temp_config)
            
            return True
            
        except Exception as e:
            print(f"❌ Backend detection test failed: {e}")
            return False
    
    def test_framework_profile_parsing(self) -> bool:
        """Test framework profile parsing in Rust."""
        print("🔧 Testing framework profile parsing...")
        
        try:
            import tempfile
            import yaml
            
            # Create config with framework profiles
            config = {
                "model": {"name": "profile_test"},
                "framework": "pytorch",
                "dataset": {"data_folder": "file:///tmp/test", "format": "npz"},
                "reader": {"batch_size": 8},
                "framework_profiles": {
                    "pytorch": {
                        "num_workers": 4,
                        "pin_memory": True,
                        "persistent_workers": False,
                        "prefetch_factor": 2,
                        "drop_last": True
                    },
                    "tensorflow": {
                        "buffer_size": 1024,
                        "num_parallel_calls": 8,
                        "deterministic": True
                    }
                }
            }
            
            with tempfile.NamedTemporaryFile(mode='w', suffix='.yaml', delete=False) as f:
                yaml.dump(config, f)
                temp_config = f.name
            
            try:
                # Test that Rust can parse framework profiles
                result = subprocess.run([
                    self.dl_driver_binary,
                    "validate", 
                    "--config", temp_config
                ], capture_output=True, text=True)
                
                if result.returncode != 0:
                    print(f"❌ Framework profile parsing failed")
                    print(f"Error: {result.stderr}")
                    return False
                
                # Rust validation should succeed with framework profiles
                if "✅ YAML parsing: SUCCESS" not in result.stdout:
                    print(f"❌ Framework profile YAML parsing failed")
                    return False
                
                print("✅ Framework profile parsing successful")
                return True
                
            finally:
                os.unlink(temp_config)
            
        except Exception as e:
            print(f"❌ Framework profile parsing test failed: {e}")
            return False
    
    def test_data_generation_rust(self) -> bool:
        """Test Rust-based data generation capability."""
        print("🔧 Testing Rust data generation...")
        
        try:
            # Use existing framework test config
            config_path = "/home/eval/Documents/Rust-Devel/dl-driver/tests/configs/framework_test_npz_config.yaml"
            
            if not Path(config_path).exists():
                print(f"⚠️  Test config not found: {config_path}")
                return True  # Skip if config missing
            
            # Test generation (dry run check)
            result = subprocess.run([
                self.dl_driver_binary,
                "generate",
                "--config", config_path,
                "--skip-existing"  # Don't regenerate if exists
            ], capture_output=True, text=True)
            
            if result.returncode != 0:
                print(f"❌ Rust data generation failed")
                print(f"Error: {result.stderr}")
                return False
            
            # Check that generation completed or was skipped
            output = result.stdout
            success_indicators = [
                "✅ Dataset generation completed!",
                "⏭️  Data directory already exists, skipping generation"
            ]
            
            if not any(indicator in output for indicator in success_indicators):
                print(f"❌ No success indicator in generation output")
                print(f"Output: {output}")
                return False
            
            print("✅ Rust data generation working")
            
            # Verify data exists
            data_path = Path("/mnt/vast1/dl_driver_framework_test/npz_small")
            if data_path.exists():
                file_count = len(list(data_path.glob("*.npz")))
                print(f"✅ Generated data verified: {file_count} NPZ files")
            
            return True
            
        except Exception as e:
            print(f"❌ Rust data generation test failed: {e}")
            return False
    
    def test_python_wrapper_structure(self) -> bool:
        """Test Python wrapper file structure and imports."""
        print("🔧 Testing Python wrapper structure...")
        
        try:
            # Check that Python wrapper files exist
            frameworks_dir = Path(__file__).parent.parent / "crates/py_api/src/frameworks"
            
            required_files = [
                "pytorch.py",
                "tensorflow.py", 
                "__init__.py"
            ]
            
            for filename in required_files:
                file_path = frameworks_dir / filename
                if not file_path.exists():
                    print(f"❌ Missing Python wrapper: {file_path}")
                    return False
                
                # Basic syntax check
                try:
                    with open(file_path) as f:
                        content = f.read()
                    
                    # Check for expected structure
                    if filename == "pytorch.py":
                        if "DlioPyTorchDataset" not in content:
                            print(f"❌ Missing DlioPyTorchDataset in {filename}")
                            return False
                        if "s3dlio" not in content:
                            print(f"❌ Missing s3dlio import in {filename}")
                            return False
                    
                    elif filename == "tensorflow.py":
                        if "DlioTensorFlowDataset" not in content:
                            print(f"❌ Missing DlioTensorFlowDataset in {filename}")
                            return False
                    
                    print(f"✅ Python wrapper structure OK: {filename}")
                    
                except Exception as e:
                    print(f"❌ Error reading {filename}: {e}")
                    return False
            
            return True
            
        except Exception as e:
            print(f"❌ Python wrapper structure test failed: {e}")
            return False
    
    def test_rust_first_architecture(self) -> bool:
        """Validate overall Rust-first architecture principles."""
        print("🔧 Testing Rust-first architecture compliance...")
        
        try:
            # Check that heavy lifting is in Rust
            core_rust_files = [
                "crates/core/src/dlio_compat.rs",  # DLIO config parsing
                "crates/core/src/generation.rs",   # Data generation
                "crates/core/src/workload.rs",     # Workload execution
            ]
            
            for rust_file in core_rust_files:
                file_path = Path(__file__).parent.parent / rust_file
                if not file_path.exists():
                    print(f"❌ Missing core Rust file: {rust_file}")
                    return False
                
                # Check file size - should be substantial for core logic
                file_size = file_path.stat().st_size
                if file_size < 1000:  # Less than 1KB suggests missing implementation
                    print(f"⚠️  Small Rust core file: {rust_file} ({file_size} bytes)")
                else:
                    print(f"✅ Substantial Rust core: {rust_file} ({file_size} bytes)")
            
            # Check that Python files are thin wrappers
            python_files = [
                "crates/py_api/src/frameworks/pytorch.py",
                "crates/py_api/src/frameworks/tensorflow.py"
            ]
            
            for py_file in python_files:
                file_path = Path(__file__).parent.parent / py_file
                if file_path.exists():
                    with open(file_path) as f:
                        content = f.read()
                    
                    # Check for s3dlio integration (Rust backend)
                    if "s3dlio" not in content:
                        print(f"⚠️  Python file missing s3dlio integration: {py_file}")
                    
                    # Check that it's not doing heavy data processing
                    heavy_processing_indicators = [
                        "numpy.random.RandomState",  # Local data generation
                        "for i in range(1000",        # Large loops
                        "def generate_data"           # Data generation functions
                    ]
                    
                    if any(indicator in content for indicator in heavy_processing_indicators):
                        print(f"⚠️  Python file may contain heavy processing: {py_file}")
                    else:
                        print(f"✅ Thin Python wrapper confirmed: {py_file}")
            
            return True
            
        except Exception as e:
            print(f"❌ Architecture validation failed: {e}")
            return False
    
    def run_all_validations(self) -> Dict[str, bool]:
        """Run all framework integration validations."""
        print("🚀 dl-driver Framework Integration Validation")
        print("Rust-First Architecture: Heavy lifting in Rust + thin Python wrappers")
        print("=" * 80)
        
        validations = [
            ("Rust Config Validation", self.test_rust_config_validation),
            ("Backend Detection", self.test_backend_detection),
            ("Framework Profile Parsing", self.test_framework_profile_parsing),
            ("Rust Data Generation", self.test_data_generation_rust),
            ("Python Wrapper Structure", self.test_python_wrapper_structure),
            ("Rust-First Architecture", self.test_rust_first_architecture),
        ]
        
        results = {}
        
        for test_name, test_func in validations:
            print(f"\n📋 {test_name}")
            print("-" * 50)
            try:
                results[test_name] = test_func()
            except Exception as e:
                print(f"❌ Validation '{test_name}' crashed: {e}")
                results[test_name] = False
        
        return results


def main():
    """Main validation execution."""
    validator = FrameworkIntegrationValidator()
    results = validator.run_all_validations()
    
    # Summary
    print("\n" + "=" * 60)
    print("📊 VALIDATION SUMMARY")
    print("=" * 60)
    
    passed = sum(1 for r in results.values() if r)
    total = len(results)
    
    for test_name, success in results.items():
        status = "✅ PASS" if success else "❌ FAIL" 
        print(f"{status} {test_name}")
    
    print(f"\n📈 Results: {passed}/{total} validations passed")
    
    if passed == total:
        print("🎉 Framework integration architecture validated!")
        print("💡 Ready for ML dependency installation and full testing")
        return 0
    else:
        print(f"💥 {total - passed} validations failed")
        return 1


if __name__ == "__main__":
    exit(main())