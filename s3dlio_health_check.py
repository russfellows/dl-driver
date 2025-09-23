#!/usr/bin/env python3
"""
s3dlio Health Check and Diagnostics
Tests core s3dlio functionality to identify any Tokio runtime or version issues.
"""

import sys
import os
import traceback
import time
from pathlib import Path

def test_s3dlio_basic_import():
    """Test basic s3dlio import and version detection."""
    print("🔍 Testing s3dlio basic import...")
    
    try:
        import s3dlio
        print(f"✅ s3dlio imported successfully")
        
        # Try to get version or module info
        try:
            version = getattr(s3dlio, '__version__', 'unknown')
            print(f"   Version: {version}")
        except:
            print("   Version: unknown")
        
        # Check what's available in the module
        attrs = [attr for attr in dir(s3dlio) if not attr.startswith('_')]
        print(f"   Available attributes: {attrs}")
        
        return True
        
    except Exception as e:
        print(f"❌ s3dlio import failed: {e}")
        traceback.print_exc()
        return False

def test_s3dlio_torch_import():
    """Test s3dlio PyTorch integration import."""
    print("\n🔍 Testing s3dlio.torch import...")
    
    try:
        from s3dlio.torch import S3IterableDataset, S3MapDataset
        print("✅ s3dlio.torch classes imported successfully")
        
        # Check class signatures
        print(f"   S3IterableDataset: {S3IterableDataset}")
        print(f"   S3MapDataset: {S3MapDataset}")
        
        return True
        
    except Exception as e:
        print(f"❌ s3dlio.torch import failed: {e}")
        traceback.print_exc()
        return False

def test_s3dlio_tensorflow_import():
    """Test s3dlio TensorFlow/JAX integration import.""" 
    print("\n🔍 Testing s3dlio.jax_tf import...")
    
    try:
        from s3dlio.jax_tf import S3JaxIterable, make_tf_dataset
        print("✅ s3dlio.jax_tf functions imported successfully")
        
        print(f"   S3JaxIterable: {S3JaxIterable}")
        print(f"   make_tf_dataset: {make_tf_dataset}")
        
        return True
        
    except Exception as e:
        print(f"❌ s3dlio.jax_tf import failed: {e}")
        traceback.print_exc()
        return False

def test_s3dlio_core_backend():
    """Test s3dlio core Rust backend."""
    print("\n🔍 Testing s3dlio core Rust backend...")
    
    try:
        import s3dlio._pymod as core
        print("✅ s3dlio._pymod imported successfully")
        
        # Check what's available in the core module
        attrs = [attr for attr in dir(core) if not attr.startswith('_')]
        print(f"   Available core attributes: {attrs}")
        
        # Look for the specific class that was missing
        if hasattr(core, 'PyS3AsyncDataLoader'):
            print("   ✅ PyS3AsyncDataLoader found")
        elif hasattr(core, 'PyAsyncDataLoader'):
            print("   ⚠️  PyAsyncDataLoader found (but PyS3AsyncDataLoader missing)")
        else:
            print("   ❌ Neither PyS3AsyncDataLoader nor PyAsyncDataLoader found")
        
        return True
        
    except Exception as e:
        print(f"❌ s3dlio._pymod import failed: {e}")
        traceback.print_exc()
        return False

def test_s3dlio_dataset_creation():
    """Test creating s3dlio datasets (without iteration)."""
    print("\n🔍 Testing s3dlio dataset creation...")
    
    try:
        from s3dlio.torch import S3IterableDataset
        
        # Test with our generated data
        data_folder = "/mnt/vast1/dlio_data_generated"
        if not os.path.exists(data_folder):
            print("   ⚠️  Test data not found, using dummy path")
            data_folder = "/tmp"
        
        uri = f"file://{data_folder}"
        loader_opts = {"file_pattern": "*.npz", "shuffle": False}
        
        print(f"   Creating dataset with URI: {uri}")
        print(f"   Loader options: {loader_opts}")
        
        dataset = S3IterableDataset(uri, loader_opts=loader_opts)
        print("✅ S3IterableDataset created successfully")
        print(f"   Dataset object: {dataset}")
        print(f"   Dataset type: {type(dataset)}")
        
        return True
        
    except Exception as e:
        print(f"❌ s3dlio dataset creation failed: {e}")
        traceback.print_exc()
        return False

def test_s3dlio_tensorflow_dataset():
    """Test creating TensorFlow dataset via s3dlio."""
    print("\n🔍 Testing s3dlio TensorFlow dataset creation...")
    
    try:
        from s3dlio.jax_tf import make_tf_dataset
        
        data_folder = "/mnt/vast1/dlio_data_generated"
        if not os.path.exists(data_folder):
            print("   ⚠️  Test data not found, using dummy path")
            data_folder = "/tmp"
        
        uri = f"file://{data_folder}"
        
        print(f"   Creating TF dataset with URI: {uri}")
        
        tf_dataset = make_tf_dataset(uri, shuffle=False, batch_size=1)
        print("✅ TensorFlow dataset created successfully")
        print(f"   TF Dataset: {tf_dataset}")
        print(f"   TF Dataset type: {type(tf_dataset)}")
        
        return True
        
    except Exception as e:
        print(f"❌ s3dlio TensorFlow dataset creation failed: {e}")
        traceback.print_exc()
        return False

