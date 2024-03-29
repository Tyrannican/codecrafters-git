pub(crate) mod catfile;

#[allow(dead_code)]
pub(crate) enum GitObject {
    Blob,
    Tree,
    Commit,
}
