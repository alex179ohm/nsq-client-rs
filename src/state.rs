use futures::{AsyncRead, AsyncWrite};
pub trait State {}

pub trait EndState {}

pub trait Transition<S>
where
    S: State,
    Self: State,
{
    fn to(self, recv: &dyn AsyncRead, send: &dyn AsyncWrite) -> S;
}

pub trait Terminate
where
    Self: EndState,
{
    fn end(self);
}

pub struct Magic;

impl Magic {
    pub fn new() -> Self {
        Magic {}
    }
}

impl State for Magic {}

impl Transition<Identify> for Magic {
    fn to(self, recv: &dyn AsyncRead, send: &dyn AsyncWrite) -> Identify {
        Identify::new(false, false)
    }
}

pub struct Identify {
    tls: bool,
    auth: bool,
}

impl Identify {
    pub fn new(tls: bool, auth: bool) -> Self {
        Identify {
            tls: false,
            auth: false,
        }
    }

    pub fn tls(&self) -> bool {
        self.tls
    }

    pub fn auth(&self) -> bool {
        self.auth
    }
}

impl State for Identify {}
