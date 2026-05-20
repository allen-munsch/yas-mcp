use anyhow::Result;
use vergen::{BuildBuilder, CargoBuilder, Emitter, RustcBuilder, SysinfoBuilder};

fn main() -> Result<()> {
    let build = BuildBuilder::all_build()?;
    let cargo = CargoBuilder::all_cargo()?;
    let rustc = RustcBuilder::all_rustc()?;
    let si = SysinfoBuilder::all_sysinfo()?;

    Emitter::default()
        .add_instructions(&build)?
        .add_instructions(&cargo)?
        .add_instructions(&rustc)?
        .add_instructions(&si)?
        .emit()?;

    // Compile protobuf if protoc is available (needed for gRPC feature)
    match std::process::Command::new("protoc").arg("--version").output() {
        Ok(_) => {
            println!("cargo:warning=compiling protobuf definitions");
            tonic_build::configure()
                .build_server(true)
                .build_client(false)
                .compile_protos(&["proto/mcp.proto"], &["proto/"])?;
        }
        Err(_) => {
            println!("cargo:warning=protoc not found, skipping gRPC codegen");
            println!("cargo:warning=install protoc: apt install protobuf-compiler");
        }
    }

    Ok(())
}
