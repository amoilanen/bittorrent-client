pub(crate) fn new_error(message: String) -> anyhow::Error {
    std::io::Error::new(std::io::ErrorKind::Other, message).into()
}