fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Always try to generate API proto files when grpc feature is enabled
    if std::env::var("CARGO_FEATURE_GRPC").is_ok() {
        if let Err(e) = generate_api_proto() {
            println!("cargo:warning=Failed to generate API proto files: {}", e);
            println!("cargo:warning=Using pre-generated proto files if available");
            println!("cargo:warning=Install protoc to regenerate: https://grpc.io/docs/protoc-installation/");
        }
    }
    
    // Check if we should regenerate protobuf code
    let regenerate = std::env::var("CARGO_FEATURE_REGENERATE_PROTO").is_ok();
    
    if regenerate {
        // Force regeneration - requires protoc
        println!("cargo:warning=Regenerating protobuf code...");
        generate_protobuf_code()?;
    } else {
        // Use pre-generated code - no protoc needed
        // The generated code is already in src/cluster/generated/
        println!("cargo:rustc-cfg=grpc_enabled");
        println!("cargo:rerun-if-changed=src/cluster/rpc.proto");
    }
    
    Ok(())
}

fn generate_api_proto() -> Result<(), Box<dyn std::error::Error>> {
    // Create output directory if it doesn't exist
    std::fs::create_dir_all("src/api/generated")?;
    
    tonic_build::configure()
        .build_server(true)
        .build_client(false)
        .out_dir("src/api/generated/")
        .compile(
            &[
                "proto/collections.proto",
                "proto/points.proto",
                "proto/qdrant.proto",
            ],
            &["proto/"],
        )?;
    
    println!("cargo:rerun-if-changed=proto/qdrant.proto");
    println!("cargo:rerun-if-changed=proto/collections.proto");
    println!("cargo:rerun-if-changed=proto/points.proto");
    
    Ok(())
}

fn generate_protobuf_code() -> Result<(), Box<dyn std::error::Error>> {
    // Check if protoc is available
    match std::process::Command::new("protoc").arg("--version").output() {
        Ok(_) => {
            // Generate to the src/cluster/generated/ directory
            tonic_build::configure()
                .build_server(true)
                .build_client(true)
                .out_dir("src/cluster/generated/")
                .compile(
                    &["src/cluster/rpc.proto"],
                    &["src/cluster/"],
                )?;
            
            println!("cargo:warning=Protobuf code regenerated successfully!");
            println!("cargo:warning=Don't forget to check in the updated files:");
            println!("cargo:warning=  git add src/cluster/generated/");
            println!("cargo:rustc-cfg=grpc_enabled");
            println!("cargo:rerun-if-changed=src/cluster/rpc.proto");
        }
        Err(e) => {
            println!("cargo:warning=protoc not found: {}", e);
            println!("cargo:warning=Cannot regenerate protobuf code. Using pre-generated files.");
            println!("cargo:warning=Install protoc to regenerate: https://grpc.io/docs/protoc-installation/");
        }
    }
    
    Ok(())
}
