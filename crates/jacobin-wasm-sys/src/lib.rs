// This includes the JAVA_HOME path in the binary, which is not what we want.
#![feature(once_cell)]
include!(concat!(env!("OUT_DIR"), "/binding.rs"));

use anyhow::{anyhow, Result};
use std::process::Command;

pub fn compile_java_to_jar(file: String) -> Result<()> {
    let mut cmd = Command::new(format!("{}/bin/javac", JAVA_HOME));
    cmd.arg(file);

    let output = cmd.output()?;
    if !output.status.success() {
        return Err(anyhow!(
            "Compiling Java failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    // Here we test if we can run the compiled wasm file with wasmtime
    use super::*;
    use crate::*;
    use wasmtime::*;

    use wasmtime_wasi::sync::WasiCtxBuilder;
    use wasmtime_wasi::sync::*;
    use wasmtime_wasi::WasiCtx;
    use wizer::Wizer;

    fn create_linker(engine: &wasmtime::Engine) -> wasmtime::Linker<wasmtime_wasi::WasiCtx> {
        let mut linker = wasmtime::Linker::new(&engine);

        wasmtime_wasi::add_to_linker(&mut linker, |s| s).unwrap();
        // Tamper wasi read
        linker.clone()
    }

    fn get_current_working_dir() -> std::io::Result<std::path::PathBuf> {
        std::env::current_dir()
    }

    #[test]
    fn test_run_wasm() {
        // Read from the output directory
        let wasm = include_bytes!(concat!(env!("OUT_DIR"), "/jacobin.wasm"));

        eprintln!("wasm: {:?} bytes", wasm.len());
        eprintln!("JAVA_HOME: {}", JAVA_HOME);
        eprintln!(
            "Current working dir: {:?}",
            get_current_working_dir().unwrap()
        );

        // Get wasmtime to run it.
        let mut config = wasmtime::Config::default();
        let config = config.strategy(wasmtime::Strategy::Cranelift);

        // This actually produces the same default binary :|
        // let config = config.cranelift_opt_level(wasmtime::OptLevel::SpeedAndSize);

        // We need to save the generated machine code to disk

        // Create a new store
        let engine = wasmtime::Engine::new(&config).unwrap();

        let module = wasmtime::Module::new(&engine, wasm).unwrap();
        let java_home_buff = std::path::PathBuf::from(JAVA_HOME);

        // Compile Java file
        compile_java_to_jar("./tests/Hello.java".to_string()).unwrap();

        let wasi = WasiCtxBuilder::new()
            .inherit_stdio()
            .inherit_args()
            .unwrap()
            .preopened_dir(
                wasmtime_wasi::sync::Dir::open_ambient_dir(
                    java_home_buff,
                    wasmtime_wasi::sync::ambient_authority(),
                )
                .unwrap(),
                JAVA_HOME,
            )
            .unwrap()
            .preopened_dir(
                wasmtime_wasi::sync::Dir::open_ambient_dir(
                    get_current_working_dir().unwrap(),
                    wasmtime_wasi::sync::ambient_authority(),
                )
                .unwrap(),
                "/Users/javierca/Documents/Develop/Gavy/crates/jacobin-wasm-sys",
            )
            .unwrap()
            .env("JAVA_HOME", JAVA_HOME)
            .unwrap()
            .env("HOME", ".")
            .unwrap()
            //.arg("-jar")
            //.unwrap()
            //.arg("-trace:inst")
            //.unwrap()
            // UNcomment for logs
            //.arg("-verbose:finest")
            //.unwrap()
            .arg("/Users/javierca/Documents/Develop/Gavy/crates/jacobin-wasm-sys/tests/HelloWorld.class")
            .unwrap()
            .build();

        // TODO share the linker between instances ?
        let mut linker = create_linker(&engine);
        let mut store = wasmtime::Store::new(&engine, wasi);

        linker.module(&mut store, "", &module).unwrap();

        linker
            .get_default(&mut store, "")
            .unwrap()
            .typed::<(), ()>(&mut store)
            .unwrap()
            .call(&mut store, ());
    }
    use std::{collections::HashMap, rc::Rc, sync::OnceLock};
    static mut WASI: OnceLock<WasiCtx> = OnceLock::new();
}
