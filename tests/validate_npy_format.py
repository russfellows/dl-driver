#!/usr/bin/env python3
"""
Validate that dl-driver's custom .npy serialization produces
format-compatible files that numpy can read correctly.
"""

import sys
import tempfile
import numpy as np
from pathlib import Path
import zipfile

def validate_npy_magic(npy_bytes):
    """Verify NPY magic number and version."""
    if len(npy_bytes) < 10:
        return False, f"File too short: {len(npy_bytes)} bytes"
    
    # Check magic
    if npy_bytes[0:6] != b'\x93NUMPY':
        return False, f"Invalid magic: {npy_bytes[0:6]}"
    
    # Check version
    if npy_bytes[6] != 1 or npy_bytes[7] != 0:
        return False, f"Invalid version: {npy_bytes[6]}.{npy_bytes[7]}"
    
    return True, "Magic and version correct"

def validate_npy_header(npy_bytes):
    """Parse and validate NPY header."""
    header_len = int.from_bytes(npy_bytes[8:10], 'little')
    
    if header_len == 0:
        return False, "Zero header length"
    
    if 10 + header_len > len(npy_bytes):
        return False, f"Header extends beyond data: {10 + header_len} > {len(npy_bytes)}"
    
    header = npy_bytes[10:10+header_len].decode('ascii')
    
    # Check for required keys
    if "'descr'" not in header:
        return False, "Missing 'descr' key in header"
    if "'fortran_order'" not in header:
        return False, "Missing 'fortran_order' key"
    if "'shape'" not in header:
        return False, "Missing 'shape' key"
    if not header.endswith('\n'):
        return False, "Header must end with newline"
    
    return True, f"Header valid ({header_len} bytes)"

def validate_numpy_readable(npy_path):
    """Try to load .npy file with numpy."""
    try:
        arr = np.load(npy_path)
        return True, f"Loaded array: shape={arr.shape}, dtype={arr.dtype}"
    except Exception as e:
        return False, f"numpy.load failed: {e}"

def validate_npz_file(npz_path):
    """Validate NPZ file structure and contents."""
    results = []
    
    # Check it's a valid ZIP
    try:
        with zipfile.ZipFile(npz_path, 'r') as zf:
            results.append(("ZIP structure", True, f"{len(zf.namelist())} entries"))
            
            # Validate each entry
            for name in zf.namelist():
                if not name.endswith('.npy'):
                    results.append((f"Entry {name}", False, "Not a .npy file"))
                    continue
                
                # Extract and validate
                npy_bytes = zf.read(name)
                
                # Check magic
                ok, msg = validate_npy_magic(npy_bytes)
                results.append((f"{name} magic", ok, msg))
                
                # Check header
                ok, msg = validate_npy_header(npy_bytes)
                results.append((f"{name} header", ok, msg))
                
                # Try loading with numpy
                with tempfile.NamedTemporaryFile(suffix='.npy', delete=False) as tmp:
                    tmp.write(npy_bytes)
                    tmp_path = tmp.name
                
                try:
                    ok, msg = validate_numpy_readable(tmp_path)
                    results.append((f"{name} numpy", ok, msg))
                finally:
                    Path(tmp_path).unlink()
    
    except zipfile.BadZipFile as e:
        results.append(("ZIP structure", False, f"Invalid ZIP: {e}"))
    except Exception as e:
        results.append(("ZIP reading", False, f"Error: {e}"))
    
    return results

def main():
    if len(sys.argv) < 2:
        print("Usage: validate_npy_format.py <npz_file>")
        print("Validates that NPZ files produced by dl-driver are numpy-compatible")
        return 1
    
    npz_file = Path(sys.argv[1])
    
    if not npz_file.exists():
        print(f"Error: File not found: {npz_file}")
        return 1
    
    print(f"Validating: {npz_file}")
    print("=" * 60)
    
    results = validate_npz_file(npz_file)
    
    # Print results
    all_passed = True
    for test_name, passed, message in results:
        status = "✓ PASS" if passed else "✗ FAIL"
        print(f"{status:8} | {test_name:30} | {message}")
        if not passed:
            all_passed = False
    
    print("=" * 60)
    
    if all_passed:
        print("SUCCESS: All validation checks passed!")
        return 0
    else:
        print("FAILURE: Some validation checks failed")
        return 1

if __name__ == "__main__":
    sys.exit(main())
