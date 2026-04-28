use cef::{ContextMenuHandler, *};

#[derive(Clone)]
pub struct IcyContextMenuHandler {}

impl IcyContextMenuHandler {
    pub fn new() -> Self {
        Self {}
    }
}

wrap_context_menu_handler! {
    pub(crate) struct ContextMenuHandlerBuilder {
        context_menu_handler: IcyContextMenuHandler,
    }

    impl ContextMenuHandler {
        fn on_before_context_menu(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _params: Option<&mut ContextMenuParams>,
            model: Option<&mut MenuModel>,
        ) {
            if let Some(model) = model {
                model.clear();
            }
        }
    }
}

impl ContextMenuHandlerBuilder {
    pub(crate) fn build(context_menu_handler: IcyContextMenuHandler) -> ContextMenuHandler {
        Self::new(context_menu_handler)
    }
}
