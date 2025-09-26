#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2025 Russ Fellows <russ.fellows@gmail.com>
# SPDX-License-Identifier: GPL-3.0-or-later

"""
Performance Benchmark Tests for dl-driver M4 Framework Profiles
Compares dl-driver framework integrations against pure s3dlio usage.
Measures overhead, throughput, and memory usage.
"""

import sys
import os
import time
import traceback
import tracemalloc
import statistics
import numpy as np
from pathlib import Path
from typing import List, Dict, Any, Tuple

# Add the framework path to sys.path
sys.path.insert(0, '/home/eval/Documents/Rust-Devel/dl-driver/crates/py_api/src')

def benchmark_function(func, *args, **kwargs) -> Tuple[float, float, Any]:
    """
    Benchmark a function measuring execution time and memory usage.
    
    Returns:
        Tuple of (execution_time_seconds, peak_memory_mb, result)
    """
    # Start memory tracking
    tracemalloc.start()
    
    # Measure execution time
    start_time = time.perf_counter()
    result = func(*args, **kwargs)
    end_time = time.perf_counter()
    
    # Get memory usage
    current, peak = tracemalloc.get_traced_memory()
    tracemalloc.stop()
    
    execution_time = end_time - start_time
    peak_memory_mb = peak / 1024 / 1024  # Convert bytes to MB
    
    return execution_time, peak_memory_mb, result

def benchmark_pure_s3dlio_pytorch() -> Dict[str, Any]:
    """Benchmark pure s3dlio PyTorch integration."""
    print("🔧 Benchmarking pure s3dlio PyTorch...")
    
    try:
        import torch
        from s3dlio.torch import S3IterableDataset
        
        def create_dataset():
            data_folder = "/mnt/vast1/dlio_data_generated"
            uri = f"file://{data_folder}"
            loader_opts = {"file_pattern": "*.npz", "shuffle": True, "seed": 42}
            dataset = S3IterableDataset(uri, loader_opts=loader_opts)
            return dataset
        
        def iterate_dataset(dataset, max_items=5):
            """Iterate through dataset items (limited to avoid long benchmarks)."""
            items = []
            for i, item in enumerate(dataset):
                items.append(item)
                if i >= max_items - 1:
                    break
            return items
        
        # Benchmark dataset creation
        creation_time, creation_memory, dataset = benchmark_function(create_dataset)
        
        # Benchmark iteration
        iteration_time, iteration_memory, items = benchmark_function(iterate_dataset, dataset, 3)
        
        return {
            'framework': 'pure_s3dlio_pytorch',
            'creation_time_s': creation_time,
            'creation_memory_mb': creation_memory,
            'iteration_time_s': iteration_time,
            'iteration_memory_mb': iteration_memory,
            'items_loaded': len(items),
            'success': True,
            'error': None
        }
        
    except Exception as e:
        return {
            'framework': 'pure_s3dlio_pytorch',
            'success': False,
            'error': str(e),
            'traceback': traceback.format_exc()
        }

def benchmark_dldriver_pytorch() -> Dict[str, Any]:
    """Benchmark dl-driver PyTorch integration."""
    print("🔧 Benchmarking dl-driver PyTorch...")
    
    try:
        import torch
        from frameworks.pytorch import DlioPyTorchDataset
        
        def create_dataset():
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
            dataset = DlioPyTorchDataset(config_dict=test_config)
            return dataset
        
        def iterate_dataset(dataset, max_items=5):
            """Iterate through dataset items (limited to avoid long benchmarks)."""
            items = []
            for i, item in enumerate(dataset):
                items.append(item)
                if i >= max_items - 1:
                    break
            return items
        
        # Benchmark dataset creation
        creation_time, creation_memory, dataset = benchmark_function(create_dataset)
        
        # Benchmark iteration
        iteration_time, iteration_memory, items = benchmark_function(iterate_dataset, dataset, 3)
        
        return {
            'framework': 'dldriver_pytorch',
            'creation_time_s': creation_time,
            'creation_memory_mb': creation_memory,
            'iteration_time_s': iteration_time,
            'iteration_memory_mb': iteration_memory,
            'items_loaded': len(items),
            'success': True,
            'error': None
        }
        
    except Exception as e:
        return {
            'framework': 'dldriver_pytorch',
            'success': False,
            'error': str(e),
            'traceback': traceback.format_exc()
        }

