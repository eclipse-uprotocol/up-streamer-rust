use up_rust::{UMessage, UStatus};

mod up_client_foo;

pub type UTransportListener = Box<dyn Fn(Result<UMessage, UStatus>) + Send + Sync + 'static>;

pub use up_client_foo::{UPClientFoo, UTransportBuilderFoo};
