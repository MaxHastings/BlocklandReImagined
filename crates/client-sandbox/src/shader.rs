//! Add-On shaders. An Add-On writes WGSL against a fixed interface (the
//! prelude below); every shader is parsed, checked against the sandbox's
//! rules and rewritten before a GPU ever sees it, when the Add-On loads.
//!
//! The rules exist because the GPU is shared with the game and a stuck or
//! runaway shader can freeze or reset the whole device:
//!
//! - Only a vertex stage `vs_main` and a fragment stage `fs_main`. No
//!   compute, so no dispatches of any size.
//! - No resources beyond the prelude's two uniforms: no storage buffers,
//!   atomics, textures or immediates, so a shader cannot read or write
//!   anything but its own draw.
//! - Every loop is bounded. The rewrite gives each invocation one shared
//!   iteration allowance that every loop draws from; when it runs out,
//!   loops exit. The engine sets the allowance every frame
//!   (`bri_frame.limits.x`) from the GPU's measured speed, the screen size
//!   and the shader's cost; it starts at [`DEFAULT_LOOP_LIMIT`] and never
//!   exceeds [`MAX_LOOP_LIMIT`].
//! - Bounded cost. WGSL has no recursion, but helper calls can still fan
//!   out (each function calling the next twice doubles the work), so the
//!   rewrite counts every expression an entry point runs with all calls
//!   expanded ([`Shader::vertex_cost`], [`Shader::fragment_cost`]) and
//!   refuses shaders over [`MAX_COST`]. Work per invocation is at most
//!   cost x (allowance + 1).
//! - Bounded size: source bytes, functions, expressions per function, and
//!   the size of any type (a million-element local array is refused).
use naga::{
    AddressSpace, BinaryOperator, Binding, Block, Expression, Function, Handle, Literal, Module,
    ShaderStage, Span, Statement, TypeInner,
    valid::{Capabilities, ValidationFlags, Validator},
};

pub const MAX_SHADER_BYTES: usize = 64 * 1024;
pub const MAX_FUNCTIONS: usize = 256;
pub const MAX_EXPRESSIONS: usize = 16 * 1024;
pub const MAX_TYPE_BYTES: u32 = 16 * 1024;
/// Loop iterations one shader invocation may run in total, across every
/// loop it enters, until the GPU's speed has been measured.
pub const DEFAULT_LOOP_LIMIT: u32 = 16;
/// The most loop iterations one invocation may ever run, on any GPU.
pub const MAX_LOOP_LIMIT: u32 = 4096;
/// The most expressions one entry point may run per loop iteration (or in
/// total, without loops), with every helper call expanded.
pub const MAX_COST: u32 = 8192;
/// Vertex attributes the engine supplies: position, normal, uv.
pub const VERTEX_LOCATIONS: u32 = 3;

const BUDGET_NAME: &str = "bri_loop_budget";

/// The interface every Add-On shader is compiled with. It is appended after
/// the Add-On's source (WGSL declarations are order independent), so error
/// line numbers are the Add-On's own.
pub const PRELUDE: &str = r#"
// ---- Blockland ReImagined Add-On shader interface ----
struct BriFrame {
    view_proj: mat4x4<f32>,
    camera: vec4<f32>,
    // x: seconds since the Add-On started, y: seconds since last frame.
    time: vec4<f32>,
    // x: loop iterations each invocation may run this frame (engine-set).
    limits: vec4<u32>,
};
struct BriDraw {
    model: mat4x4<f32>,
    params: array<vec4<f32>, 4>,
};
struct BriVertex {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};
@group(0) @binding(0) var<uniform> bri_frame: BriFrame;
@group(1) @binding(0) var<uniform> bri_draw: BriDraw;
var<private> bri_loop_budget: u32;
"#;

