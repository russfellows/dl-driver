#!/usr/bin/env python3
"""
Create a reference TFRecord file using TensorFlow's Python API
to compare with our Rust implementation
"""

import tensorflow as tf
import numpy as np

def create_reference_tfrecord():
    """Create a reference TFRecord file using TensorFlow's API"""
    
    # Create some sample data similar to what our Rust code generates
    image_data = np.array([149, 154, 76, 194, 162, 165, 7, 60, 100, 200, 50, 75, 25, 128, 255, 0], dtype=np.float32) / 255.0
    
    # Create tf.train.Example
    example = tf.train.Example(features=tf.train.Features(feature={
        'image': tf.train.Feature(float_list=tf.train.FloatList(value=image_data))
    }))
    
    # Write to file
    with tf.io.TFRecordWriter('/tmp/reference.tfrecord') as writer:
        writer.write(example.SerializeToString())
    
    print(f"Created reference TFRecord with {len(image_data)} float values")
    
    # Read it back to verify
    dataset = tf.data.TFRecordDataset('/tmp/reference.tfrecord')
    for raw_record in dataset.take(1):
        example = tf.train.Example()
        example.ParseFromString(raw_record.numpy())
        print(f"Verified: {len(example.features.feature['image'].float_list.value)} floats")
        print(f"First few values: {list(example.features.feature['image'].float_list.value[:5])}")

if __name__ == "__main__":
    create_reference_tfrecord()