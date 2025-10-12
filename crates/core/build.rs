fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Compile protobuf definitions for distributed execution
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(&["src/dist/proto/bench.proto"], &["src/dist/proto"])?;
    
    Ok(())
}