def benchmark_pure_s3dlio_tensorflow() -> Dict[str, Any]:
    """Benchmark pure s3dlio TensorFlow integration."""
    print("🔧 Benchmarking pure s3dlio TensorFlow...")
    
    try:
        import tensorflow as tf
        from s3dlio.jax_tf import make_tf_dataset
        
        def create_dataset():
            data_folder = "/mnt/vast1/dlio_data_generated"
            uri = f"file://{data_folder}"
            dataset = make_tf_dataset(uri, shuffle=True, seed=42, batch_size=2)
            return dataset
        
        def iterate_dataset(dataset, max_items=3):
            """Iterate through dataset items (limited)."""
            items = []
            for i, item in enumerate(dataset.take(max_items)):
                items.append(item)
            return items
        
        # Benchmark dataset creation
        creation_time, creation_memory, dataset = benchmark_function(create_dataset)
        
        # Benchmark iteration
        iteration_time, iteration_memory, items = benchmark_function(iterate_dataset, dataset, 3)
        
        return {
            'framework': 'pure_s3dlio_tensorflow',
            'creation_time_s': creation_time,
            'creation_memory_mb': creation_memory,
            'iteration_time_s': iteration_time,
            'iteration_memory_mb': iteration_memory,
            'items_loaded': len(items),
            'success': True,
            'error': None
        }
        
    except Exception as e:
        return {
            'framework': 'pure_s3dlio_tensorflow',
            'success': False,
            'error': str(e),
            'traceback': traceback.format_exc()
        }

def benchmark_dldriver_tensorflow() -> Dict[str, Any]:
    """Benchmark dl-driver TensorFlow integration."""
    print("🔧 Benchmarking dl-driver TensorFlow...")
    
    try:
        import tensorflow as tf
        from frameworks.tensorflow import DlioTensorFlowDataset
        
        def create_dataset():
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
            tf_dataset_wrapper = DlioTensorFlowDataset(config_dict=test_config)
            dataset = tf_dataset_wrapper.create_dataset()
            return dataset
        
        def iterate_dataset(dataset, max_items=3):
            """Iterate through dataset items (limited)."""
            items = []
            for i, item in enumerate(dataset.take(max_items)):
                items.append(item)
            return items
        
        # Benchmark dataset creation
        creation_time, creation_memory, dataset = benchmark_function(create_dataset)
        
        # Benchmark iteration
        iteration_time, iteration_memory, items = benchmark_function(iterate_dataset, dataset, 3)
        
        return {
            'framework': 'dldriver_tensorflow',
            'creation_time_s': creation_time,
            'creation_memory_mb': creation_memory,
            'iteration_time_s': iteration_time,
            'iteration_memory_mb': iteration_memory,
            'items_loaded': len(items),
            'success': True,
            'error': None
        }
        
    except Exception as e:
        return {
            'framework': 'dldriver_tensorflow',
            'success': False,
            'error': str(e),
            'traceback': traceback.format_exc()
        }

def calculate_overhead(pure_result: Dict[str, Any], dldriver_result: Dict[str, Any]) -> Dict[str, Any]:
    """Calculate overhead of dl-driver vs pure s3dlio."""
    if not (pure_result['success'] and dldriver_result['success']):
        return {'error': 'One or both benchmarks failed'}
    
    creation_overhead = ((dldriver_result['creation_time_s'] - pure_result['creation_time_s']) / pure_result['creation_time_s']) * 100
    iteration_overhead = ((dldriver_result['iteration_time_s'] - pure_result['iteration_time_s']) / pure_result['iteration_time_s']) * 100
    
    memory_overhead_creation = dldriver_result['creation_memory_mb'] - pure_result['creation_memory_mb']
    memory_overhead_iteration = dldriver_result['iteration_memory_mb'] - pure_result['iteration_memory_mb']
    
    return {
        'creation_time_overhead_percent': creation_overhead,
        'iteration_time_overhead_percent': iteration_overhead,
        'memory_overhead_creation_mb': memory_overhead_creation,
        'memory_overhead_iteration_mb': memory_overhead_iteration,
        'pure_creation_time_s': pure_result['creation_time_s'],
        'dldriver_creation_time_s': dldriver_result['creation_time_s'],
        'pure_iteration_time_s': pure_result['iteration_time_s'],
        'dldriver_iteration_time_s': dldriver_result['iteration_time_s']
    }

def run_multiple_benchmarks(benchmark_func, runs=3) -> List[Dict[str, Any]]:
    """Run benchmark multiple times and return results."""
    results = []
    for i in range(runs):
        print(f"   Run {i+1}/{runs}...")
        result = benchmark_func()
        results.append(result)
        if not result['success']:
            break
    return results

