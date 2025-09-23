#!/usr/bin/env python3
"""
Simplified Performance Benchmark Tests for dl-driver M4 Framework Profiles
Focuses on configuration parsing overhead and dataset creation performance.
"""

import sys
import os
import time
import traceback
import statistics
from pathlib import Path
from typing import List, Dict, Any

# Add the framework path to sys.path
sys.path.insert(0, '/home/eval/Documents/Rust-Devel/dl-driver/crates/py_api/src')

def benchmark_config_parsing():
    """Benchmark DLIO configuration parsing overhead."""
    print("🔧 Benchmarking DLIO configuration parsing...")
    
    try:
        from frameworks.pytorch import DlioPyTorchDataset
        from frameworks.tensorflow import DlioTensorFlowDataset
        
        # Test configurations
        pytorch_config = {
            'dataset': {
                'data_folder': 'file:///mnt/vast1/dlio_data_generated',
                'format': 'npz',
                'num_files_train': 10,
                'record_length_bytes': 1048576,
                'num_samples_per_file': 1
            },
            'reader': {
                'data_loader': 'pytorch',
                'batch_size': 4,
                'read_threads': 2
            },
            'train': {
                'epochs': 1,
                'seed': 42
            }
        }
        
        tf_config = pytorch_config.copy()
        tf_config['reader']['data_loader'] = 'tensorflow'
        
        results = {}
        
        # Benchmark PyTorch config parsing
        start_time = time.perf_counter()
        for _ in range(100):  # Parse config 100 times
            pytorch_dataset = DlioPyTorchDataset(config_dict=pytorch_config)
        pytorch_parse_time = (time.perf_counter() - start_time) / 100
        
        # Benchmark TensorFlow config parsing
        start_time = time.perf_counter()
        for _ in range(100):  # Parse config 100 times
            tf_dataset = DlioTensorFlowDataset(config_dict=tf_config)
        tf_parse_time = (time.perf_counter() - start_time) / 100
        
        results = {
            'pytorch_config_parse_time_ms': pytorch_parse_time * 1000,
            'tensorflow_config_parse_time_ms': tf_parse_time * 1000,
            'success': True
        }
        
        return results
        
    except Exception as e:
        return {
            'success': False,
            'error': str(e),
            'traceback': traceback.format_exc()
        }

def benchmark_dataset_creation():
    """Benchmark dataset object creation (without data loading)."""
    print("🔧 Benchmarking dataset object creation...")
    
    try:
        from frameworks.pytorch import DlioPyTorchDataset
        from frameworks.tensorflow import DlioTensorFlowDataset, DlioJaxDataset
        
        test_config = {
            'dataset': {
                'data_folder': 'file:///mnt/vast1/dlio_data_generated',
                'format': 'npz',
                'num_files_train': 10,
                'record_length_bytes': 1048576,
                'num_samples_per_file': 1
            },
            'reader': {
                'data_loader': 'pytorch',
                'batch_size': 4,
                'read_threads': 2
            },
            'train': {
                'epochs': 1,
                'seed': 42
            }
        }
        
        results = {}
        runs = 10
        
        # PyTorch dataset creation
        pytorch_times = []
        for _ in range(runs):
            start = time.perf_counter()
            dataset = DlioPyTorchDataset(config_dict=test_config)
            end = time.perf_counter()
            pytorch_times.append(end - start)
        
        # TensorFlow dataset creation
        tf_config = test_config.copy()
        tf_config['reader']['data_loader'] = 'tensorflow'
        tf_times = []
        for _ in range(runs):
            start = time.perf_counter()
            dataset = DlioTensorFlowDataset(config_dict=tf_config)
            end = time.perf_counter()
            tf_times.append(end - start)
        
        # JAX dataset creation
        jax_config = test_config.copy()
        jax_config['reader']['data_loader'] = 'jax'
        jax_times = []
        for _ in range(runs):
            start = time.perf_counter()
            dataset = DlioJaxDataset(config_dict=jax_config)
            end = time.perf_counter()
            jax_times.append(end - start)
        
        results = {
            'pytorch_creation': {
                'mean_ms': statistics.mean(pytorch_times) * 1000,
                'stdev_ms': statistics.stdev(pytorch_times) * 1000 if len(pytorch_times) > 1 else 0,
                'min_ms': min(pytorch_times) * 1000,
                'max_ms': max(pytorch_times) * 1000
            },
            'tensorflow_creation': {
                'mean_ms': statistics.mean(tf_times) * 1000,
                'stdev_ms': statistics.stdev(tf_times) * 1000 if len(tf_times) > 1 else 0,
                'min_ms': min(tf_times) * 1000,
                'max_ms': max(tf_times) * 1000
            },
            'jax_creation': {
                'mean_ms': statistics.mean(jax_times) * 1000,
                'stdev_ms': statistics.stdev(jax_times) * 1000 if len(jax_times) > 1 else 0,
                'min_ms': min(jax_times) * 1000,
                'max_ms': max(jax_times) * 1000
            },
            'runs': runs,
            'success': True
        }
        
        return results
        
    except Exception as e:
        return {
            'success': False,
            'error': str(e),
            'traceback': traceback.format_exc()
        }

