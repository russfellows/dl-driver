#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2025 Russ Fellows <russ.fellows@gmail.com>
# SPDX-License-Identifier: GPL-3.0-or-later

"""
dl-driver Format Validation Test Suite

Comprehensive verification that dl-driver generated files match standard format specifications
and can be read correctly by standard Python libraries (numpy, h5py, tensorflow).

Based on s3dlio's verify_s3_objects.py patterns for format validation.
"""

import os
import sys
import struct
import zlib
import tempfile
import subprocess
from pathlib import Path
from typing import Dict, List, Optional

import numpy as np
import h5py
import tensorflow as tf
import zipfile
import io

# Color output for better readability
try:
    from termcolor import colored
    SUCCESS_ICON = colored('✅', 'green')
    ERROR_ICON = colored('❌', 'red')
    INFO_ICON = colored('📂', 'blue')
    WARNING_ICON = colored('⚠️', 'yellow')
except ImportError:
    SUCCESS_ICON = '✅'
    ERROR_ICON = '❌'
    INFO_ICON = '📂'
    WARNING_ICON = '⚠️'


class FormatValidator:
    """Validates dl-driver generated files against standard format specifications"""
    
    def __init__(self, test_data_dir: str):
        self.test_data_dir = Path(test_data_dir)
        self.errors = []
        self.successes = []
        
    def log_error(self, message: str):
        """Log validation error"""
        self.errors.append(message)
        print(f"{ERROR_ICON} {message}")
        
    def log_success(self, message: str):
        """Log validation success"""
        self.successes.append(message)
        print(f"{SUCCESS_ICON} {message}")
        
    def log_info(self, message: str):
        """Log informational message"""
        print(f"{INFO_ICON} {message}")

    def validate_npz_files(self) -> bool:
        """Validate NPZ files match numpy specification"""
        self.log_info("Validating NPZ format files...")
        
        npz_files = list(self.test_data_dir.glob("*.npz"))
        if not npz_files:
            self.log_error("No NPZ files found for validation")
            return False
            
        all_valid = True
        
        for npz_file in npz_files:
            self.log_info(f"Validating NPZ file: {npz_file.name}")
            
            try:
                # Test 1: Can numpy load it?
                npz_data = np.load(npz_file)
                
                # Check if it's a proper NPZ (zip archive) or single NPY file
                if hasattr(npz_data, 'keys'):
                    # Proper NPZ format (zip archive)
                    arrays = list(npz_data.keys())
                    if not arrays:
                        self.log_error(f"NPZ file {npz_file.name} contains no arrays")
                        all_valid = False
                        continue
                    
                    self.log_success(f"NPZ file {npz_file.name} loaded successfully with {len(arrays)} arrays")
                    
                    # Test 2: Check array structure
                    for array_name in arrays:
                        arr = npz_data[array_name]
                        self.log_info(f"  Array '{array_name}': shape={arr.shape}, dtype={arr.dtype}")
                        
                        # Verify data is not all zeros/corrupt
                        if arr.size > 0:
                            unique_vals = len(np.unique(arr.flat[:min(1000, arr.size)]))
                            if unique_vals > 1:
                                self.log_success(f"  Array '{array_name}' has diverse data ({unique_vals} unique values in sample)")
                            else:
                                self.log_error(f"  Array '{array_name}' appears to have uniform/corrupt data")
                                all_valid = False
                    
                    npz_data.close()
                else:
                    # Single NPY file (incorrectly named as .npz)
                    self.log_info(f"File {npz_file.name} is actually a single NPY array, not a proper NPZ")
                    arr = npz_data
                    self.log_info(f"  Array shape: {arr.shape}, dtype: {arr.dtype}")
                    
                    # Verify data structure
                    if arr.size > 0:
                        unique_vals = len(np.unique(arr.flat[:min(1000, arr.size)]))
                        if unique_vals > 1:
                            self.log_success(f"  Array has diverse data ({unique_vals} unique values in sample)")
                        else:
                            self.log_error(f"  Array appears to have uniform/corrupt data (all zeros)")
                            all_valid = False
                    
                    self.log_success(f"NPY file {npz_file.name} loaded successfully (but should be proper NPZ format)")
                        
                # Test 3: Verify ZIP structure
                with zipfile.ZipFile(npz_file, 'r') as zip_file:
                    zip_contents = zip_file.namelist()
                    self.log_success(f"NPZ ZIP structure valid with {len(zip_contents)} entries: {zip_contents}")
                    
                    # Check for .npy file inside
                    npy_files = [f for f in zip_contents if f.endswith('.npy')]
                    if npy_files:
                        self.log_success(f"Found NPY files inside NPZ: {npy_files}")
                    else:
                        self.log_error(f"No .npy files found in NPZ {npz_file.name}")
                        all_valid = False
                    
            except Exception as e:
                self.log_error(f"Failed to validate NPZ file {npz_file.name}: {e}")
                all_valid = False
                
        return all_valid

    def validate_hdf5_files(self) -> bool:
        """Validate HDF5 files match HDF5 specification"""
        self.log_info("Validating HDF5 format files...")
        
        hdf5_files = list(self.test_data_dir.glob("*.h5")) + list(self.test_data_dir.glob("*.hdf5"))
        if not hdf5_files:
            self.log_error("No HDF5 files found for validation")
            return False
            
        all_valid = True
        
        for hdf5_file in hdf5_files:
            self.log_info(f"Validating HDF5 file: {hdf5_file.name}")
            
            try:
                # Test 1: Check HDF5 signature
                with open(hdf5_file, 'rb') as f:
                    signature = f.read(8)
                    expected_signature = b"\x89HDF\r\n\x1a\n"
                    
                    if signature == expected_signature:
                        self.log_success(f"HDF5 signature valid for {hdf5_file.name}")
                    else:
                        self.log_error(f"Invalid HDF5 signature in {hdf5_file.name}: got {signature.hex()}, expected {expected_signature.hex()}")
                        all_valid = False
                        continue
                
                # Test 2: Can h5py open it?
                with h5py.File(hdf5_file, 'r') as h5f:
                    datasets = list(h5f.keys())
                    self.log_success(f"HDF5 file {hdf5_file.name} opened successfully with datasets: {datasets}")
                    
                    # Test 3: Validate datasets
                    for dataset_name in datasets:
                        dataset = h5f[dataset_name]
                        if hasattr(dataset, 'shape'):  # Check if it's actually a dataset
                            self.log_info(f"  Dataset '{dataset_name}': shape={dataset.shape}, dtype={dataset.dtype}")
                            
                            # Sample some data
                            if dataset.size > 0:
                                sample_size = min(1000, dataset.size)
                                if dataset.ndim == 1:
                                    sample = dataset[:sample_size]
                                else:
                                    # Convert to numpy array to get flat access
                                    sample_data = np.array(dataset)
                                    sample = sample_data.flat[:sample_size]
                                
                                unique_vals = len(np.unique(sample))
                                if unique_vals > 1:
                                    self.log_success(f"  Dataset '{dataset_name}' has diverse data ({unique_vals} unique values in sample)")
                                else:
                                    self.log_error(f"  Dataset '{dataset_name}' appears to have uniform/corrupt data")
                                    all_valid = False
                        else:
                            self.log_info(f"  Item '{dataset_name}' is not a dataset (likely a group)")
                    
            except Exception as e:
                self.log_error(f"Failed to validate HDF5 file {hdf5_file.name}: {e}")
                all_valid = False
                
        return all_valid

    def mask_crc(self, crc: int) -> int:
        """Apply TensorFlow's CRC masking (from TFRecord specification)"""
        return (((crc >> 15) | (crc << 17)) + 0xa282ead8) & 0xFFFFFFFF
    
    def unmask_crc(self, masked_crc: int) -> int:
        """Reverse TensorFlow's CRC masking"""
        unmasked = (masked_crc - 0xa282ead8) & 0xFFFFFFFF
        return ((unmasked << 15) | (unmasked >> 17)) & 0xFFFFFFFF

    def validate_tfrecord_files(self) -> bool:
        """Validate TFRecord files match TensorFlow specification"""
        self.log_info("Validating TFRecord format files...")
        
        tfrecord_files = list(self.test_data_dir.glob("*.tfrecord"))
        if not tfrecord_files:
            self.log_error("No TFRecord files found for validation")
            return False
            
        all_valid = True
        
        for tfrecord_file in tfrecord_files:
            self.log_info(f"Validating TFRecord file: {tfrecord_file.name}")
            
            try:
                # Test 1: Manual TFRecord parsing
                with open(tfrecord_file, 'rb') as f:
                    data = f.read()
                    
                cursor = 0
                record_count = 0
                
                while cursor < len(data):
                    # Read length (8 bytes)
                    if cursor + 8 > len(data):
                        break
                        
                    length_bytes = data[cursor:cursor + 8]
                    length = struct.unpack('<Q', length_bytes)[0]
                    cursor += 8
                    
                    # Read length CRC (4 bytes)
                    if cursor + 4 > len(data):
                        self.log_error(f"Incomplete length CRC at record {record_count}")
                        all_valid = False
                        break
                        
                    length_crc_bytes = data[cursor:cursor + 4]
                    length_crc = struct.unpack('<I', length_crc_bytes)[0]
                    cursor += 4
                    
                    # Verify length CRC (using CRC-32C like TensorFlow)
                    try:
                        import crc32c
                        computed_length_crc = crc32c.crc32c(length_bytes) & 0xFFFFFFFF
                    except ImportError:
                        # Fall back to zlib.crc32 if crc32c not available
                        computed_length_crc = zlib.crc32(length_bytes) & 0xFFFFFFFF
                    if self.unmask_crc(length_crc) != computed_length_crc:
                        self.log_error(f"Invalid length CRC at record {record_count}: {length_crc:x} vs {computed_length_crc:x}")
                        all_valid = False
                        break
                    
                    # Read data
                    if cursor + length > len(data):
                        self.log_error(f"Incomplete data at record {record_count}")
                        all_valid = False
                        break
                        
                    record_data = data[cursor:cursor + length]
                    cursor += length
                    
                    # Read data CRC (4 bytes)
                    if cursor + 4 > len(data):
                        self.log_error(f"Incomplete data CRC at record {record_count}")
                        all_valid = False
                        break
                        
                    data_crc_bytes = data[cursor:cursor + 4]
                    data_crc = struct.unpack('<I', data_crc_bytes)[0]
                    cursor += 4
                    
                    # Verify data CRC (using CRC-32C like TensorFlow)
                    try:
                        import crc32c
                        computed_data_crc = crc32c.crc32c(record_data) & 0xFFFFFFFF
                    except ImportError:
                        # Fall back to zlib.crc32 if crc32c not available
                        computed_data_crc = zlib.crc32(record_data) & 0xFFFFFFFF
                    if self.unmask_crc(data_crc) != computed_data_crc:
                        self.log_error(f"Invalid data CRC at record {record_count}: {data_crc:x} vs {computed_data_crc:x}")
                        all_valid = False
                        break
                    
                    record_count += 1
                    
                self.log_success(f"TFRecord {tfrecord_file.name} parsed successfully: {record_count} records")
                
                # Test 2: Try TensorFlow parsing
                try:
                    dataset = tf.data.TFRecordDataset(str(tfrecord_file))
                    tf_record_count = 0
                    for _ in dataset:
                        tf_record_count += 1
                    
                    if tf_record_count == record_count:
                        self.log_success(f"TensorFlow successfully parsed {tf_record_count} records from {tfrecord_file.name}")
                    else:
                        self.log_error(f"TensorFlow record count mismatch: manual={record_count}, tf={tf_record_count}")
                        all_valid = False
                        
                except Exception as tf_error:
                    self.log_error(f"TensorFlow failed to parse {tfrecord_file.name}: {tf_error}")
                    all_valid = False
                    
            except Exception as e:
                self.log_error(f"Failed to validate TFRecord file {tfrecord_file.name}: {e}")
                all_valid = False
                
        return all_valid

    def generate_test_data(self, format_type: str) -> bool:
        """Generate test data using dl-driver"""
        self.log_info(f"Generating {format_type} test data...")
        
        config_file = Path(__file__).parent / "test_configs" / f"{format_type}_validation_config.yaml"
        if not config_file.exists():
            self.log_error(f"Config file not found: {config_file}")
            return False
            
        try:
            # Clean up any existing test data
            if self.test_data_dir.exists():
                import shutil
                shutil.rmtree(self.test_data_dir)
            self.test_data_dir.mkdir(parents=True, exist_ok=True)
            
            # Run dl-driver to generate test data  
            project_root = Path(__file__).parent.parent.parent
            dl_driver_path = project_root / "target" / "release" / "dl-driver"
            cmd = [str(dl_driver_path), "generate", "--config", str(config_file)]
            result = subprocess.run(cmd, capture_output=True, text=True, cwd=project_root)
            
            if result.returncode == 0:
                self.log_success(f"Successfully generated {format_type} test data")
                return True
            else:
                self.log_error(f"Failed to generate {format_type} test data: {result.stderr}")
                return False
                
        except Exception as e:
            self.log_error(f"Error generating {format_type} test data: {e}")
            return False

    def validate_all_formats(self) -> Dict[str, bool]:
        """Validate all supported formats"""
        results = {}
        
        for format_type in ['npz', 'hdf5', 'tfrecord']:
            print(f"\n{'='*60}")
            print(f"TESTING {format_type.upper()} FORMAT")
            print(f"{'='*60}")
            
            # Generate test data
            if not self.generate_test_data(format_type):
                results[format_type] = False
                continue
            
            # Validate the generated files
            if format_type == 'npz':
                results[format_type] = self.validate_npz_files()
            elif format_type == 'hdf5':
                results[format_type] = self.validate_hdf5_files()
            elif format_type == 'tfrecord':
                results[format_type] = self.validate_tfrecord_files()
                
        return results
    
    def print_summary(self, results: Dict[str, bool]):
        """Print validation summary"""
        print(f"\n{'='*60}")
        print("VALIDATION SUMMARY")
        print(f"{'='*60}")
        
        all_passed = True
        for format_type, passed in results.items():
            status = SUCCESS_ICON if passed else ERROR_ICON
            print(f"{status} {format_type.upper()}: {'PASSED' if passed else 'FAILED'}")
            if not passed:
                all_passed = False
                
        print(f"\nTotal errors: {len(self.errors)}")
        print(f"Total successes: {len(self.successes)}")
        
        if all_passed:
            print(f"\n{SUCCESS_ICON} ALL FORMAT VALIDATIONS PASSED!")
            print("dl-driver generates files compatible with standard Python libraries.")
        else:
            print(f"\n{ERROR_ICON} SOME VALIDATIONS FAILED!")
            print("Review errors above and fix format implementations.")
            
        return all_passed


def main():
    """Main validation function"""
    test_data_dir = "/mnt/vast1/dl_driver_format_validation"
    validator = FormatValidator(test_data_dir)
    
    print("🔬 dl-driver Format Validation Test Suite")
    print("Verifying generated files work with standard Python libraries")
    print(f"Test data directory: {test_data_dir}")
    
    try:
        results = validator.validate_all_formats()
        success = validator.print_summary(results)
        return 0 if success else 1
        
    except KeyboardInterrupt:
        print(f"\n{WARNING_ICON} Validation interrupted by user")
        return 1
    except Exception as e:
        print(f"\n{ERROR_ICON} Validation failed with error: {e}")
        return 1


if __name__ == "__main__":
    sys.exit(main())