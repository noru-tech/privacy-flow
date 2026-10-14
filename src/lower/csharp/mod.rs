//! C# lowering (tree-sitter-c-sharp).

use crate::ir::*;

pub fn lower(path: &str, _src: &str) -> FileIr {
    FileIr::new(path, Lang::CSharp)
}
