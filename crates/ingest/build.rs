fn main() -> Result<(), Box<dyn std::error::Error>> {
    prost_build::Config::new().compile_protos(&["proto/gtfs-realtime.proto"], &["proto/"])?;
    println!("cargo:rerun-if-changed=proto/gtfs-realtime.proto");
    println!("cargo:rerun-if-changed=proto/nyct-subway.proto");
    Ok(())
}
