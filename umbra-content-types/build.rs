extern crate protoc_rust;

use std::io::Result;
fn main() -> Result<()> {
    prost_build::compile_protos(&["protos/content.proto"], &["protos/"])?;
    Ok(())
}