def aggregate_benchmark_results(results: List[Dict[str, Any]]) -> Dict[str, Any]:
    """Aggregate multiple benchmark runs."""
    if not results or not all(r['success'] for r in results):
        return {'success': False, 'error': 'Some benchmarks failed'}
    
    creation_times = [r['creation_time_s'] for r in results]
    iteration_times = [r['iteration_time_s'] for r in results]
    creation_memory = [r['creation_memory_mb'] for r in results]
    iteration_memory = [r['iteration_memory_mb'] for r in results]
    
    return {
        'success': True,
        'runs': len(results),
        'creation_time_s': {
            'mean': statistics.mean(creation_times),
            'stdev': statistics.stdev(creation_times) if len(creation_times) > 1 else 0,
            'min': min(creation_times),
            'max': max(creation_times)
        },
        'iteration_time_s': {
            'mean': statistics.mean(iteration_times),
            'stdev': statistics.stdev(iteration_times) if len(iteration_times) > 1 else 0,
            'min': min(iteration_times),
            'max': max(iteration_times)
        },
        'creation_memory_mb': {
            'mean': statistics.mean(creation_memory),
            'stdev': statistics.stdev(creation_memory) if len(creation_memory) > 1 else 0,
            'min': min(creation_memory),
            'max': max(creation_memory)
        },
        'iteration_memory_mb': {
            'mean': statistics.mean(iteration_memory),
            'stdev': statistics.stdev(iteration_memory) if len(iteration_memory) > 1 else 0,
            'min': min(iteration_memory),
            'max': max(iteration_memory)
        }
    }

