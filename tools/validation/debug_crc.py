#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2025 Russ Fellows <russ.fellows@gmail.com>
# SPDX-License-Identifier: GPL-3.0-or-later

"""
Debug CRC calculation differences between TensorFlow and our implementation
"""

import struct
import binascii

# Read the reference TFRecord and our TFRecord
with open('/tmp/reference.tfrecord', 'rb') as f:
    ref_data = f.read()

with open('/mnt/vast1/dlio_tfrecord_test/train_file_000000.tfrecord', 'rb') as f:
    our_data = f.read()

print("=== REFERENCE TFRECORD ===")
print(f"Length: {len(ref_data)} bytes")
length_ref = struct.unpack('<Q', ref_data[:8])[0]  # little-endian uint64
crc_len_ref = struct.unpack('<I', ref_data[8:12])[0]  # little-endian uint32
print(f"Record length: {length_ref}")
print(f"Length CRC: 0x{crc_len_ref:08x}")

# Extract data portion
data_ref = ref_data[12:12+length_ref]
crc_data_ref = struct.unpack('<I', ref_data[12+length_ref:12+length_ref+4])[0]
print(f"Data CRC: 0x{crc_data_ref:08x}")
print(f"Data bytes: {binascii.hexlify(data_ref[:20]).decode()}...")

print("\n=== OUR TFRECORD ===")
print(f"Length: {len(our_data)} bytes")
length_ours = struct.unpack('<Q', our_data[:8])[0]
crc_len_ours = struct.unpack('<I', our_data[8:12])[0]
print(f"Record length: {length_ours}")
print(f"Length CRC: 0x{crc_len_ours:08x}")

# Extract data portion
data_ours = our_data[12:12+length_ours]
crc_data_ours = struct.unpack('<I', our_data[12+length_ours:12+length_ours+4])[0]
print(f"Data CRC: 0x{crc_data_ours:08x}")
print(f"Data bytes: {binascii.hexlify(data_ours[:20]).decode()}...")

print(f"\nData identical: {data_ref == data_ours}")
print(f"Length identical: {length_ref == length_ours}")