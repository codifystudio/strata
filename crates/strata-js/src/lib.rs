//! Strata JS — обёртка над boa_engine.
//!
//! MVP: создаёт JS-контекст с регистром console.log, выполняет скрипты
//! из <script> тегов. DOM API пока не реализован — следующий этап.

use boa_engine::{js_string, Context, JsValue, Source};
use strata_core::{StrataError, StrataResult};

pub struct JsEngine {
    ctx: Context,
}

impl JsEngine {
    pub fn new() -> StrataResult<Self> {
        let mut ctx = Context::default();

        // Регистр console.log → писать в log crate.
        let console_log = boa_engine::native_function::NativeFunction::from_fn_ptr(|_, args, ctx| {
            let msg: Vec<String> = args
                .iter()
                .map(|v| {
                    v.to_string(ctx)
                        .map(|s| s.to_std_string_escaped())
                        .unwrap_or_default()
                })
                .collect();
            log::info!("[console.log] {}", msg.join(" "));
            Ok(JsValue::undefined())
        });
        let console = boa_engine::object::ObjectInitializer::new(&mut ctx)
            .function(console_log, js_string!("log"), 0)
            .build();
        ctx.register_global_property(
            js_string!("console"),
            console,
            boa_engine::property::Attribute::all(),
        );

        Ok(Self { ctx })
    }

    pub fn eval(&mut self, src: &str) -> StrataResult<String> {
        let bytes = Source::from_bytes(src);
        let result = self
            .ctx
            .eval(bytes)
            .map_err(|e| StrataError::Js(e.to_string()))?;
        let display = result
            .to_string(&mut self.ctx)
            .map_err(|e| StrataError::Js(e.to_string()))?;
        Ok(display.to_std_string_escaped())
    }
}

impl Default for JsEngine {
    fn default() -> Self {
        Self::new().expect("failed to build JsEngine")
    }
}
