use crate::git::fetcher::GithubFetchable;

pub struct BinaryFile {
    data: Vec<u8>,
}

impl GithubFetchable for BinaryFile {
    fn from_body(body: Vec<u8>) -> Result<Self, Self::Error> {
        Ok(BinaryFile { data: body })
    }
    type Error = ();
}
