//! Reading a module without compiling it: full validation against the
//! WebAssembly features the sandbox allows, its shape, and its function
//! imports.
use wasmparser::{Parser, Payload, TypeRef, Validator, WasmFeatures};

/// WebAssembly 2.0: SIMD, bulk memory, multi-value, reference types. No
/// threads or shared memory, 64-bit memory, multiple memories, GC,
/// exceptions or stack switching: each is more host surface for no
/// presentation need yet.
pub const FEATURES: WasmFeatures = WasmFeatures::WASM2;

/// Compile time grows with the code Cranelift has to compile, and a module
/// is compiled before it runs, so its shape is bounded as well as its size:
/// measured (red team, 2026-09-28, one core) at about 5 s for 60,000 tiny
/// functions (1.2 MB) and 3 s for one 4 MB function. Ordinary Add-Ons built
/// from Rust have a few thousand functions of a few KiB.
pub const MAX_FUNCTIONS: u32 = 20_000;
pub const MAX_FUNCTION_BYTES: usize = 512 * 1024;

pub struct Import {
    pub module: String,
    pub name: String,
}

/// Validate `bytes` and list what it imports. Only functions may be
/// imported: the host provides no memories, tables or globals.
pub fn function_imports(bytes: &[u8]) -> Result<Vec<Import>, String> {
    Validator::new_with_features(FEATURES)
        .validate_all(bytes)
        .map_err(|e| format!("not a valid WebAssembly module: {e}"))?;
    let mut out = Vec::new();
    for payload in Parser::new(0).parse_all(bytes) {
        let payload = payload.map_err(|e| e.to_string())?;
        if let Payload::FunctionSection(reader) = &payload
            && reader.count() > MAX_FUNCTIONS
        {
            return Err(format!(
                "has {} functions; the limit is {MAX_FUNCTIONS}",
                reader.count()
            ));
        }
        if let Payload::CodeSectionEntry(body) = &payload
            && body.range().len() > MAX_FUNCTION_BYTES
        {
            return Err(format!(
                "has a function of {} bytes; the limit is {MAX_FUNCTION_BYTES}",
                body.range().len()
            ));
        }
        if let Payload::ImportSection(reader) = payload {
            for import in reader.into_imports() {
                let import = import.map_err(|e| e.to_string())?;
                if !matches!(import.ty, TypeRef::Func(_) | TypeRef::FuncExact(_)) {
                    return Err(format!(
                        "imports `{}.{}`, which is not a function; the host provides only functions",
                        import.module, import.name
                    ));
                }
                out.push(Import {
                    module: import.module.to_string(),
                    name: import.name.to_string(),
                });
            }
        }
    }
    Ok(out)
}