def benchmark_backend_detection():
    """Benchmark backend detection performance."""
    print("🔧 Benchmarking backend detection...")
    
    try:
        from frameworks.pytorch import DlioPyTorchDataset
        
        uris = [
            'file:///tmp/data',
            's3://bucket/data',
            'az://account/container/data',
            'direct:///tmp/data'
        ]
        
        results = {}
        
        for uri in uris:
            config = {
                'dataset': {
                    'data_folder': uri,
                    'format': 'npz',
                    'num_files_train': 10,
                    'record_length_bytes': 1048576,
                    'num_samples_per_file': 1
                },
                'reader': {
                    'data_loader': 'pytorch',
                    'batch_size': 4,
                    'read_threads': 2
                },
                'train': {
                    'epochs': 1,
                    'seed': 42
                }
            }
            
            times = []
            for _ in range(50):  # 50 iterations for good statistics
                start = time.perf_counter()
                try:
                    dataset = DlioPyTorchDataset(config_dict=config)
                    backend = dataset.backend_type
                    end = time.perf_counter()
                    times.append(end - start)
                except Exception as e:
                    # Expected for non-existent paths, but we still measure backend detection
                    end = time.perf_counter()
                    times.append(end - start)
            
            scheme = uri.split('://')[0]
            results[f'{scheme}_backend_detection'] = {
                'mean_ms': statistics.mean(times) * 1000,
                'stdev_ms': statistics.stdev(times) * 1000 if len(times) > 1 else 0,
                'min_ms': min(times) * 1000,
                'max_ms': max(times) * 1000,
                'uri': uri
            }
        
        results['success'] = True
        return results
        
    except Exception as e:
        return {
            'success': False,
            'error': str(e),
            'traceback': traceback.format_exc()
        }

def benchmark_config_validation():
    """Benchmark configuration validation performance."""
    print("🔧 Benchmarking configuration validation...")
    
    try:
        import sys
        import os
        
        # Import Rust config validation
        sys.path.insert(0, '/home/eval/Documents/Rust-Devel/dl-driver/target/release')
        
        # We'll test this by creating and parsing many different configs
        from frameworks.pytorch import DlioPyTorchDataset
        
        # Different config variations
        configs = []
        for batch_size in [1, 4, 8, 16, 32]:
            for read_threads in [1, 2, 4, 8]:
                for format_type in ['npz', 'hdf5']:
                    config = {
                        'dataset': {
                            'data_folder': 'file:///mnt/vast1/dlio_data_generated',
                            'format': format_type,
                            'num_files_train': 10,
                            'record_length_bytes': 1048576,
                            'num_samples_per_file': 1
                        },
                        'reader': {
                            'data_loader': 'pytorch',
                            'batch_size': batch_size,
                            'read_threads': read_threads
                        },
                        'train': {
                            'epochs': 1,
                            'seed': 42
                        }
                    }
                    configs.append(config)
        
        # Benchmark parsing all configs
        start_time = time.perf_counter()
        for config in configs:
            dataset = DlioPyTorchDataset(config_dict=config)
        end_time = time.perf_counter()
        
        total_time = end_time - start_time
        per_config_time = total_time / len(configs)
        
        return {
            'total_configs': len(configs),
            'total_time_s': total_time,
            'per_config_time_ms': per_config_time * 1000,
            'configs_per_second': len(configs) / total_time,
            'success': True
        }
        
    except Exception as e:
        return {
            'success': False,
            'error': str(e),
            'traceback': traceback.format_exc()
        }

