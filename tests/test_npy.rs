use ndarray::{ArrayD, IxDyn};
use ndarray_npy::{write_npy};
use std::io::Cursor;

fn main() {
    let data: Vec<f32> = vec![1.0, 2.0, 3.0, 4.0];
    let array = ArrayD::from_shape_vec(IxDyn(&[2, 2]), data).unwrap();
    
    // Try writing to cursor
    let mut buffer = Vec::new();
    let mut cursor = Cursor::new(&mut buffer);
    
    // This should work if we can write to a cursor
    write_npy(&mut cursor, &array).unwrap();
    println!("NPY written to buffer, size: {}", buffer.len());
}
