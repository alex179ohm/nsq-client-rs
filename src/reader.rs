#[cfg(feature = "async")]
use crate::async_context::ContextAsync;
use crate::client::Context;
use crate::msgs::Msg;
use crate::msgs::Touch;
#[cfg(feature = "async")]
use std::future::Future;

#[cfg(not(feature = "async"))]
pub trait Consumer: Clone + Sync + Send + 'static {
    fn on_msg(&mut self, msg: Msg, ctx: &mut Context);
    fn on_max_attemps(&mut self, msg: Msg, ctx: &mut Context) {
        ctx.send(Touch(msg.id));
    }
    fn on_close(&mut self, ctx: &mut Context) {}
}

#[cfg(feature = "async")]
pub trait Consumer: Clone + Sync + Send + 'static {
    type Output: Box<dyn Future<Output = ()>>;
    fn handle(&mut self, msg: Msg, ctx: &mut ContextAsync) -> Self::_Future;
    fn on_max_attemps(&mut self, msg: Msg, ctx: &mut ContextAsync) -> Self::_Future {
        ctx.send(Touch(msg.id));
    }
}