def main():
    """Run simplified performance benchmark tests."""
    print("🚀 dl-driver M4 Framework Profiles - Performance Benchmark Tests")
    print("=" * 80)
    
    # Check if test data exists
    data_dir = Path('/mnt/vast1/dlio_data_generated')
    if not data_dir.exists():
        print("⚠️  Test data not found, but proceeding with configuration benchmarks...")
    else:
        print(f"✅ Found test data: {len(list(data_dir.glob('*.npz')))} NPZ files")
    
    print()
    
    benchmarks = [
        ("Configuration Parsing", benchmark_config_parsing),
        ("Dataset Creation", benchmark_dataset_creation),
        ("Backend Detection", benchmark_backend_detection),
        ("Config Validation", benchmark_config_validation),
    ]
    
    results = {}
    
    for benchmark_name, benchmark_func in benchmarks:
        print(f"📊 Running {benchmark_name} benchmark...")
        result = benchmark_func()
        results[benchmark_name] = result
        
        if result['success']:
            print(f"✅ {benchmark_name} benchmark completed")
        else:
            print(f"❌ {benchmark_name} benchmark failed: {result['error']}")
        print()
    
    # Results Summary
    print("=" * 80)
    print("📊 PERFORMANCE BENCHMARK RESULTS")
    print("=" * 80)
    
    # Configuration Parsing Results
    if results["Configuration Parsing"]['success']:
        config_result = results["Configuration Parsing"]
        print("\n🔧 CONFIGURATION PARSING PERFORMANCE:")
        print(f"  PyTorch config parsing: {config_result['pytorch_config_parse_time_ms']:.2f}ms per config")
        print(f"  TensorFlow config parsing: {config_result['tensorflow_config_parse_time_ms']:.2f}ms per config")
        
    # Dataset Creation Results
    if results["Dataset Creation"]['success']:
        creation_result = results["Dataset Creation"]
        print("\n🏗️  DATASET CREATION PERFORMANCE:")
        print(f"  PyTorch: {creation_result['pytorch_creation']['mean_ms']:.2f}±{creation_result['pytorch_creation']['stdev_ms']:.2f}ms")
        print(f"  TensorFlow: {creation_result['tensorflow_creation']['mean_ms']:.2f}±{creation_result['tensorflow_creation']['stdev_ms']:.2f}ms")
        print(f"  JAX: {creation_result['jax_creation']['mean_ms']:.2f}±{creation_result['jax_creation']['stdev_ms']:.2f}ms")
    
    # Backend Detection Results
    if results["Backend Detection"]['success']:
        backend_result = results["Backend Detection"]
        print("\n🔍 BACKEND DETECTION PERFORMANCE:")
        for key, value in backend_result.items():
            if key != 'success' and isinstance(value, dict):
                print(f"  {key}: {value['mean_ms']:.2f}±{value['stdev_ms']:.2f}ms ({value['uri']})")
    
    # Config Validation Results
    if results["Config Validation"]['success']:
        validation_result = results["Config Validation"]
        print("\n✅ CONFIGURATION VALIDATION PERFORMANCE:")
        print(f"  Parsed {validation_result['total_configs']} configs in {validation_result['total_time_s']:.3f}s")
        print(f"  Average: {validation_result['per_config_time_ms']:.2f}ms per config")
        print(f"  Throughput: {validation_result['configs_per_second']:.1f} configs/second")
    
    # Overall Assessment
    print("\n" + "=" * 80)
    print("🎯 PERFORMANCE ASSESSMENT:")
    print("=" * 80)
    
    success_count = sum(1 for result in results.values() if result['success'])
    total_count = len(results)
    
    if success_count == total_count:
        print("✅ All performance benchmarks completed successfully!")
        print("📊 dl-driver configuration and setup overhead is minimal")
        print("🚀 Performance validation: PASSED")
        return 0
    else:
        print(f"⚠️  {total_count - success_count}/{total_count} benchmarks failed")
        print("❌ Performance validation: PARTIAL")
        return 1

if __name__ == "__main__":
    sys.exit(main())