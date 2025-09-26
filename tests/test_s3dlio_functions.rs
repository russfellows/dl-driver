// SPDX-FileCopyrightText: 2025 Russ Fellows <russ.fellows@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later

use s3dlio;

fn main() {
    // Test what s3dlio functions are available
    println!("Testing s3dlio functions");
    let data = s3dlio::generate_controlled_data(100, 0, 0);
    println!("generate_controlled_data works, length: {}", data.len());
}
