use crate::object::GitObject;

pub(crate) fn read_object(object: GitObject, content: Vec<u8>) {
    match object {
        GitObject::Blob => read_blob(content),
        GitObject::Tree => read_tree(content),
        GitObject::Commit => read_commit(content),
        _ => unreachable!("this shouldn't happen"),
    }
}
fn read_blob(content: Vec<u8>) {
    println!("Parsing blob with {} bytes", content.len());
}
fn read_tree(content: Vec<u8>) {
    println!("Parsing tree with {} bytes", content.len());
}
fn read_commit(content: Vec<u8>) {
    println!("Parsing commit with {} bytes", content.len());
}
