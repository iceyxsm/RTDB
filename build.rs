fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Check if protoc is available
    match std::process::Command::new("protoc").arg("--version").output() {
        Ok(_) => {
            tonic_build::configure()
                .build_server(true)
                .build_client(false)
                .compile(
                    &["proto/collections.proto", "proto/points.proto", "proto/qdrant.proto"],
                    &["proto"],
                )?;
            println!("cargo:rustc-cfg=grpc_enabled");
            println!("cargo:rerun-if-changed=proto/");
        }
        Err(_) => {
            println!("cargo:warning=protoc not found, gRPC API will be disabled");
        }
    }
    
    Ok(())
}