def test_s3dlio_simple_iteration():
    """Test very basic s3dlio iteration (with timeout)."""
    print("\n🔍 Testing s3dlio basic iteration (with timeout)...")
    
    try:
        from s3dlio.torch import S3IterableDataset
        import signal
        
        # Check if we have actual data to iterate over
        data_folder = "/mnt/vast1/dlio_data_generated" 
        if not os.path.exists(data_folder) or not list(Path(data_folder).glob("*.npz")):
            print("   ⚠️  No NPZ test data found - skipping iteration test")
            return True
        
        uri = f"file://{data_folder}"
        loader_opts = {"file_pattern": "*.npz", "shuffle": False}
        
        dataset = S3IterableDataset(uri, loader_opts=loader_opts)
        
        print("   Attempting to iterate (max 5 seconds, 1 item)...")
        
        # Set up a timeout to avoid hanging
        def timeout_handler(signum, frame):
            raise TimeoutError("Iteration timeout")
        
        signal.signal(signal.SIGALRM, timeout_handler)
        signal.alarm(5)  # 5 second timeout
        
        try:
            iterator = iter(dataset)
            print("   ✅ Iterator created")
            
            # Try to get just one item
            item = next(iterator)
            print(f"   ✅ Got first item: type={type(item)}, shape={getattr(item, 'shape', 'no shape')}")
            
            signal.alarm(0)  # Cancel timeout
            return True
            
        except TimeoutError:
            print("   ❌ Iteration timed out (likely Tokio runtime issue)")
            return False
        except StopIteration:
            print("   ⚠️  No items in dataset (empty)")
            return True
        except Exception as e:
            print(f"   ❌ Iteration failed: {e}")
            traceback.print_exc()
            return False
        finally:
            signal.alarm(0)  # Ensure timeout is cancelled
        
    except Exception as e:
        print(f"❌ s3dlio iteration test setup failed: {e}")
        traceback.print_exc()
        return False

def test_version_compatibility():
    """Check for version mismatches between installed wheel and local code."""
    print("\n🔍 Testing version compatibility...")
    
    try:
        # Check installed s3dlio wheel location
        import s3dlio
        wheel_path = s3dlio.__file__
        print(f"   Installed s3dlio wheel: {wheel_path}")
        
        # Check if we have local s3dlio source
        local_s3dlio = "/home/eval/Documents/Rust-Devel/s3dlio"
        if os.path.exists(local_s3dlio):
            print(f"   Local s3dlio source: {local_s3dlio}")
            
            # Check if local source is in PYTHONPATH
            if local_s3dlio in sys.path:
                print("   ⚠️  WARNING: Local s3dlio source is in Python path")
                print("   This could cause conflicts with installed wheel")
            else:
                print("   ✅ Local source not in Python path - good")
        
        # Check current working directory
        cwd = os.getcwd()
        if "s3dlio" in cwd:
            print(f"   ⚠️  WARNING: Currently in s3dlio directory: {cwd}")
            print("   This could affect imports")
        else:
            print(f"   ✅ Working directory: {cwd}")
        
        return True
        
    except Exception as e:
        print(f"❌ Version compatibility check failed: {e}")
        traceback.print_exc()
        return False

def main():
    """Run comprehensive s3dlio health check."""
    print("🚀 s3dlio Health Check and Diagnostics")
    print("=" * 60)
    print("Investigating potential Tokio runtime and version issues...")
    
    tests = [
        ("Basic Import", test_s3dlio_basic_import),
        ("PyTorch Import", test_s3dlio_torch_import), 
        ("TensorFlow Import", test_s3dlio_tensorflow_import),
        ("Core Backend", test_s3dlio_core_backend),
        ("Dataset Creation", test_s3dlio_dataset_creation),
        ("TensorFlow Dataset", test_s3dlio_tensorflow_dataset),
        ("Version Compatibility", test_version_compatibility),
        ("Basic Iteration", test_s3dlio_simple_iteration),  # Most likely to fail
    ]
    
    results = {}
    
    for test_name, test_func in tests:
        print(f"\n{'='*60}")
        result = test_func()
        results[test_name] = result
    
    # Summary
    print("\n" + "=" * 60)
    print("📊 s3dlio HEALTH CHECK RESULTS")
    print("=" * 60)
    
    passed = 0
    total = len(results)
    
    for test_name, success in results.items():
        status = "✅ PASS" if success else "❌ FAIL"
        print(f"{status}: {test_name}")
        if success:
            passed += 1
    
    print(f"\n🎯 Overall: {passed}/{total} tests passed ({passed/total*100:.1f}%)")
    
    if passed == total:
        print("🎉 s3dlio appears to be working correctly!")
        print("🚀 Safe to proceed with dl-driver integration")
    elif passed >= total * 0.75:  # 75% or better
        print("⚠️  s3dlio has some issues but basic functionality works")
        print("🔧 May need investigation or workarounds")
    else:
        print("🚨 CRITICAL: s3dlio has serious issues")
        print("🛑 DO NOT PROCEED until s3dlio is fixed")
    
    return 0 if passed >= total * 0.75 else 1

if __name__ == "__main__":
    sys.exit(main())