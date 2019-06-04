#[cfg(feature = "async")]
use crate::async_context::ContextAsync;
use crate::client::Context;
use crate::msgs::Msg;
use crate::msgs::Touch;
#[cfg(feature = "async")]
use std::future::Future;
use log::warn;

#[cfg(not(feature = "async"))]
pub trait Consumer: Copy + Sync + Send + 'static {
    fn handle(&mut self, msg: Msg, ctx: &mut Context);
    fn on_max_attemps(&mut self, msg: Msg, ctx: &mut Context) {
        ctx.send(Touch(msg.id));
    }
    fn on_disconnected(&mut self) {
        warn!("Connection closed by peer");
    }
}

#[cfg(feature = "async")]
pub trait Consumer: Copy + Sync + Send + 'static {
    type Output: Box<dyn Future<Output = ()>>;
    fn handle(&mut self, msg: Msg, ctx: &mut ContextAsync) -> Self::_Future;
    fn on_max_attemps(&mut self, msg: Msg, ctx: &mut ContextAsync) -> Self::_Future {
        ctx.send(Touch(msg.id));
    }
}
