//! JSON State

use super::{app::Application, node::NodeKey};
use crate::{ArcStr, ro_string, Error};
use lmfu::json::Path;

pub type NamespaceCallback = fn(
    app: &Application,
    ns_creator: NodeKey,
    ns_user: NodeKey,
    path: &mut Path,
) -> Result<(), Error>;

#[derive(Clone)]
pub struct Namespace {
    pub name: ArcStr,
    pub path: Path,
    pub callback: NamespaceCallback,
}

fn root_ns_callback(_: &Application, _: NodeKey, _: NodeKey, _: &mut Path) -> Result<(), Error> {
    Ok(())
}

pub fn root_ns() -> Namespace {
    Namespace {
        name: ro_string!("root"),
        path: Path::new(),
        callback: root_ns_callback,
    }
}
