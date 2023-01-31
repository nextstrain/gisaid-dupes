use crypto::digest::Digest;
use crypto::sha2::Sha256;

pub fn sha256<S: AsRef<str>>(s: S) -> String {
  let mut hasher = Sha256::new();
  hasher.input_str(s.as_ref());
  hasher.result_str()
}
