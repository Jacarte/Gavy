#[cfg(test)]
mod tests {
    // Here we test if we can run the compiled wasm file with wasmtime
    use wasmtime::*;

    use wasmtime_wasi::sync::WasiCtxBuilder;
    use wasmtime_wasi::sync::*;

    fn create_linker(engine: &wasmtime::Engine) -> wasmtime::Linker<wasmtime_wasi::WasiCtx> {
        let mut linker = wasmtime::Linker::new(&engine);

        wasmtime_wasi::add_to_linker(&mut linker, |s| s).unwrap();
        linker.clone()
    }

    #[test]
    fn test_run_wasm() {
        // Read from the output directory
        let wasm = include_bytes!(concat!(env!("OUT_DIR"), "/jacobin.wasm"));

        eprint!("wasm: {:?} bytes", wasm.len());

        // Get wasmtime to run it.
        let mut config = wasmtime::Config::default();
        let config = config.strategy(wasmtime::Strategy::Cranelift);

        // This actually produces the same default binary :|
        // let config = config.cranelift_opt_level(wasmtime::OptLevel::SpeedAndSize);

        // We need to save the generated machine code to disk

        // Create a new store
        let engine = wasmtime::Engine::new(&config).unwrap();

        let module = wasmtime::Module::new(&engine, wasm).unwrap();
        let JAVA_HOME = concat!(env!("OUT_DIR"), "/java/jdk-17.0.8.jdk/Contents/Home");
        let java_home_buff = std::path::PathBuf::from(JAVA_HOME);

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
            .env("JAVA_HOME", JAVA_HOME)
            .unwrap()
            .env("HOME", JAVA_HOME)
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
            .call(&mut store, ())
            .unwrap();
    }
}