/// A shader that passed every rule, ready for `wgpu::ShaderSource::Naga`.
#[derive(Debug, Clone)]
pub struct Shader {
    pub name: String,
    pub module: Module,
    /// Loops the rewrite bounded.
    pub loops: usize,
    /// Expressions `vs_main` / `fs_main` run per loop iteration (or in
    /// total, without loops), with every call expanded.
    pub vertex_cost: u32,
    pub fragment_cost: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShaderError {
    pub code: &'static str,
    pub message: String,
}
impl std::fmt::Display for ShaderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for ShaderError {}

fn fail(code: &'static str, message: impl Into<String>) -> ShaderError {
    ShaderError {
        code,
        message: message.into(),
    }
}

/// Check and rewrite one Add-On shader. `name` is its file in the Add-On.
pub fn compile(name: &str, source: &str) -> Result<Shader, ShaderError> {
    if source.len() > MAX_SHADER_BYTES {
        return Err(fail(
            "shader.too_large",
            format!("{name} is over {} KiB", MAX_SHADER_BYTES / 1024),
        ));
    }
    let full = format!("{source}\n{PRELUDE}");
    let mut module = naga::front::wgsl::parse_str(&full).map_err(|e| {
        fail(
            "shader.parse",
            e.emit_to_string_with_path(&full, name)
                .trim_end()
                .to_string(),
        )
    })?;
    let mut validator = Validator::new(ValidationFlags::all(), Capabilities::empty());
    validator.validate(&module).map_err(|e| {
        fail(
            "shader.invalid",
            e.emit_to_string_with_path(&full, name)
                .trim_end()
                .to_string(),
        )
    })?;
    check_interface(name, &module)?;
    check_size(name, &module)?;
    let budget = module
        .global_variables
        .iter()
        .find(|(_, g)| g.name.as_deref() == Some(BUDGET_NAME))
        .map(|(h, _)| h)
        .ok_or_else(|| fail("shader.interface", "the loop budget is missing"))?;
    let mut loops = 0;
    for (_, function) in module.functions.iter_mut() {
        loops += bound_loops(function, budget)?;
    }
    let limit = limit_member(&module)?;
    for entry in module.entry_points.iter_mut() {
        loops += bound_loops(&mut entry.function, budget)?;
        start_budget(&mut entry.function, budget, limit);
    }
    let costs = costs(&module);
    let entry_cost = |stage| {
        module
            .entry_points
            .iter()
            .find(|e| e.stage == stage)
            .map_or(0, |e| cost_of(&e.function, &costs))
    };
    let vertex_cost = entry_cost(ShaderStage::Vertex);
    let fragment_cost = entry_cost(ShaderStage::Fragment);
    if vertex_cost.max(fragment_cost) > MAX_COST {
        return Err(fail(
            "shader.too_costly",
            format!(
                "{name} runs {} expressions per invocation with its helper calls expanded; the limit is {MAX_COST}",
                vertex_cost.max(fragment_cost)
            ),
        ));
    }
    // The rewritten module must still be valid; anything else is our bug,
    // and still refused.
    validator.validate(&module).map_err(|e| {
        fail(
            "shader.rewrite",
            format!("bounding the loops made an invalid shader: {e}"),
        )
    })?;
    Ok(Shader {
        name: name.to_string(),
        module,
        loops,
        vertex_cost,
        fragment_cost,
    })
}

/// `bri_frame` and the index of its `limits` member.
fn limit_member(module: &Module) -> Result<(Handle<naga::GlobalVariable>, u32), ShaderError> {
    let missing = || fail("shader.interface", "bri_frame.limits is missing");
    let (frame, global) = module
        .global_variables
        .iter()
        .find(|(_, g)| g.name.as_deref() == Some("bri_frame"))
        .ok_or_else(missing)?;
    let TypeInner::Struct { members, .. } = &module.types[global.ty].inner else {
        return Err(missing());
    };
    let index = members
        .iter()
        .position(|m| m.name.as_deref() == Some("limits"))
        .ok_or_else(missing)?;
    Ok((frame, index as u32))
}

/// Start an entry point with `bri_loop_budget = bri_frame.limits.x;`.
fn start_budget(
    function: &mut Function,
    budget: Handle<naga::GlobalVariable>,
    (frame, member): (Handle<naga::GlobalVariable>, u32),
) {
    let e = &mut function.expressions;
    let pointer = e.append(Expression::GlobalVariable(budget), Span::UNDEFINED);
    let frame = e.append(Expression::GlobalVariable(frame), Span::UNDEFINED);
    let limits = e.append(
        Expression::AccessIndex {
            base: frame,
            index: member,
        },
        Span::UNDEFINED,
    );
    let x = e.append(
        Expression::AccessIndex {
            base: limits,
            index: 0,
        },
        Span::UNDEFINED,
    );
    let value = e.append(Expression::Load { pointer: x }, Span::UNDEFINED);
    let mut prefix = Block::new();
    prefix.push(
        Statement::Emit(naga::Range::new_from_bounds(limits, value)),
        Span::UNDEFINED,
    );
    prefix.push(Statement::Store { pointer, value }, Span::UNDEFINED);
    function.body.splice(0..0, prefix);
}

/// Each helper function's cost: its expressions plus, for every call it
/// makes, the callee's cost. Saturates rather than overflowing.
fn costs(module: &Module) -> Vec<u32> {
    // The validator requires callees to come before their callers.
    let mut costs = Vec::with_capacity(module.functions.len());
    for (_, function) in module.functions.iter() {
        let cost = cost_of(function, &costs);
        costs.push(cost);
    }
    costs
}

fn cost_of(function: &Function, costs: &[u32]) -> u32 {
    fn calls(block: &Block, costs: &[u32], total: &mut u32) {
        for statement in block.iter() {
            match statement {
                Statement::Call { function, .. } => {
                    let callee = costs.get(function.index()).copied().unwrap_or(u32::MAX);
                    *total = total.saturating_add(callee);
                }
                Statement::Block(inner) => calls(inner, costs, total),
                Statement::If { accept, reject, .. } => {
                    calls(accept, costs, total);
                    calls(reject, costs, total);
                }
                Statement::Switch { cases, .. } => {
                    for case in cases {
                        calls(&case.body, costs, total);
                    }
                }
                Statement::Loop {
                    body, continuing, ..
                } => {
                    calls(body, costs, total);
                    calls(continuing, costs, total);
                }
                _ => {}
            }
        }
    }
    let mut total = u32::try_from(function.expressions.len()).unwrap_or(u32::MAX);
    calls(&function.body, costs, &mut total);
    total
}

fn check_interface(name: &str, module: &Module) -> Result<(), ShaderError> {
    for (_, global) in module.global_variables.iter() {
        let own = matches!(
            global.name.as_deref(),
            Some("bri_frame" | "bri_draw" | BUDGET_NAME)
        );
        if own {
            continue;
        }
        if global.space != AddressSpace::Private || global.binding.is_some() {
            return Err(fail(
                "shader.resource",
                format!(
                    "{name}: `{}` is a {:?} resource; Add-On shaders may only use bri_frame, bri_draw and var<private> values",
                    global.name.as_deref().unwrap_or("?"),
                    global.space
                ),
            ));
        }
    }
    if !module.overrides.is_empty() {
        return Err(fail(
            "shader.override",
            format!("{name}: pipeline-overridable constants are not supported"),
        ));
    }
    let mut vertex = false;
    let mut fragment = false;
    for entry in &module.entry_points {
        match (entry.stage, entry.name.as_str()) {
            (ShaderStage::Vertex, "vs_main") => vertex = true,
            (ShaderStage::Fragment, "fs_main") => fragment = true,
            (stage, entry_name) => {
                return Err(fail(
                    "shader.entry_point",
                    format!(
                        "{name}: `{entry_name}` is a {stage:?} entry point; only @vertex vs_main and @fragment fs_main are allowed"
                    ),
                ));
            }
        }
        for argument in &entry.function.arguments {
            for location in locations(module, argument.ty, argument.binding.as_ref()) {
                if entry.stage == ShaderStage::Vertex && location >= VERTEX_LOCATIONS {
                    return Err(fail(
                        "shader.vertex_input",
                        format!(
                            "{name}: vertex input @location({location}) does not exist; use BriVertex (0 position, 1 normal, 2 uv)"
                        ),
                    ));
                }
            }
        }
        if entry.stage == ShaderStage::Fragment {
            let outputs = entry
                .function
                .result
                .as_ref()
                .map(|r| locations(module, r.ty, r.binding.as_ref()))
                .unwrap_or_default();
            if outputs != [0] {
                return Err(fail(
                    "shader.fragment_output",
                    format!("{name}: fs_main must return one @location(0) vec4<f32> colour"),
                ));
            }
        }
    }
    if !(vertex && fragment) {
        return Err(fail(
            "shader.entry_point",
            format!("{name}: needs both @vertex fn vs_main and @fragment fn fs_main"),
        ));
    }
    Ok(())
}

/// Every `@location` a value carries, looking through one struct level.
fn locations(module: &Module, ty: Handle<naga::Type>, binding: Option<&Binding>) -> Vec<u32> {
    match binding {
        Some(Binding::Location { location, .. }) => vec![*location],
        Some(Binding::BuiltIn(_)) => Vec::new(),
        None => match &module.types[ty].inner {
            TypeInner::Struct { members, .. } => members
                .iter()
                .filter_map(|m| match m.binding {
                    Some(Binding::Location { location, .. }) => Some(location),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        },
    }
}

fn check_size(name: &str, module: &Module) -> Result<(), ShaderError> {
    if module.functions.len() > MAX_FUNCTIONS {
        return Err(fail(
            "shader.too_complex",
            format!("{name} has more than {MAX_FUNCTIONS} functions"),
        ));
    }
    let functions = module
        .functions
        .iter()
        .map(|(_, f)| f)
        .chain(module.entry_points.iter().map(|e| &e.function));
    for function in functions {
        if function.expressions.len() > MAX_EXPRESSIONS {
            return Err(fail(
                "shader.too_complex",
                format!(
                    "{name}: `{}` has more than {MAX_EXPRESSIONS} expressions",
                    function.name.as_deref().unwrap_or("?")
                ),
            ));
        }
    }
    let ctx = module.to_ctx();
    for (_, ty) in module.types.iter() {
        // Runtime-sized arrays live only in storage buffers, refused above.
        let size = ty.inner.size(ctx);
        if size > MAX_TYPE_BYTES {
            return Err(fail(
                "shader.type_too_large",
                format!(
                    "{name}: a {} type is {size} bytes; the limit is {MAX_TYPE_BYTES}",
                    ty.name.as_deref().unwrap_or("value")
                ),
            ));
        }
    }
    Ok(())
}

/// Prefix every loop body in `function` with
/// `if bri_loop_budget == 0u { break; } bri_loop_budget -= 1u;`.
/// Returns how many loops it bounded. Add-On code may not touch the budget.
fn bound_loops(
    function: &mut Function,
    budget: Handle<naga::GlobalVariable>,
) -> Result<usize, ShaderError> {
    if function
        .expressions
        .iter()
        .any(|(_, e)| matches!(e, Expression::GlobalVariable(g) if *g == budget))
    {
        return Err(fail(
            "shader.reserved",
            format!("`{BUDGET_NAME}` belongs to the engine"),
        ));
    }
    let mut guard = None;
    let mut count = 0;
    let mut body = std::mem::take(&mut function.body);
    visit(&mut body, &mut |block| {
        let (pointer, zero, one) = *guard.get_or_insert_with(|| {
            let e = &mut function.expressions;
            (
                e.append(Expression::GlobalVariable(budget), Span::UNDEFINED),
                e.append(Expression::Literal(Literal::U32(0)), Span::UNDEFINED),
                e.append(Expression::Literal(Literal::U32(1)), Span::UNDEFINED),
            )
        });
        let e = &mut function.expressions;
        let load = e.append(Expression::Load { pointer }, Span::UNDEFINED);
        let empty = e.append(
            Expression::Binary {
                op: BinaryOperator::Equal,
                left: load,
                right: zero,
            },
            Span::UNDEFINED,
        );
        let left = e.append(Expression::Load { pointer }, Span::UNDEFINED);
        let less = e.append(
            Expression::Binary {
                op: BinaryOperator::Subtract,
                left,
                right: one,
            },
            Span::UNDEFINED,
        );
        let mut prefix = Block::new();
        prefix.push(
            Statement::Emit(naga::Range::new_from_bounds(load, empty)),
            Span::UNDEFINED,
        );
        let mut exit = Block::new();
        exit.push(Statement::Break, Span::UNDEFINED);
        prefix.push(
            Statement::If {
                condition: empty,
                accept: exit,
                reject: Block::new(),
            },
            Span::UNDEFINED,
        );
        prefix.push(
            Statement::Emit(naga::Range::new_from_bounds(left, less)),
            Span::UNDEFINED,
        );
        prefix.push(
            Statement::Store {
                pointer,
                value: less,
            },
            Span::UNDEFINED,
        );
        block.splice(0..0, prefix);
        count += 1;
    });
    function.body = body;
    Ok(count)
}

/// Call `on_loop` with the body of every loop in `block`, innermost loops
/// included.
fn visit(block: &mut Block, on_loop: &mut impl FnMut(&mut Block)) {
    for (statement, _) in block.span_iter_mut() {
        match statement {
            Statement::Block(inner) => visit(inner, on_loop),
            Statement::If { accept, reject, .. } => {
                visit(accept, on_loop);
                visit(reject, on_loop);
            }
            Statement::Switch { cases, .. } => {
                for case in cases {
                    visit(&mut case.body, on_loop);
                }
            }
            Statement::Loop {
                body, continuing, ..
            } => {
                visit(body, on_loop);
                visit(continuing, on_loop);
                on_loop(body);
            }
            _ => {}
        }
    }
}
