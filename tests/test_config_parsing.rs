use anyhow::Result;
use real_dlio_core::Config;

fn main() -> Result<()> {
    // Test parsing the expanded DLIO config
    let config = Config::from_yaml_file("test_expanded_config.yaml")?;
    
    println!("✅ Successfully parsed expanded DLIO config!");
    println!("Model: {:?}", config.model);
    println!("Framework: {:?}", config.framework);
    println!("Workflow: {:?}", config.workflow);
    println!("Storage Backend: {:?}", config.storage_backend());
    println!("Storage URI: {}", config.storage_uri());
    
    // Test that all new sections are properly parsed
    if let Some(eval) = &config.evaluation {
        println!("Evaluation: {:?}", eval);
    }
    
    if let Some(checkpoint) = &config.checkpoint {
        println!("Checkpoint: {:?}", checkpoint);
    }
    
    if let Some(output) = &config.output {
        println!("Output: {:?}", output);
    }
    
    if let Some(profiling) = &config.profiling {
        println!("Profiling: {:?}", profiling);
    }
    
    Ok(())
}