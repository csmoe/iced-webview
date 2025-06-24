use cef::{rc::*, *};

use crate::v8::IcyV8HandlerBuilder;

pub struct RenderApp {
    object: *mut RcImpl<sys::_cef_app_t, Self>,
}

impl RenderApp {
    pub fn new() -> App {
        App::new(Self {
            object: std::ptr::null_mut(),
        })
    }
}

impl WrapApp for RenderApp {
    fn wrap_rc(&mut self, object: *mut RcImpl<sys::_cef_app_t, Self>) {
        self.object = object;
    }
}

impl Clone for RenderApp {
    fn clone(&self) -> Self {
        let object = unsafe {
            let rc_impl = &mut *self.object;
            rc_impl.interface.add_ref();
            self.object
        };

        Self { object }
    }
}

impl Rc for RenderApp {
    fn as_base(&self) -> &sys::cef_base_ref_counted_t {
        unsafe {
            let base = &*self.object;
            std::mem::transmute(&base.cef_object)
        }
    }
}

impl ImplApp for RenderApp {
    fn get_raw(&self) -> *mut sys::_cef_app_t {
        self.object as *mut sys::_cef_app_t
    }

    fn render_process_handler(&self) -> Option<RenderProcessHandler> {
        Some(IcyRenderProcessHandlerBuilder::build(
            IcyRenderProcessHandler {},
        ))
    }
}

#[derive(Clone)]
struct IcyRenderProcessHandler {}

struct IcyRenderProcessHandlerBuilder {
    object: *mut RcImpl<sys::cef_render_process_handler_t, Self>,
    handler: IcyRenderProcessHandler,
}

impl IcyRenderProcessHandlerBuilder {
    fn build(handler: IcyRenderProcessHandler) -> RenderProcessHandler {
        RenderProcessHandler::new(Self {
            object: std::ptr::null_mut(),
            handler,
        })
    }
}

impl Rc for IcyRenderProcessHandlerBuilder {
    fn as_base(&self) -> &sys::cef_base_ref_counted_t {
        unsafe {
            let base = &*self.object;
            std::mem::transmute(&base.cef_object)
        }
    }
}

impl WrapRenderProcessHandler for IcyRenderProcessHandlerBuilder {
    fn wrap_rc(&mut self, object: *mut RcImpl<sys::_cef_render_process_handler_t, Self>) {
        self.object = object;
    }
}

impl Clone for IcyRenderProcessHandlerBuilder {
    fn clone(&self) -> Self {
        let object = unsafe {
            let rc_impl = &mut *self.object;
            rc_impl.interface.add_ref();
            rc_impl
        };

        Self {
            object,
            handler: self.handler.clone(),
        }
    }
}

impl ImplRenderProcessHandler for IcyRenderProcessHandlerBuilder {
    fn get_raw(&self) -> *mut sys::_cef_render_process_handler_t {
        self.object.cast()
    }

    fn on_browser_created(
        &self,
        browser: Option<&mut Browser>,
        _extra_info: Option<&mut DictionaryValue>,
    ) {
        if let Some(browser) = browser {
            eprintln!("render: create browser {}", browser.identifier());
        }
    }

    fn on_web_kit_initialized(&self) {
        eprintln!("render: webkit init");
    }

    fn on_focused_node_changed(
        &self,
        _browser: Option<&mut Browser>,
        frame: Option<&mut Frame>,
        node: Option<&mut Domnode>,
    ) {
        if let Some(node) = node {
            if node.is_editable() == 1 {
                let bound = node.element_bounds();

                let Some(frame) = frame else {
                    return;
                };
                let Some(mut message) = cef::process_message_create(Some(&cef::CefString::from(
                    "renderer.editable_node_focused",
                ))) else {
                    return;
                };
                let Some(args) = message.argument_list() else {
                    return;
                };
                let element =
                    serde_json::to_string(&serde_json::json!({ "x": bound.x, "y": bound.y, "width": bound.height, "height": bound.width })).ok();
                if args.set_string(
                    0,
                    element
                        .as_ref()
                        .map(|s| (&cef::CefStringUtf8::from(s.as_str())).into())
                        .as_ref(),
                ) != 1
                {
                    return;
                }

                frame.send_process_message(
                    cef::sys::cef_process_id_t::PID_BROWSER.into(),
                    Some(&mut message),
                );
            }
        }
    }

    fn on_context_created(
        &self,
        browser: Option<&mut Browser>,
        frame: Option<&mut Frame>,
        context: Option<&mut V8Context>,
    ) {
        if let Some(browser) = browser {
            eprintln!("render: context created {}", browser.identifier());
        }
        if let Some(frame) = frame {
            frame.execute_java_script(
                Some(
                    &r#"
// caret changed by click
window.addEventListener('click', (event) => {
  const { offset } = document.caretPositionFromPoint(event.clientX, event.clientY);
  window.caret_offset(offset);
});

// caret changed by arrow keys
window.addEventListener('keyup', (event) => {
    switch (event.code) {
        case "ArrowLeft":
        case "ArrowRight":
        case "ArrowUp":
        case "ArrowDown":
            const offset = event.target.selectionStart;
            if (offset !== undefined) {
                window.caret_offset(offset);
            }
            break;
        default:
            break;
    };
});
            "#
                    .into(),
                ),
                None,
                0,
            );
        }

        let mut caret_handler = IcyV8HandlerBuilder::build(|name, _this, args| {
            if name != "caret_offset" {
                return cef::v8_value_create_null()
                    .ok_or_else(|| anyhow::anyhow!("cannot create null"));
            }
            let [Some(offset)] = args else {
                anyhow::bail!("no args");
            };
            let offset = offset.double_value();

            let Some(mut message) = cef::process_message_create(Some(&cef::CefString::from(
                "renderer.caret_offset_changed",
            ))) else {
                anyhow::bail!("cannot create ipc message");
            };
            let Some(args) = message.argument_list() else {
                anyhow::bail!("no args");
            };
            let element = serde_json::to_string(&serde_json::json!({ "offset": offset })).ok();
            if args.set_string(
                0,
                element
                    .as_ref()
                    .map(|s| (&cef::CefStringUtf8::from(s.as_str())).into())
                    .as_ref(),
            ) != 1
            {
                anyhow::bail!("cannot set payload");
            }

            let Some(context) = cef::v8_context_get_current_context() else {
                anyhow::bail!("no v8 context")
            };
            let Some(frame) = context.frame() else {
                anyhow::bail!("no frame")
            };
            frame.send_process_message(
                cef::sys::cef_process_id_t::PID_BROWSER.into(),
                Some(&mut message),
            );

            return cef::v8_value_create_null()
                .ok_or_else(|| anyhow::anyhow!("cannot create v8 value"));
        });
        let Some(context) = context else {
            return;
        };
        if let Some(global) = context.global() {
            context.enter();
            let Some(mut caret_offset) = cef::v8_value_create_function(
                Some(&"caret_offset".into()),
                Some(&mut caret_handler),
            ) else {
                context.exit();
                return;
            };
            global.set_value_bykey(
                Some(&"caret_offset".into()),
                Some(&mut caret_offset),
                cef::sys::cef_v8_propertyattribute_t::V8_PROPERTY_ATTRIBUTE_READONLY.into(),
            );
            context.exit();
        }
    }

    fn on_context_released(
        &self,
        browser: Option<&mut Browser>,
        _frame: Option<&mut Frame>,
        _context: Option<&mut V8Context>,
    ) {
        if let Some(browser) = browser {
            eprintln!("render: context released {}", browser.identifier());
        }
    }
}
