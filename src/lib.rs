#![feature(async_await, async_closure)]
#![recursion_limit = "128"]

pub mod action;
pub mod build;
pub mod citeproc;
pub mod client;
pub mod codec;
pub mod completion;
pub mod definition;
pub mod diagnostics;
pub mod folding;
pub mod formatting;
pub mod forward_search;
pub mod highlight;
pub mod hover;
pub mod link;
pub mod reference;
pub mod rename;
pub mod scenario;
pub mod server;
pub mod symbol;
pub mod syntax;
pub mod tex;
pub mod workspace;