def main():
    """Run comprehensive performance benchmark tests."""
    print("🚀 dl-driver M4 Framework Profiles - Performance Benchmark Tests")
    print("=" * 80)
    
    # Check if test data exists
    data_dir = Path('/mnt/vast1/dlio_data_generated')
    if not data_dir.exists() or not list(data_dir.glob('*.npz')):
        print("❌ Test data not found. Please generate test data first:")
        print("   ./target/release/dl-driver generate --config test_data_generation_config.yaml")
        return 1
    
    print(f"✅ Found test data: {len(list(data_dir.glob('*.npz')))} NPZ files")
    print()
    
    # PyTorch Benchmarks
    print("🔥 PYTORCH PERFORMANCE BENCHMARKS")
    print("-" * 50)
    
    print("📊 Running pure s3dlio PyTorch benchmarks (3 runs)...")
    pure_pytorch_results = run_multiple_benchmarks(benchmark_pure_s3dlio_pytorch, 3)
    pure_pytorch_agg = aggregate_benchmark_results(pure_pytorch_results)
    
    print("📊 Running dl-driver PyTorch benchmarks (3 runs)...")
    dldriver_pytorch_results = run_multiple_benchmarks(benchmark_dldriver_pytorch, 3)
    dldriver_pytorch_agg = aggregate_benchmark_results(dldriver_pytorch_results)
    
    # TensorFlow Benchmarks
    print("\n🔥 TENSORFLOW PERFORMANCE BENCHMARKS")
    print("-" * 50)
    
    print("📊 Running pure s3dlio TensorFlow benchmarks (3 runs)...")
    pure_tf_results = run_multiple_benchmarks(benchmark_pure_s3dlio_tensorflow, 3)
    pure_tf_agg = aggregate_benchmark_results(pure_tf_results)
    
    print("📊 Running dl-driver TensorFlow benchmarks (3 runs)...")
    dldriver_tf_results = run_multiple_benchmarks(benchmark_dldriver_tensorflow, 3)
    dldriver_tf_agg = aggregate_benchmark_results(dldriver_tf_results)
    
    # Results Summary
    print("\n" + "=" * 80)
    print("📊 PERFORMANCE BENCHMARK RESULTS")
    print("=" * 80)
    
    # PyTorch Results
    print("\n🔥 PYTORCH PERFORMANCE:")
    if pure_pytorch_agg['success'] and dldriver_pytorch_agg['success']:
        print("Pure s3dlio PyTorch:")
        print(f"  Creation: {pure_pytorch_agg['creation_time_s']['mean']:.4f}s (±{pure_pytorch_agg['creation_time_s']['stdev']:.4f}s)")
        print(f"  Iteration: {pure_pytorch_agg['iteration_time_s']['mean']:.4f}s (±{pure_pytorch_agg['iteration_time_s']['stdev']:.4f}s)")
        print(f"  Memory: {pure_pytorch_agg['creation_memory_mb']['mean']:.2f}MB creation, {pure_pytorch_agg['iteration_memory_mb']['mean']:.2f}MB iteration")
        
        print("\ndl-driver PyTorch:")
        print(f"  Creation: {dldriver_pytorch_agg['creation_time_s']['mean']:.4f}s (±{dldriver_pytorch_agg['creation_time_s']['stdev']:.4f}s)")
        print(f"  Iteration: {dldriver_pytorch_agg['iteration_time_s']['mean']:.4f}s (±{dldriver_pytorch_agg['iteration_time_s']['stdev']:.4f}s)")
        print(f"  Memory: {dldriver_pytorch_agg['creation_memory_mb']['mean']:.2f}MB creation, {dldriver_pytorch_agg['iteration_memory_mb']['mean']:.2f}MB iteration")
        
        # Calculate overhead
        overhead_pytorch = calculate_overhead(
            {k: v['mean'] if isinstance(v, dict) else v for k, v in pure_pytorch_agg.items()},
            {k: v['mean'] if isinstance(v, dict) else v for k, v in dldriver_pytorch_agg.items()}
        )
        
        print("\nPyTorch Overhead Analysis:")
        print(f"  Creation time overhead: {overhead_pytorch['creation_time_overhead_percent']:+.1f}%")
        print(f"  Iteration time overhead: {overhead_pytorch['iteration_time_overhead_percent']:+.1f}%")
        print(f"  Memory overhead: {overhead_pytorch['memory_overhead_creation_mb']:+.1f}MB creation, {overhead_pytorch['memory_overhead_iteration_mb']:+.1f}MB iteration")
    else:
        print("❌ PyTorch benchmarks failed")
    
    # TensorFlow Results  
    print("\n🔥 TENSORFLOW PERFORMANCE:")
    if pure_tf_agg['success'] and dldriver_tf_agg['success']:
        print("Pure s3dlio TensorFlow:")
        print(f"  Creation: {pure_tf_agg['creation_time_s']['mean']:.4f}s (±{pure_tf_agg['creation_time_s']['stdev']:.4f}s)")
        print(f"  Iteration: {pure_tf_agg['iteration_time_s']['mean']:.4f}s (±{pure_tf_agg['iteration_time_s']['stdev']:.4f}s)")
        print(f"  Memory: {pure_tf_agg['creation_memory_mb']['mean']:.2f}MB creation, {pure_tf_agg['iteration_memory_mb']['mean']:.2f}MB iteration")
        
        print("\ndl-driver TensorFlow:")
        print(f"  Creation: {dldriver_tf_agg['creation_time_s']['mean']:.4f}s (±{dldriver_tf_agg['creation_time_s']['stdev']:.4f}s)")
        print(f"  Iteration: {dldriver_tf_agg['iteration_time_s']['mean']:.4f}s (±{dldriver_tf_agg['iteration_time_s']['stdev']:.4f}s)")
        print(f"  Memory: {dldriver_tf_agg['creation_memory_mb']['mean']:.2f}MB creation, {dldriver_tf_agg['iteration_memory_mb']['mean']:.2f}MB iteration")
        
        # Calculate overhead
        overhead_tf = calculate_overhead(
            {k: v['mean'] if isinstance(v, dict) else v for k, v in pure_tf_agg.items()},
            {k: v['mean'] if isinstance(v, dict) else v for k, v in dldriver_tf_agg.items()}
        )
        
        print("\nTensorFlow Overhead Analysis:")
        print(f"  Creation time overhead: {overhead_tf['creation_time_overhead_percent']:+.1f}%")
        print(f"  Iteration time overhead: {overhead_tf['iteration_time_overhead_percent']:+.1f}%")
        print(f"  Memory overhead: {overhead_tf['memory_overhead_creation_mb']:+.1f}MB creation, {overhead_tf['memory_overhead_iteration_mb']:+.1f}MB iteration")
    else:
        print("❌ TensorFlow benchmarks failed")
    
    # Overall Assessment
    print("\n" + "=" * 80)
    print("🎯 PERFORMANCE ASSESSMENT:")
    print("=" * 80)
    
    success_count = sum([
        pure_pytorch_agg['success'],
        dldriver_pytorch_agg['success'], 
        pure_tf_agg['success'],
        dldriver_tf_agg['success']
    ])
    
    if success_count == 4:
        print("✅ All benchmarks completed successfully!")
        print("📊 dl-driver wrapper overhead analysis complete")
        print("🚀 Performance validation: PASSED")
        return 0
    else:
        print(f"⚠️  {4-success_count}/4 benchmarks failed")
        print("❌ Performance validation: INCOMPLETE")
        return 1

if __name__ == "__main__":
    sys.exit(main())